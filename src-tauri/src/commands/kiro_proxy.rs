use crate::modules::config::KiroProxyConfig;
use crate::modules::kiro_proxy::types::{
    AddApiKeyInput, ProxyAccountView, ProxyAdminLogsResponse, ProxyAdminStatsResponse,
    ProxyApiKeyView, ProxyModelView, ProxyStatus, UpdateApiKeyInput,
};

#[tauri::command]
pub async fn kiro_proxy_start() -> Result<ProxyStatus, String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    service.start().await
}

#[tauri::command]
pub async fn kiro_proxy_stop() -> Result<ProxyStatus, String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    service.stop().await
}

#[tauri::command]
pub async fn kiro_proxy_get_status() -> Result<ProxyStatus, String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    Ok(service.status().await)
}

#[tauri::command]
pub async fn kiro_proxy_get_config() -> Result<KiroProxyConfig, String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    Ok(service.get_config().await)
}

#[tauri::command]
pub async fn kiro_proxy_update_config(config: KiroProxyConfig) -> Result<KiroProxyConfig, String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    service.update_config(config).await
}

#[tauri::command]
pub async fn kiro_proxy_sync_accounts() -> Result<Vec<ProxyAccountView>, String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    Ok(service.sync_accounts().await)
}

#[tauri::command]
pub async fn kiro_proxy_get_accounts() -> Result<Vec<ProxyAccountView>, String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    Ok(service.get_accounts().await)
}

#[tauri::command]
pub async fn kiro_proxy_refresh_models() -> Result<Vec<ProxyModelView>, String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    service.refresh_models().await
}

#[tauri::command]
pub async fn kiro_proxy_get_models() -> Result<Vec<ProxyModelView>, String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    service.get_models().await
}

#[tauri::command]
pub async fn kiro_proxy_get_logs(limit: Option<usize>) -> Result<ProxyAdminLogsResponse, String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    Ok(service.get_logs(limit).await)
}

#[tauri::command]
pub async fn kiro_proxy_clear_logs() -> Result<(), String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    service.clear_logs().await
}

#[tauri::command]
pub async fn kiro_proxy_reset_stats() -> Result<(), String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    service.reset_stats().await
}

#[tauri::command]
pub async fn kiro_proxy_get_stats() -> Result<ProxyAdminStatsResponse, String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    Ok(service.get_stats().await)
}

#[tauri::command]
pub async fn kiro_proxy_get_api_keys() -> Result<Vec<ProxyApiKeyView>, String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    Ok(service.get_api_keys().await)
}

#[tauri::command]
pub async fn kiro_proxy_add_api_key(input: AddApiKeyInput) -> Result<(), String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    let _ = service.add_api_key(input).await?;
    Ok(())
}

#[tauri::command]
pub async fn kiro_proxy_update_api_key(input: UpdateApiKeyInput) -> Result<(), String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    let _ = service.update_api_key(input).await?;
    Ok(())
}

#[tauri::command]
pub async fn kiro_proxy_delete_api_key(id: String) -> Result<(), String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    service.delete_api_key(&id).await
}

#[tauri::command]
pub async fn kiro_proxy_reset_api_key_usage(id: String) -> Result<(), String> {
    let service = crate::modules::kiro_proxy::shared_service().await;
    service.reset_api_key_usage(&id).await
}
