//! 配置服务模块
//! 管理应用配置，包括 WebSocket 端口等

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};
use sys_locale::get_locale;

/// 默认 WebSocket 端口
pub const DEFAULT_WS_PORT: u16 = 19528;

/// 端口尝试范围（从配置端口开始，最多尝试 100 个）
pub const PORT_RANGE: u16 = 100;

/// 服务状态配置文件名（供外部客户端读取）
const SERVER_STATUS_FILE: &str = "server.json";

/// 用户配置文件名
const USER_CONFIG_FILE: &str = "config.json";

/// 数据目录名
const DATA_DIR: &str = ".antigravity_mira";

/// 服务状态（写入共享文件供其他客户端读取）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    /// WebSocket 服务端口（实际绑定的端口）
    pub ws_port: u16,
    /// 服务版本
    pub version: String,
    /// 进程 ID（用于检测服务是否存活）
    pub pid: u32,
    /// 启动时间戳
    pub started_at: i64,
}

/// 用户配置（持久化存储）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    /// WebSocket 服务是否启用
    #[serde(default = "default_ws_enabled")]
    pub ws_enabled: bool,
    /// WebSocket 首选端口（用户配置的，实际可能不同）
    #[serde(default = "default_ws_port")]
    pub ws_port: u16,
    /// 界面语言
    #[serde(default = "default_language")]
    pub language: String,
    /// 应用主题
    #[serde(default = "default_theme")]
    pub theme: String,
    /// 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_auto_refresh")]
    pub auto_refresh_minutes: i32,
    /// Codex 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_codex_auto_refresh")]
    pub codex_auto_refresh_minutes: i32,
    /// GitHub Copilot 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_ghcp_auto_refresh")]
    pub ghcp_auto_refresh_minutes: i32,
    /// Windsurf 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_windsurf_auto_refresh")]
    pub windsurf_auto_refresh_minutes: i32,
    /// Kiro 自动刷新间隔（分钟），-1 表示禁用
    #[serde(default = "default_kiro_auto_refresh")]
    pub kiro_auto_refresh_minutes: i32,
    /// 窗口关闭行为
    #[serde(default = "default_close_behavior")]
    pub close_behavior: CloseWindowBehavior,
    /// OpenCode 启动路径（为空则使用默认路径）
    #[serde(default = "default_opencode_app_path")]
    pub opencode_app_path: String,
    /// Antigravity 启动路径（为空则使用默认路径）
    #[serde(default = "default_antigravity_app_path")]
    pub antigravity_app_path: String,
    /// Codex 启动路径（为空则使用默认路径）
    #[serde(default = "default_codex_app_path")]
    pub codex_app_path: String,
    /// VS Code 启动路径（为空则使用默认路径）
    #[serde(default = "default_vscode_app_path")]
    pub vscode_app_path: String,
    /// Windsurf 启动路径（为空则使用默认路径）
    #[serde(default = "default_windsurf_app_path")]
    pub windsurf_app_path: String,
    /// Kiro 启动路径（为空则使用默认路径）
    #[serde(default = "default_kiro_app_path")]
    pub kiro_app_path: String,
    /// 切换 Codex 时是否自动重启 OpenCode
    #[serde(default = "default_opencode_sync_on_switch")]
    pub opencode_sync_on_switch: bool,
    /// 切换 Codex 时是否自动启动/重启 Codex App
    #[serde(default = "default_codex_launch_on_switch")]
    pub codex_launch_on_switch: bool,
    /// 是否启用自动切号
    #[serde(default = "default_auto_switch_enabled")]
    pub auto_switch_enabled: bool,
    /// 自动切号阈值（百分比），任意模型配额低于此值触发
    #[serde(default = "default_auto_switch_threshold")]
    pub auto_switch_threshold: i32,
    /// 是否启用配额预警通知
    #[serde(default = "default_quota_alert_enabled")]
    pub quota_alert_enabled: bool,
    /// 配额预警阈值（百分比），任意模型配额低于此值触发
    #[serde(default = "default_quota_alert_threshold")]
    pub quota_alert_threshold: i32,
    /// 是否启用 Codex 配额预警通知
    #[serde(default = "default_codex_quota_alert_enabled")]
    pub codex_quota_alert_enabled: bool,
    /// Codex 配额预警阈值（百分比）
    #[serde(default = "default_codex_quota_alert_threshold")]
    pub codex_quota_alert_threshold: i32,
    /// 是否启用 GitHub Copilot 配额预警通知
    #[serde(default = "default_ghcp_quota_alert_enabled")]
    pub ghcp_quota_alert_enabled: bool,
    /// GitHub Copilot 配额预警阈值（百分比）
    #[serde(default = "default_ghcp_quota_alert_threshold")]
    pub ghcp_quota_alert_threshold: i32,
    /// 是否启用 Windsurf 配额预警通知
    #[serde(default = "default_windsurf_quota_alert_enabled")]
    pub windsurf_quota_alert_enabled: bool,
    /// Windsurf 配额预警阈值（百分比）
    #[serde(default = "default_windsurf_quota_alert_threshold")]
    pub windsurf_quota_alert_threshold: i32,
    /// 是否启用 Kiro 配额预警通知
    #[serde(default = "default_kiro_quota_alert_enabled")]
    pub kiro_quota_alert_enabled: bool,
    /// Kiro 配额预警阈值（百分比）
    #[serde(default = "default_kiro_quota_alert_threshold")]
    pub kiro_quota_alert_threshold: i32,
    /// Kiro API 代理配置
    #[serde(default)]
    pub kiro_proxy: KiroProxyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KiroProxyApiKeyUsageDaily {
    #[serde(default)]
    pub requests: u64,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub credits: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KiroProxyApiKeyUsageModel {
    #[serde(default)]
    pub requests: u64,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub credits: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KiroProxyApiKeyUsage {
    #[serde(default)]
    pub total_requests: u64,
    #[serde(default)]
    pub total_input_tokens: u64,
    #[serde(default)]
    pub total_output_tokens: u64,
    #[serde(default)]
    pub total_credits: f64,
    #[serde(default)]
    pub daily: HashMap<String, KiroProxyApiKeyUsageDaily>,
    #[serde(default)]
    pub by_model: HashMap<String, KiroProxyApiKeyUsageModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KiroProxyApiKey {
    pub id: String,
    pub name: String,
    pub key: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_timestamp_now")]
    pub created_at: i64,
    #[serde(default)]
    pub last_used_at: Option<i64>,
    #[serde(default)]
    pub credits_limit: Option<f64>,
    #[serde(default)]
    pub usage: KiroProxyApiKeyUsage,
    #[serde(default)]
    pub usage_history: Vec<crate::modules::kiro_proxy::types::ProxyUsageRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KiroProxyModelMappingRule {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(
        rename = "type",
        alias = "mappingType",
        default = "default_model_mapping_type"
    )]
    pub mapping_type: String,
    pub source_model: String,
    #[serde(default)]
    pub target_models: Vec<String>,
    #[serde(default)]
    pub weights: Vec<f64>,
    #[serde(default = "default_model_mapping_priority")]
    pub priority: i32,
    #[serde(default)]
    pub api_key_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KiroProxyConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default = "default_proxy_host")]
    pub host: String,
    #[serde(default = "default_proxy_port")]
    pub port: u16,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_keys: Vec<KiroProxyApiKey>,
    #[serde(default = "default_true")]
    pub enable_multi_account: bool,
    #[serde(default)]
    pub selected_account_ids: Vec<String>,
    #[serde(default = "default_true")]
    pub log_requests: bool,
    #[serde(default = "default_proxy_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_proxy_retry_delay_ms")]
    pub retry_delay_ms: u64,
    #[serde(default = "default_thinking_output_format")]
    pub thinking_output_format: String,
    #[serde(default)]
    pub auto_continue_rounds: u32,
    #[serde(default)]
    pub disable_tools: bool,
    #[serde(default)]
    pub preferred_endpoint: Option<String>,
    #[serde(default = "default_proxy_model_cache_ttl_sec")]
    pub model_cache_ttl_sec: u64,
    #[serde(default = "default_proxy_token_refresh_before_expiry_sec")]
    pub token_refresh_before_expiry_sec: u64,
    #[serde(default)]
    pub auto_switch_on_quota_exhausted: bool,
    #[serde(default)]
    pub model_mappings: Vec<KiroProxyModelMappingRule>,
}

fn default_proxy_host() -> String {
    "127.0.0.1".to_string()
}

fn default_proxy_port() -> u16 {
    5580
}

fn default_proxy_max_retries() -> u32 {
    3
}

fn default_proxy_retry_delay_ms() -> u64 {
    1000
}

fn default_thinking_output_format() -> String {
    "reasoning_content".to_string()
}

fn default_proxy_model_cache_ttl_sec() -> u64 {
    300
}

fn default_proxy_token_refresh_before_expiry_sec() -> u64 {
    300
}

fn default_true() -> bool {
    true
}

fn default_timestamp_now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn default_model_mapping_type() -> String {
    "replace".to_string()
}

fn default_model_mapping_priority() -> i32 {
    100
}

impl Default for KiroProxyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_start: false,
            host: default_proxy_host(),
            port: default_proxy_port(),
            api_key: None,
            api_keys: Vec::new(),
            enable_multi_account: true,
            selected_account_ids: Vec::new(),
            log_requests: true,
            max_retries: default_proxy_max_retries(),
            retry_delay_ms: default_proxy_retry_delay_ms(),
            thinking_output_format: default_thinking_output_format(),
            auto_continue_rounds: 0,
            disable_tools: false,
            preferred_endpoint: None,
            model_cache_ttl_sec: default_proxy_model_cache_ttl_sec(),
            token_refresh_before_expiry_sec: default_proxy_token_refresh_before_expiry_sec(),
            auto_switch_on_quota_exhausted: false,
            model_mappings: Vec::new(),
        }
    }
}

