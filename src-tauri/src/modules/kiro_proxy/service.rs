use std::sync::{Arc, OnceLock};

use axum::http::HeaderMap;
use serde::Serialize;
use serde_json::Value;
use tauri::Emitter;
use tokio::sync::{oneshot, Mutex, RwLock};

use crate::models::kiro::KiroAccount;
use crate::modules::config::{
    self, KiroProxyApiKey, KiroProxyApiKeyUsage, KiroProxyConfig, KiroProxyModelMappingRule,
};
use crate::modules::kiro_account;
use crate::modules::logger;

use super::account_pool::{AccountPool, PoolAccount};
use super::kiro_api;
use super::stats::{api_key_views, record_api_key_usage, StatsStore};
use super::storage::{
    load_model_cache, load_persisted_stats, load_request_logs, save_model_cache, save_persisted_stats,
    save_request_logs, PersistedStats,
};
use super::types::{
    AddApiKeyInput, ModelCacheState, ProxyAccountView, ProxyAdminLogsResponse,
    ProxyAdminStatsResponse, ProxyAggregateStats, ProxyApiKeyView, ProxyModelView, ProxyRequestLog,
    ProxyStatus, UpdateApiKeyInput,
};

#[derive(Debug)]
struct ServerRuntime {
    shutdown_tx: Option<oneshot::Sender<()>>,
    join: tokio::task::JoinHandle<()>,
}

#[derive(Debug)]
struct RuntimeState {
    config: KiroProxyConfig,
    status: ProxyStatus,
    account_pool: AccountPool,
    stats: StatsStore,
    model_cache: Option<ModelCacheState>,
    server: Option<ServerRuntime>,
}

