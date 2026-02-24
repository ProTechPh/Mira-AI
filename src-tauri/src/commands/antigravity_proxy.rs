use crate::modules::config::AntigravityProxyConfig;
use crate::modules::antigravity_proxy::types::{
    ProxyAccountView, ProxyAdminLogsResponse, ProxyAdminStatsResponse, ProxyModelView, ProxyStatus,
};

#[tauri::command]
pub async fn antigravity_proxy_start() -> Result<ProxyStatus, String> {
    let service = crate::modules::antigravity_proxy::shared_service().await;
    service.start().await
}

#[tauri::command]
pub async fn antigravity_proxy_stop() -> Result<ProxyStatus, String> {
    let service = crate::modules::antigravity_proxy::shared_service().await;
    service.stop().await
}

#[tauri::command]
pub async fn antigravity_proxy_get_status() -> Result<ProxyStatus, String> {
    let service = crate::modules::antigravity_proxy::shared_service().await;
    Ok(service.status().await)
}

#[tauri::command]
pub async fn antigravity_proxy_get_config() -> Result<AntigravityProxyConfig, String> {
    let service = crate::modules::antigravity_proxy::shared_service().await;
    Ok(service.get_config().await)
}

#[tauri::command]
pub async fn antigravity_proxy_update_config(
    config: AntigravityProxyConfig,
) -> Result<AntigravityProxyConfig, String> {
    let service = crate::modules::antigravity_proxy::shared_service().await;
    service.update_config(config).await
}

#[tauri::command]
pub async fn antigravity_proxy_sync_accounts() -> Result<Vec<ProxyAccountView>, String> {
    let service = crate::modules::antigravity_proxy::shared_service().await;
    Ok(service.sync_accounts().await)
}

#[tauri::command]
pub async fn antigravity_proxy_get_accounts() -> Result<Vec<ProxyAccountView>, String> {
    let service = crate::modules::antigravity_proxy::shared_service().await;
    Ok(service.get_accounts().await)
}

#[tauri::command]
pub async fn antigravity_proxy_refresh_models() -> Result<Vec<ProxyModelView>, String> {
    let service = crate::modules::antigravity_proxy::shared_service().await;
    service.refresh_models().await
}

#[tauri::command]
pub async fn antigravity_proxy_get_models() -> Result<Vec<ProxyModelView>, String> {
    let service = crate::modules::antigravity_proxy::shared_service().await;
    service.get_models().await
}

#[tauri::command]
pub async fn antigravity_proxy_get_logs(
    limit: Option<usize>,
) -> Result<ProxyAdminLogsResponse, String> {
    let service = crate::modules::antigravity_proxy::shared_service().await;
    Ok(service.get_logs(limit).await)
}

#[tauri::command]
pub async fn antigravity_proxy_clear_logs() -> Result<(), String> {
    let service = crate::modules::antigravity_proxy::shared_service().await;
    service.clear_logs().await
}

#[tauri::command]
pub async fn antigravity_proxy_reset_stats() -> Result<(), String> {
    let service = crate::modules::antigravity_proxy::shared_service().await;
    service.reset_stats().await
}

#[tauri::command]
pub async fn antigravity_proxy_get_stats() -> Result<ProxyAdminStatsResponse, String> {
    let service = crate::modules::antigravity_proxy::shared_service().await;
    Ok(service.get_stats().await)
}