/// 窗口关闭行为
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CloseWindowBehavior {
    /// 每次询问
    Ask,
    /// 最小化到托盘
    Minimize,
    /// 退出应用
    Quit,
}

impl Default for CloseWindowBehavior {
    fn default() -> Self {
        CloseWindowBehavior::Ask
    }
}

fn default_ws_enabled() -> bool {
    true
}
fn default_ws_port() -> u16 {
    DEFAULT_WS_PORT
}

fn normalize_language_candidate(candidate: &str) -> String {
    let normalized = candidate.trim().replace('_', "-").to_lowercase();
    if normalized.is_empty() {
        return "en".to_string();
    }

    if normalized.starts_with("zh-hant")
        || normalized.starts_with("zh-tw")
        || normalized.starts_with("zh-hk")
        || normalized.starts_with("zh-mo")
    {
        return "zh-tw".to_string();
    }
    if normalized.starts_with("zh") {
        return "zh-cn".to_string();
    }
    if normalized.starts_with("pt-br") || normalized == "pt" {
        return "pt-br".to_string();
    }
    if normalized.starts_with("en") {
        return "en".to_string();
    }
    if normalized.starts_with("ja") {
        return "ja".to_string();
    }
    if normalized.starts_with("es") {
        return "es".to_string();
    }
    if normalized.starts_with("de") {
        return "de".to_string();
    }
    if normalized.starts_with("fr") {
        return "fr".to_string();
    }
    if normalized.starts_with("ru") {
        return "ru".to_string();
    }
    if normalized.starts_with("ko") {
        return "ko".to_string();
    }
    if normalized.starts_with("it") {
        return "it".to_string();
    }
    if normalized.starts_with("tr") {
        return "tr".to_string();
    }
    if normalized.starts_with("pl") {
        return "pl".to_string();
    }
    if normalized.starts_with("cs") {
        return "cs".to_string();
    }
    if normalized.starts_with("vi") {
        return "vi".to_string();
    }
    if normalized.starts_with("ar") {
        return "ar".to_string();
    }

    "en".to_string()
}