impl RuntimeState {
    fn new(config: KiroProxyConfig) -> Self {
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
pub struct KiroProxyService {
    state: RwLock<RuntimeState>,
    model_refresh_lock: Mutex<()>,
}

static GLOBAL_PROXY_SERVICE: OnceLock<Arc<KiroProxyService>> = OnceLock::new();

impl KiroProxyService {
    pub fn shared() -> Arc<Self> {
        GLOBAL_PROXY_SERVICE
            .get_or_init(|| {
                let cfg = config::get_user_config().kiro_proxy;
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
            let latest_cfg = config::get_user_config().kiro_proxy;
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
                logger::log_warn(&format!("[Kiro Proxy] 自动启动失败: {}", err));
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
            .map_err(|e| format!("Kiro Proxy 端口监听失败 {}:{} => {}", host, port, e))?;

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
                logger::log_error(&format!("[Kiro Proxy] HTTP 服务异常: {}", err));
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

    pub async fn get_config(&self) -> KiroProxyConfig {
        self.state.read().await.config.clone()
    }

    pub async fn update_config(self: &Arc<Self>, config_patch: KiroProxyConfig) -> Result<KiroProxyConfig, String> {
        let should_restart = self.apply_config_patch(config_patch.clone()).await?;
        if should_restart {
            let _ = self.restart().await?;
        } else {
            self.emit_status_change().await;
        }
        Ok(config_patch)
    }

    pub async fn update_config_without_restart(
        self: &Arc<Self>,
        config_patch: KiroProxyConfig,
    ) -> Result<KiroProxyConfig, String> {
        let _ = self.apply_config_patch(config_patch.clone()).await?;
        self.emit_status_change().await;
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
        // Single-flight guard: avoid duplicate upstream ListAvailableModels requests.
        let _refresh_guard = self.model_refresh_lock.lock().await;
        {
            let state = self.state.read().await;
            if let Some(cache) = state.model_cache.as_ref() {
                let cache_source_ok = cache
                    .models
                    .iter()
                    .all(|model| model.source.eq_ignore_ascii_case("kiro-api"));
                let cache_age = chrono::Utc::now().timestamp() - cache.updated_at;
                if cache_source_ok && cache_age >= 0 && cache_age <= 5 {
                    return Ok(cache.models.clone());
                }
            }
        }

        let (config, accounts) = {
            let mut state = self.state.write().await;
            let cfg = state.config.clone();
            state.account_pool.sync_accounts(&cfg);
            let accounts = state
                .account_pool
                .all_accounts()
                .into_iter()
                .map(|item| item.account)
                .collect::<Vec<_>>();
            (cfg, accounts)
        };

        if accounts.is_empty() {
            return Err("未找到可用 Kiro 账号".to_string());
        }

        let mut models: Vec<ProxyModelView> = Vec::new();
        let mut had_success = false;
        let mut last_error: Option<String> = None;

        for mut account in accounts {
            for attempt in 0..config.max_retries.max(1) {
                match kiro_api::list_available_models(&account).await {
                    Ok(mut dynamic) => {
                        models.append(&mut dynamic);
                        self.mark_account_success(&account.id).await;
                        had_success = true;
                        break;
                    }
                    Err(err) => {
                        last_error = Some(err.clone());
                        let status = parse_upstream_status(&err).unwrap_or(500);
                        if status == 401 || status == 403 {
                            if let Ok(updated) = self.refresh_account_token(&account.id).await {
                                account = updated;
                                continue;
                            }
                            self.mark_account_error(&account.id, false).await;
                            break;
                        }
                        if status == 429 {
                            self.mark_account_error(&account.id, true).await;
                            break;
                        }
                        if status >= 500 && attempt + 1 < config.max_retries.max(1) {
                            self.mark_account_error(&account.id, false).await;
                            tokio::time::sleep(std::time::Duration::from_millis(
                                config.retry_delay_ms.saturating_mul((attempt + 1) as u64),
                            ))
                            .await;
                            continue;
                        }
                        self.mark_account_error(&account.id, false).await;
                        break;
                    }
                }
            }
        }

        if !had_success || models.is_empty() {
            return Err(last_error.unwrap_or_else(|| "获取模型列表失败".to_string()));
        }

        dedup_models(&mut models);

        let cache = ModelCacheState {
            updated_at: chrono::Utc::now().timestamp(),
            models: models.clone(),
        };

        {
            let mut state = self.state.write().await;
            state.model_cache = Some(cache.clone());
        }

        let _ = save_model_cache(&cache);
        Ok(models)
    }

    pub async fn get_models(&self) -> Result<Vec<ProxyModelView>, String> {
        let (cache, ttl) = {
            let state = self.state.read().await;
            (
                state.model_cache.clone(),
                state.config.model_cache_ttl_sec.max(30) as i64,
            )
        };

        let now = chrono::Utc::now().timestamp();
        if let Some(cache) = cache {
            let cache_source_ok = cache
                .models
                .iter()
                .all(|model| model.source.eq_ignore_ascii_case("kiro-api"));
            if now - cache.updated_at <= ttl && cache_source_ok {
                return Ok(cache.models);
            }
        }

        self.refresh_models().await
    }

    pub async fn get_stats(&self) -> ProxyAdminStatsResponse {
        let status = self.status().await;
        let state = self.state.read().await;
        state
            .stats
            .snapshot(status, state.account_pool.views())
    }

    pub async fn get_logs(&self, limit: Option<usize>) -> ProxyAdminLogsResponse {
        let state = self.state.read().await;
        ProxyAdminLogsResponse {
            logs: state.stats.logs(limit),
        }
    }

    pub async fn clear_logs(&self) -> Result<(), String> {
        {
            let mut state = self.state.write().await;
            state.stats.clear_logs();
            save_request_logs(&state.stats.all_logs())?;
        }

        Ok(())
    }

    pub async fn reset_stats(&self) -> Result<(), String> {
        {
            let mut state = self.state.write().await;
            state.stats.reset();
            save_persisted_stats(&PersistedStats {
                aggregate: state.stats.aggregate(),
            })?;
        }
        Ok(())
    }

    pub async fn get_api_keys(&self) -> Vec<ProxyApiKeyView> {
        let state = self.state.read().await;
        api_key_views(&state.config)
    }

    pub async fn add_api_key(self: &Arc<Self>, input: AddApiKeyInput) -> Result<KiroProxyApiKey, String> {
        let key = KiroProxyApiKey {
            id: uuid::Uuid::new_v4().to_string(),
            name: if input.name.trim().is_empty() {
                "API Key".to_string()
            } else {
                input.name.trim().to_string()
            },
            key: input.key.trim().to_string(),
            enabled: input.enabled.unwrap_or(true),
            created_at: chrono::Utc::now().timestamp(),
            last_used_at: None,
            credits_limit: input.credits_limit,
            usage: KiroProxyApiKeyUsage::default(),
            usage_history: Vec::new(),
        };

        if key.key.is_empty() {
            return Err("API Key 不能为空".to_string());
        }

        {
            let mut state = self.state.write().await;
            if state.config.api_keys.iter().any(|item| item.key == key.key) {
                return Err("API Key 已存在".to_string());
            }
            state.config.api_keys.push(key.clone());
            self.persist_config_locked(&state.config)?;
        }

        Ok(key)
    }

    pub async fn update_api_key(
        self: &Arc<Self>,
        input: UpdateApiKeyInput,
    ) -> Result<KiroProxyApiKey, String> {
        let updated = {
            let mut state = self.state.write().await;
            let entry = state
                .config
                .api_keys
                .iter_mut()
                .find(|item| item.id == input.id)
                .ok_or_else(|| "API Key 不存在".to_string())?;

            if let Some(name) = input.name {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    entry.name = trimmed.to_string();
                }
            }
            if let Some(enabled) = input.enabled {
                entry.enabled = enabled;
            }
            if input.credits_limit.is_some() {
                entry.credits_limit = input.credits_limit;
            }

            let cloned = entry.clone();
            self.persist_config_locked(&state.config)?;
            cloned
        };

        Ok(updated)
    }

    pub async fn delete_api_key(self: &Arc<Self>, id: &str) -> Result<(), String> {
        {
            let mut state = self.state.write().await;
            let before = state.config.api_keys.len();
            state.config.api_keys.retain(|item| item.id != id);
            if state.config.api_keys.len() == before {
                return Err("API Key 不存在".to_string());
            }
            self.persist_config_locked(&state.config)?;
        }

        Ok(())
    }

    pub async fn reset_api_key_usage(self: &Arc<Self>, id: &str) -> Result<(), String> {
        {
            let mut state = self.state.write().await;
            let entry = state
                .config
                .api_keys
                .iter_mut()
                .find(|item| item.id == id)
                .ok_or_else(|| "API Key 不存在".to_string())?;
            entry.usage = KiroProxyApiKeyUsage::default();
            entry.usage_history.clear();
            self.persist_config_locked(&state.config)?;
        }

        Ok(())
    }

    pub async fn take_next_account(&self) -> Option<PoolAccount> {
        let mut state = self.state.write().await;
        let cfg = state.config.clone();
        state.account_pool.next_account(&cfg)
    }

    pub async fn refresh_account_token(&self, account_id: &str) -> Result<KiroAccount, String> {
        let refreshed = kiro_account::refresh_account_token(account_id).await?;
        {
            let mut state = self.state.write().await;
            state.account_pool.update_account(refreshed.clone());
        }
        Ok(refreshed)
    }

    pub async fn mark_account_success(&self, account_id: &str) {
        let mut state = self.state.write().await;
        state.account_pool.record_success(account_id);
    }

    pub async fn mark_account_error(&self, account_id: &str, quota_error: bool) {
        let mut state = self.state.write().await;
        state.account_pool.record_error(account_id, quota_error);
    }

    pub async fn current_config_snapshot(&self) -> KiroProxyConfig {
        self.state.read().await.config.clone()
    }

    pub async fn apply_api_key_usage(
        self: &Arc<Self>,
        api_key_id: Option<&str>,
        credits: f64,
        input_tokens: u64,
        output_tokens: u64,
        model: Option<&str>,
        path: &str,
    ) -> Result<(), String> {
        if let Some(api_key_id) = api_key_id {
            let mut state = self.state.write().await;
            record_api_key_usage(
                &mut state.config,
                api_key_id,
                credits,
                input_tokens,
                output_tokens,
                model,
                path,
            );
            self.persist_config_locked(&state.config)?;
        }
        Ok(())
    }

    pub async fn record_request_log(
        self: &Arc<Self>,
        log: ProxyRequestLog,
        event_payload: Option<Value>,
    ) -> Result<(), String> {
        {
            let mut state = self.state.write().await;
            state.stats.record(log.clone());
            save_persisted_stats(&PersistedStats {
                aggregate: state.stats.aggregate(),
            })?;
            save_request_logs(&state.stats.all_logs())?;
        }

        if let Some(payload) = event_payload {
            self.emit_event("kiro-proxy:response", payload);
        }

        Ok(())
    }

    pub async fn emit_request_event(&self, payload: Value) {
        self.emit_event("kiro-proxy:request", payload);
    }

    pub async fn persist_stats(&self) {
        let state = self.state.read().await;
        let _ = save_persisted_stats(&PersistedStats {
            aggregate: state.stats.aggregate(),
        });
        let _ = save_request_logs(&state.stats.all_logs());
    }

    fn emit_event<T: Serialize + Clone>(&self, event: &str, payload: T) {
        if let Some(app_handle) = crate::get_app_handle() {
            let _ = app_handle.emit(event, payload);
        }
    }

    async fn emit_status_change(&self) {
        let status = self.status().await;
        self.emit_event("kiro-proxy:status-change", status);
    }

    fn persist_config_locked(&self, new_proxy_cfg: &KiroProxyConfig) -> Result<(), String> {
        let mut user = config::get_user_config();
        user.kiro_proxy = new_proxy_cfg.clone();
        config::save_user_config(&user)
    }

    async fn apply_config_patch(&self, config_patch: KiroProxyConfig) -> Result<bool, String> {
        let should_restart = {
            let mut state = self.state.write().await;
            state.config = config_patch.clone();
            state.status.host = state.config.host.clone();
            state.status.port = state.config.port;
            state.status.running
        };

        let mut user = config::get_user_config();
        user.kiro_proxy = config_patch;
        config::save_user_config(&user)?;
        Ok(should_restart)
    }

    pub async fn model_mapping_for(
        &self,
        model: &str,
        api_key_id: Option<&str>,
    ) -> String {
        let config = self.current_config_snapshot().await;
        apply_model_mapping(&config.model_mappings, model, api_key_id)
    }

    pub async fn verify_admin_auth(&self, headers: &HeaderMap) -> super::auth::ApiKeyAuthResult {
        let config = self.current_config_snapshot().await;
        super::auth::validate_api_key(&config, headers)
    }

    pub async fn should_authenticate_path(&self, path: &str) -> bool {
        !(path == "/" || path == "/health")
    }

    pub async fn aggregate_snapshot(&self) -> ProxyAggregateStats {
        self.state.read().await.stats.aggregate()
    }
}

fn dedup_models(models: &mut Vec<ProxyModelView>) {
    let mut seen = std::collections::HashSet::<String>::new();
    models.retain(|model| seen.insert(model.id.clone()));
}

fn wildcard_match(pattern: &str, input: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if !pattern.contains('*') {
        return pattern.eq_ignore_ascii_case(input);
    }

    let escaped = regex::escape(pattern).replace(r"\*", ".*");
    let regex = format!("(?i)^{}$", escaped);
    regex::Regex::new(&regex)
        .map(|re| re.is_match(input))
        .unwrap_or(false)
}

fn apply_model_mapping(
    mappings: &[KiroProxyModelMappingRule],
    requested_model: &str,
    api_key_id: Option<&str>,
) -> String {
    if mappings.is_empty() {
        return requested_model.to_string();
    }

    let mut sorted = mappings.to_vec();
    sorted.sort_by_key(|rule| rule.priority);

    for rule in sorted {
        if !rule.enabled {
            continue;
        }

        if !rule.api_key_ids.is_empty() {
            let Some(api_key_id) = api_key_id else {
                continue;
            };
            if !rule.api_key_ids.iter().any(|id| id == api_key_id) {
                continue;
            }
        }

        if !wildcard_match(&rule.source_model, requested_model) {
            continue;
        }

        let mut targets: Vec<String> = rule
            .target_models
            .into_iter()
            .filter(|item| !item.trim().is_empty())
            .collect();
        if targets.is_empty() {
            continue;
        }

        if rule.mapping_type.eq_ignore_ascii_case("loadbalance") && targets.len() > 1 {
            if !rule.weights.is_empty() {
                let total_weight: f64 = rule.weights.iter().sum();
                if total_weight > 0.0 {
                    let mut cursor = rand::random::<f64>() * total_weight;
                    for (idx, weight) in rule.weights.iter().enumerate() {
                        cursor -= *weight;
                        if cursor <= 0.0 {
                            return targets.get(idx).cloned().unwrap_or_else(|| targets[0].clone());
                        }
                    }
                }
            }

            let idx = rand::random::<usize>() % targets.len();
            return targets.swap_remove(idx);
        }

        return targets.remove(0);
    }

    requested_model.to_string()
}

pub async fn service() -> Arc<KiroProxyService> {
    KiroProxyService::shared()
}

fn parse_upstream_status(error: &str) -> Option<u16> {
    let prefix = "UPSTREAM_STATUS:";
    let rest = error.strip_prefix(prefix)?;
    let status_text = rest.split(':').next()?;
    status_text.parse::<u16>().ok()
}
