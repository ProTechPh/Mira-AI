use std::sync::{Arc, OnceLock};

use axum::http::HeaderMap;
use tauri::Emitter;
use tokio::sync::{oneshot, Mutex, RwLock};

use crate::models::Account;
use crate::modules::account;
use crate::modules::config::{self, AntigravityProxyConfig};
use crate::modules::logger;
use crate::modules::oauth;

use super::account_pool::{AccountPool, PoolAccount};
use super::stats::StatsStore;
use super::storage::{
    load_model_cache, load_persisted_stats, load_request_logs, save_model_cache, save_persisted_stats,
    save_request_logs, PersistedStats,
};
use super::types::{
    ModelCacheState, ProxyAccountView, ProxyAdminLogsResponse, ProxyAdminStatsResponse,
    ProxyModelView, ProxyRequestLog, ProxyStatus,
};

#[derive(Debug)]
struct ServerRuntime {
    shutdown_tx: Option<oneshot::Sender<()>>,
    join: tokio::task::JoinHandle<()>,
}

#[derive(Debug)]
struct RuntimeState {
    config: AntigravityProxyConfig,
    status: ProxyStatus,
    account_pool: AccountPool,
    stats: StatsStore,
    model_cache: Option<ModelCacheState>,
    server: Option<ServerRuntime>,
}

impl RuntimeState {
    fn new(config: AntigravityProxyConfig) -> Self {
        Self {
            status: ProxyStatus {
                host: config.host.clone(),
                port: config.port,
                ..ProxyStatus::default()
            },
            config,
            account_pool: AccountPool::new(),
            stats: StatsStore::default(),
            model_cache: None,
            server: None,
        }
    }
}

#[derive(Debug)]
pub struct AntigravityProxyService {
    state: RwLock<RuntimeState>,
    model_refresh_lock: Mutex<()>,
}

static GLOBAL_PROXY_SERVICE: OnceLock<Arc<AntigravityProxyService>> = OnceLock::new();

impl AntigravityProxyService {
    pub fn shared() -> Arc<Self> {
        GLOBAL_PROXY_SERVICE
            .get_or_init(|| {
                let cfg = config::get_user_config().antigravity_proxy;
                Arc::new(Self {
                    state: RwLock::new(RuntimeState::new(cfg)),
                    model_refresh_lock: Mutex::new(()),
                })
            })
            .clone()
    }

    pub async fn init(self: &Arc<Self>) -> Result<(), String> {
        {
            let mut state = self.state.write().await;
            let latest_cfg = config::get_user_config().antigravity_proxy;
            state.config = latest_cfg;
            state.status.host = state.config.host.clone();
            state.status.port = state.config.port;

            let persisted = load_persisted_stats().unwrap_or_default();
            let logs = load_request_logs().unwrap_or_default();
            state.stats = StatsStore::from_state(persisted.aggregate, logs);
            state.model_cache = load_model_cache().unwrap_or(None);
            let cfg = state.config.clone();
            state.account_pool.sync_accounts(&cfg);
        }
        Ok(())
    }

    pub async fn maybe_auto_start(self: &Arc<Self>) {
        let auto_start = {
            let state = self.state.read().await;
            state.config.auto_start && state.config.enabled
        };

        if auto_start {
            if let Err(err) = self.start().await {
                logger::log_warn(&format!("[Antigravity Proxy] 自动启动失败: {}", err));
            }
        }
    }