fn default_language() -> String {
    get_locale()
        .map(|locale| normalize_language_candidate(&locale))
        .unwrap_or_else(|| "en".to_string())
}
fn default_theme() -> String {
    "system".to_string()
}
fn default_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_codex_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_ghcp_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_windsurf_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_kiro_auto_refresh() -> i32 {
    10
} // 默认 10 分钟
fn default_close_behavior() -> CloseWindowBehavior {
    CloseWindowBehavior::Ask
}
fn default_opencode_app_path() -> String {
    String::new()
}
fn default_antigravity_app_path() -> String {
    String::new()
}
fn default_codex_app_path() -> String {
    String::new()
}
fn default_vscode_app_path() -> String {
    String::new()
}
fn default_windsurf_app_path() -> String {
    String::new()
}
fn default_kiro_app_path() -> String {
    String::new()
}
fn default_opencode_sync_on_switch() -> bool {
    true
}
fn default_codex_launch_on_switch() -> bool {
    true
}
fn default_auto_switch_enabled() -> bool {
    false
}
fn default_auto_switch_threshold() -> i32 {
    5
}
fn default_quota_alert_enabled() -> bool {
    false
}
fn default_quota_alert_threshold() -> i32 {
    20
}
fn default_codex_quota_alert_enabled() -> bool {
    false
}
fn default_codex_quota_alert_threshold() -> i32 {
    20
}
fn default_ghcp_quota_alert_enabled() -> bool {
    false
}
fn default_ghcp_quota_alert_threshold() -> i32 {
    20
}
fn default_windsurf_quota_alert_enabled() -> bool {
    false
}
fn default_windsurf_quota_alert_threshold() -> i32 {
    20
}
fn default_kiro_quota_alert_enabled() -> bool {
    false
}
fn default_kiro_quota_alert_threshold() -> i32 {
    20
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            ws_enabled: true,
            ws_port: DEFAULT_WS_PORT,
            language: default_language(),
            theme: default_theme(),
            auto_refresh_minutes: default_auto_refresh(),
            codex_auto_refresh_minutes: default_codex_auto_refresh(),
            ghcp_auto_refresh_minutes: default_ghcp_auto_refresh(),
            windsurf_auto_refresh_minutes: default_windsurf_auto_refresh(),
            kiro_auto_refresh_minutes: default_kiro_auto_refresh(),
            close_behavior: default_close_behavior(),
            opencode_app_path: default_opencode_app_path(),
            antigravity_app_path: default_antigravity_app_path(),
            codex_app_path: default_codex_app_path(),
            vscode_app_path: default_vscode_app_path(),
            windsurf_app_path: default_windsurf_app_path(),
            kiro_app_path: default_kiro_app_path(),
            opencode_sync_on_switch: default_opencode_sync_on_switch(),
            codex_launch_on_switch: default_codex_launch_on_switch(),
            auto_switch_enabled: default_auto_switch_enabled(),
            auto_switch_threshold: default_auto_switch_threshold(),
            quota_alert_enabled: default_quota_alert_enabled(),
            quota_alert_threshold: default_quota_alert_threshold(),
            codex_quota_alert_enabled: default_codex_quota_alert_enabled(),
            codex_quota_alert_threshold: default_codex_quota_alert_threshold(),
            ghcp_quota_alert_enabled: default_ghcp_quota_alert_enabled(),
            ghcp_quota_alert_threshold: default_ghcp_quota_alert_threshold(),
            windsurf_quota_alert_enabled: default_windsurf_quota_alert_enabled(),
            windsurf_quota_alert_threshold: default_windsurf_quota_alert_threshold(),
            kiro_quota_alert_enabled: default_kiro_quota_alert_enabled(),
            kiro_quota_alert_threshold: default_kiro_quota_alert_threshold(),
            kiro_proxy: KiroProxyConfig::default(),
        }
    }
}

