use crate::models;
use crate::modules;
use tauri::AppHandle;

#[tauri::command]
pub async fn start_oauth_login(app_handle: AppHandle) -> Result<models::Account, String> {
    modules::logger::log_info("start  OAuth flow...");

    let token_res = modules::oauth_server::start_oauth_flow(app_handle.clone())
        .await
        .map_err(|e| {
 modules::logger::log_error(&format!("OAuth failed: {}", e));
            e
        })?;

 modules::logger::log_info("OAuth succeeded， refresh_token...");

    let refresh_token = token_res.refresh_token.ok_or_else(|| {
        let msg = "Refresh token was not returned.\n\n\
         Possible cause: this app was authorized before\n\n\
         Solution:\n\
         1. Visit https://myaccount.google.com/permissions\n\
         2. Revoke access for 'Antigravity Tools'\n\
         3. Run OAuth authorization again"
            .to_string();
        modules::logger::log_error(&msg);
        msg
    })?;

    modules::logger::log_info("fetch user info...");
    let user_info = modules::oauth::get_user_info(&token_res.access_token)
        .await
        .map_err(|e| {
            modules::logger::log_error(&format!("fetch user infofailed: {}", e));
            e
        })?;

    modules::logger::log_info(&format!(
        "User: {} ({})",
        user_info.email,
        user_info.name.as_deref().unwrap_or("No name")
    ));

    let token_data = models::TokenData::new(
        token_res.access_token,
        refresh_token,
        token_res.expires_in,
        Some(user_info.email.clone()),
        None,
        user_info.id.clone(),
    );

    let account = modules::upsert_account(
        user_info.email.clone(),
        user_info.get_display_name(),
        token_data,
    )
    .map_err(|e| {
        modules::logger::log_error(&format!("saveaccountfailed: {}", e));
        e
    })?;

 modules::logger::log_info(&format!("accountsucceeded: {}", account.email));

    // 广播数据变更通知
    modules::websocket::broadcast_data_changed("oauth_login");

    Ok(account)
}

#[tauri::command]
pub async fn complete_oauth_login(app_handle: AppHandle) -> Result<models::Account, String> {
    modules::logger::log_info("completed OAuth flow...");

    let token_res = modules::oauth_server::complete_oauth_flow(app_handle.clone())
        .await
        .map_err(|e| {
 modules::logger::log_error(&format!("OAuth failed: {}", e));
            e
        })?;

 modules::logger::log_info("OAuth succeeded， refresh_token...");

    let refresh_token = token_res.refresh_token.ok_or_else(|| {
        let msg = "Refresh token was not returned.\n\n\
         Possible cause: this app was authorized before\n\n\
         Solution:\n\
         1. Visit https://myaccount.google.com/permissions\n\
         2. Revoke access for 'Antigravity Tools'\n\
         3. Run OAuth authorization again"
            .to_string();
        modules::logger::log_error(&msg);
        msg
    })?;

    modules::logger::log_info("fetch user info...");
    let user_info = modules::oauth::get_user_info(&token_res.access_token)
        .await
        .map_err(|e| {
            modules::logger::log_error(&format!("fetch user infofailed: {}", e));
            e
        })?;

    modules::logger::log_info(&format!(
        "User: {} ({})",
        user_info.email,
        user_info.name.as_deref().unwrap_or("No name")
    ));

    let token_data = models::TokenData::new(
        token_res.access_token,
        refresh_token,
        token_res.expires_in,
        Some(user_info.email.clone()),
        None,
        user_info.id.clone(),
    );

    let account = modules::upsert_account(
        user_info.email.clone(),
        user_info.get_display_name(),
        token_data,
    )
    .map_err(|e| {
        modules::logger::log_error(&format!("saveaccountfailed: {}", e));
        e
    })?;

 modules::logger::log_info(&format!("accountsucceeded: {}", account.email));
    modules::websocket::broadcast_data_changed("oauth_login");

    Ok(account)
}

#[tauri::command]
pub async fn prepare_oauth_url(app_handle: AppHandle) -> Result<String, String> {
    modules::oauth_server::prepare_oauth_url(app_handle).await
}

#[tauri::command]
pub async fn cancel_oauth_login() -> Result<(), String> {
    modules::oauth_server::cancel_oauth_flow();
    Ok(())
}