    pub async fn start(self: &Arc<Self>) -> Result<ProxyStatus, String> {
        let (host, port) = {
            let state = self.state.read().await;
            if state.status.running {
                return Ok(state.status.clone());
            }
            (state.config.host.clone(), state.config.port)
        };

        let listener = tokio::net::TcpListener::bind((host.as_str(), port))
            .await
            .map_err(|e| format!("Antigravity Proxy 端口监听失败 {}:{} => {}", host, port, e))?;

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let app_service = Arc::clone(self);

        {
            let mut state = self.state.write().await;
            state.status.running = true;
            state.status.error = None;
            state.status.host = host.clone();
            state.status.port = port;
            state.status.started_at = Some(chrono::Utc::now().timestamp());
            let cfg = state.config.clone();
            state.account_pool.sync_accounts(&cfg);
        }

        self.emit_status_change().await;

        let join = tokio::spawn(async move {
            if let Err(err) = super::server::serve(listener, app_service, shutdown_rx).await {
                logger::log_error(&format!("[Antigravity Proxy] HTTP 服务异常: {}", err));
            }
        });

        {
            let mut state = self.state.write().await;
            state.server = Some(ServerRuntime {
                shutdown_tx: Some(shutdown_tx),
                join,
            });
        }

        Ok(self.status().await)
    }

    pub async fn stop(self: &Arc<Self>) -> Result<ProxyStatus, String> {
        let server = {
            let mut state = self.state.write().await;
            if !state.status.running {
                return Ok(state.status.clone());
            }
            state.status.running = false;
            state.server.take()
        };

        if let Some(mut server_runtime) = server {
            if let Some(tx) = server_runtime.shutdown_tx.take() {
                let _ = tx.send(());
            }
            let _ = server_runtime.join.await;
        }

        self.persist_stats().await;
        self.emit_status_change().await;
        Ok(self.status().await)
    }

    pub async fn restart(self: &Arc<Self>) -> Result<ProxyStatus, String> {
        let _ = self.stop().await;
        self.start().await
    }

    pub async fn status(&self) -> ProxyStatus {
        let mut status = {
            let state = self.state.read().await;
            state.status.clone()
        };

        if let Some(started_at) = status.started_at {
            if status.running {
                status.uptime_seconds = Some((chrono::Utc::now().timestamp() - started_at).max(0));
            }
        }

        status
    }

    pub async fn get_config(&self) -> AntigravityProxyConfig {
        self.state.read().await.config.clone()
    }

    pub async fn update_config(
        self: &Arc<Self>,
        config_patch: AntigravityProxyConfig,
    ) -> Result<AntigravityProxyConfig, String> {
        let should_restart = self.apply_config_patch(config_patch.clone()).await?;
        if should_restart {
            let _ = self.restart().await?;
        } else {
            self.emit_status_change().await;
        }
        Ok(config_patch)
    }

    pub async fn sync_accounts(&self) -> Vec<ProxyAccountView> {
        let mut state = self.state.write().await;
        let cfg = state.config.clone();
        state.account_pool.sync_accounts(&cfg);
        state.account_pool.views()
    }

    pub async fn get_accounts(&self) -> Vec<ProxyAccountView> {
        self.state.read().await.account_pool.views()
    }