/// 运行时状态
struct RuntimeState {
    /// 当前实际使用的端口
    actual_port: Option<u16>,
    /// 用户配置
    user_config: UserConfig,
}

/// 全局运行时状态
static RUNTIME_STATE: OnceLock<RwLock<RuntimeState>> = OnceLock::new();

fn get_runtime_state() -> &'static RwLock<RuntimeState> {
    RUNTIME_STATE.get_or_init(|| {
        RwLock::new(RuntimeState {
            actual_port: None,
            user_config: load_user_config().unwrap_or_default(),
        })
    })
}

/// 获取数据目录路径
pub fn get_data_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("无法获取 Home 目录")?;
    Ok(home.join(DATA_DIR))
}

/// 获取共享目录路径（供其他模块使用）
/// 与 get_data_dir 相同，但不返回 Result
pub fn get_shared_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(DATA_DIR))
        .unwrap_or_else(|| PathBuf::from(DATA_DIR))
}

/// 获取服务状态文件路径
pub fn get_server_status_path() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    Ok(data_dir.join(SERVER_STATUS_FILE))
}

/// 获取用户配置文件路径
pub fn get_user_config_path() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    Ok(data_dir.join(USER_CONFIG_FILE))
}

/// 加载用户配置
pub fn load_user_config() -> Result<UserConfig, String> {
    let config_path = get_user_config_path()?;

    if !config_path.exists() {
        return Ok(UserConfig::default());
    }

    let content =
        fs::read_to_string(&config_path).map_err(|e| format!("读取配置文件失败: {}", e))?;

    let mut value: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("解析配置文件失败: {}", e))?;

    // 兼容旧配置：平台独立预警字段不存在时，继承历史全局预警配置
    if let Some(obj) = value.as_object_mut() {
        if !obj.contains_key("kiro_auto_refresh_minutes") {
            let inherited_refresh = obj
                .get("windsurf_auto_refresh_minutes")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32)
                .unwrap_or_else(default_kiro_auto_refresh);
            obj.insert(
                "kiro_auto_refresh_minutes".to_string(),
                json!(inherited_refresh),
            );
        }

        let legacy_enabled = obj
            .get("quota_alert_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(default_quota_alert_enabled);
        let legacy_threshold = obj
            .get("quota_alert_threshold")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .unwrap_or_else(default_quota_alert_threshold);

        if !obj.contains_key("codex_quota_alert_enabled") {
            obj.insert(
                "codex_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("codex_quota_alert_threshold") {
            obj.insert(
                "codex_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("ghcp_quota_alert_enabled") {
            obj.insert(
                "ghcp_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("ghcp_quota_alert_threshold") {
            obj.insert(
                "ghcp_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("windsurf_quota_alert_enabled") {
            obj.insert(
                "windsurf_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("windsurf_quota_alert_threshold") {
            obj.insert(
                "windsurf_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }
        if !obj.contains_key("kiro_quota_alert_enabled") {
            obj.insert(
                "kiro_quota_alert_enabled".to_string(),
                json!(legacy_enabled),
            );
        }
        if !obj.contains_key("kiro_quota_alert_threshold") {
            obj.insert(
                "kiro_quota_alert_threshold".to_string(),
                json!(legacy_threshold),
            );
        }

        if !obj.contains_key("kiro_proxy") {
            obj.insert(
                "kiro_proxy".to_string(),
                serde_json::to_value(KiroProxyConfig::default())
                    .unwrap_or_else(|_| json!({})),
            );
        }
    }

    serde_json::from_value(value).map_err(|e| format!("解析配置文件失败: {}", e))
}

/// 保存用户配置
pub fn save_user_config(config: &UserConfig) -> Result<(), String> {
    let config_path = get_user_config_path()?;
    let data_dir = get_data_dir()?;

    // 确保目录存在
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).map_err(|e| format!("创建配置目录失败: {}", e))?;
    }

    let json =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化配置失败: {}", e))?;

    fs::write(&config_path, json).map_err(|e| format!("写入配置文件失败: {}", e))?;

    // 更新运行时状态
    if let Ok(mut state) = get_runtime_state().write() {
        state.user_config = config.clone();
    }

    crate::modules::logger::log_info(&format!(
        "[Config] User config saved: ws_enabled={}, ws_port={}",
        config.ws_enabled, config.ws_port
    ));

    Ok(())
}

/// 获取用户配置（从内存）
pub fn get_user_config() -> UserConfig {
    get_runtime_state()
        .read()
        .map(|state| state.user_config.clone())
        .unwrap_or_default()
}

/// 获取用户配置的首选端口
pub fn get_preferred_port() -> u16 {
    get_user_config().ws_port
}

/// 获取当前实际使用的端口
pub fn get_actual_port() -> Option<u16> {
    get_runtime_state()
        .read()
        .ok()
        .and_then(|state| state.actual_port)
}

/// 保存服务状态到共享文件
pub fn save_server_status(status: &ServerStatus) -> Result<(), String> {
    let status_path = get_server_status_path()?;
    let data_dir = get_data_dir()?;

    // 确保目录存在
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).map_err(|e| format!("创建配置目录失败: {}", e))?;
    }

    // 写入状态文件
    let json =
        serde_json::to_string_pretty(status).map_err(|e| format!("序列化状态失败: {}", e))?;

    fs::write(&status_path, json).map_err(|e| format!("写入状态文件失败: {}", e))?;

    crate::modules::logger::log_info(&format!(
        "[Config] Server status saved: ws_port={}, pid={}",
        status.ws_port, status.pid
    ));

    Ok(())
}

/// 初始化服务状态（WebSocket 启动后调用）
pub fn init_server_status(actual_port: u16) -> Result<(), String> {
    // 更新运行时状态
    if let Ok(mut state) = get_runtime_state().write() {
        state.actual_port = Some(actual_port);
    }

    let status = ServerStatus {
        ws_port: actual_port,
        version: env!("CARGO_PKG_VERSION").to_string(),
        pid: std::process::id(),
        started_at: chrono::Utc::now().timestamp(),
    };

    save_server_status(&status)?;

    Ok(())
}