    pub async fn refresh_models(&self) -> Result<Vec<ProxyModelView>, String> {
        let _refresh_guard = self.model_refresh_lock.lock().await;

        let stale_cache: Option<ModelCacheState>;
        {
            let state = self.state.read().await;
            stale_cache = state.model_cache.clone();
            if let Some(cache) = state.model_cache.as_ref() {
                let ttl = model_cache_ttl_sec(&state.config);
                let age = chrono::Utc::now().timestamp() - cache.updated_at;
                if age >= 0 && age <= ttl {
                    return Ok(cache.models.clone());
                }
            }
        }

        let account = {
            let mut state = self.state.write().await;
            let cfg = state.config.clone();
            state.account_pool.sync_accounts(&cfg);
            state
                .account_pool
                .next_account(&cfg, account::get_current_account_id().ok().flatten().as_deref())
        }
        .ok_or_else(|| "未找到可用 Antigravity 账号".to_string())?;

        let mut acc = account.account;

        let models = match super::api::fetch_models(&acc).await {
            Ok(models) => models,
            Err(err) => {
                let status_code = parse_upstream_status(&err);
                if matches!(status_code, Some(401 | 403)) {
                    if let Ok(updated) = self.refresh_account_token(&acc.id).await {
                        acc = updated;
                        match super::api::fetch_models(&acc).await {
                            Ok(models) => models,
                            Err(refresh_err) => {
                                if let Some(cache) = stale_cache.as_ref() {
                                    self.touch_model_cache(cache.models.clone()).await;
                                    return Ok(cache.models.clone());
                                }
                                return Err(refresh_err);
                            }
                        }
                    } else {
                        if let Some(cache) = stale_cache.as_ref() {
                            self.touch_model_cache(cache.models.clone()).await;
                            return Ok(cache.models.clone());
                        }
                        return Err(err);
                    }
                } else {
                    if let Some(cache) = stale_cache.as_ref() {
                        self.touch_model_cache(cache.models.clone()).await;
                        return Ok(cache.models.clone());
                    }
                    return Err(err);
                }
            }
        };

        {
            let mut state = self.state.write().await;
            let cache = ModelCacheState {
                updated_at: chrono::Utc::now().timestamp(),
                models: models.clone(),
            };
            state.model_cache = Some(cache.clone());
            let _ = save_model_cache(&cache);
        }

        Ok(models)
    }

    pub async fn get_models(&self) -> Result<Vec<ProxyModelView>, String> {
        {
            let state = self.state.read().await;
            if let Some(cache) = state.model_cache.as_ref() {
                let ttl = model_cache_ttl_sec(&state.config);
                let age = chrono::Utc::now().timestamp() - cache.updated_at;
                if age >= 0 && age <= ttl {
                    return Ok(cache.models.clone());
                }
            }
        }
        self.refresh_models().await
    }

    pub async fn get_stats(&self) -> ProxyAdminStatsResponse {
        let state = self.state.read().await;
        state
            .stats
            .snapshot(state.status.clone(), state.account_pool.views())
    }

    pub async fn get_logs(&self, limit: Option<usize>) -> ProxyAdminLogsResponse {
        let state = self.state.read().await;
        ProxyAdminLogsResponse {
            logs: state.stats.logs(limit),
        }
    }

    pub async fn clear_logs(&self) -> Result<(), String> {
        let mut state = self.state.write().await;
        state.stats.clear_logs();
        let _ = save_request_logs(&state.stats.all_logs());
        Ok(())
    }

    pub async fn reset_stats(&self) -> Result<(), String> {
        let mut state = self.state.write().await;
        state.stats.reset();
        let _ = save_persisted_stats(&PersistedStats {
            aggregate: state.stats.aggregate(),
        });
        Ok(())
    }

    pub async fn take_next_account(&self) -> Option<PoolAccount> {
        let mut state = self.state.write().await;
        let cfg = state.config.clone();
        state.account_pool.sync_accounts(&cfg);
        let current_id = account::get_current_account_id().ok().flatten();
        state.account_pool.next_account(&cfg, current_id.as_deref())
    }

    pub async fn mark_account_success(&self, account_id: &str) {
        let mut state = self.state.write().await;
        state.account_pool.record_success(account_id);
    }

    pub async fn mark_account_error(&self, account_id: &str, quota_error: bool) {
        let mut state = self.state.write().await;
        state.account_pool.record_error(account_id, quota_error);
    }

    pub async fn refresh_account_token(&self, account_id: &str) -> Result<Account, String> {
        let mut account_data = account::load_account(account_id)?;
        let current_token = account_data.token.clone();
        let threshold = {
            let state = self.state.read().await;
            state.config.token_refresh_before_expiry_sec
        } as i64;
        let now = chrono::Utc::now().timestamp();

        let refreshed = if current_token.expiry_timestamp > now + threshold {
            current_token
        } else {
            oauth::ensure_fresh_token(&current_token).await?
        };

        if refreshed.access_token != account_data.token.access_token
            || refreshed.expiry_timestamp != account_data.token.expiry_timestamp
        {
            account_data.token = refreshed;
            account::save_account(&account_data)?;
        }

        {
            let mut state = self.state.write().await;
            state.account_pool.update_account(account_data.clone());
        }

        Ok(account_data)
    }

    pub async fn verify_auth(&self, headers: &HeaderMap) -> super::auth::ApiKeyAuthResult {
        let config = self.get_config().await;
        super::auth::validate_api_key(&config, headers)
    }

    pub async fn record_request_log(
        &self,
        log: ProxyRequestLog,
        event_payload: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let mut state = self.state.write().await;
        state.stats.record(log);

        state.status.request_count = state.stats.aggregate.total_requests;
        state.status.success_count = state.stats.aggregate.success_requests;
        state.status.failed_count = state.stats.aggregate.failed_requests;
        state.status.total_input_tokens = state.stats.aggregate.total_input_tokens;
        state.status.total_output_tokens = state.stats.aggregate.total_output_tokens;

        let aggregate = state.stats.aggregate();
        let logs = state.stats.all_logs();
        drop(state);

        let _ = save_persisted_stats(&PersistedStats { aggregate });
        let _ = save_request_logs(&logs);

        if let Some(payload) = event_payload {
            if let Some(app) = crate::APP_HANDLE.get() {
                let _ = app.emit("antigravity-proxy://request-log", payload);
            }
        }

        Ok(())
    }

    pub async fn current_config_snapshot(&self) -> AntigravityProxyConfig {
        self.state.read().await.config.clone()
    }

    pub async fn persist_stats(&self) {
        let state = self.state.read().await;
        let _ = save_persisted_stats(&PersistedStats {
            aggregate: state.stats.aggregate(),
        });
        let _ = save_request_logs(&state.stats.all_logs());
    }

    async fn apply_config_patch(
        self: &Arc<Self>,
        config_patch: AntigravityProxyConfig,
    ) -> Result<bool, String> {
        let mut state = self.state.write().await;
        let should_restart = state.status.running
            && (state.config.host != config_patch.host || state.config.port != config_patch.port);

        state.config = config_patch.clone();
        state.status.host = config_patch.host.clone();
        state.status.port = config_patch.port;
        state.account_pool.sync_accounts(&config_patch);

        drop(state);
        let _ = self.persist_config(config_patch).await?;
        Ok(should_restart)
    }

    async fn persist_config(&self, new_proxy_cfg: AntigravityProxyConfig) -> Result<bool, String> {
        let mut user = config::get_user_config();
        let changed = user.antigravity_proxy != new_proxy_cfg;
        if changed {
            user.antigravity_proxy = new_proxy_cfg;
            config::save_user_config(&user)?;
        }
        Ok(changed)
    }

    async fn touch_model_cache(&self, models: Vec<ProxyModelView>) {
        let cache = ModelCacheState {
            updated_at: chrono::Utc::now().timestamp(),
            models,
        };
        {
            let mut state = self.state.write().await;
            state.model_cache = Some(cache.clone());
        }
        let _ = save_model_cache(&cache);
    }

    async fn emit_status_change(&self) {
        if let Some(app) = crate::APP_HANDLE.get() {
            let _ = app.emit("antigravity-proxy://status", self.status().await);
        }
    }
}

pub async fn service() -> Arc<AntigravityProxyService> {
    AntigravityProxyService::shared()
}

fn parse_upstream_status(error: &str) -> Option<u16> {
    let prefix = "UPSTREAM_STATUS:";
    let rest = error.strip_prefix(prefix)?;
    let status_text = rest.split(':').next()?;
    status_text.parse::<u16>().ok()
}

fn model_cache_ttl_sec(config: &AntigravityProxyConfig) -> i64 {
    config.model_cache_ttl_sec.max(30) as i64
}
