use crate::models;
use crate::modules;

#[tauri::command]
pub async fn import_from_old_tools() -> Result<Vec<models::Account>, String> {
    modules::import::import_from_old_tools_logic().await
}

#[tauri::command]
pub async fn import_fingerprints_from_old_tools() -> Result<usize, String> {
    modules::import::import_fingerprints_from_old_tools_logic().await
}

#[tauri::command]
pub async fn import_fingerprints_from_json(json_content: String) -> Result<usize, String> {
    modules::import::import_fingerprints_from_json_logic(json_content).await
}

#[tauri::command]
pub async fn import_from_local() -> Result<models::Account, String> {
    modules::import::import_from_local_logic().await
}

#[tauri::command]
pub async fn import_from_json(json_content: String) -> Result<Vec<models::Account>, String> {
    modules::import::import_from_json_logic(json_content).await
}

#[tauri::command]
pub async fn export_accounts(account_ids: Vec<String>) -> Result<String, String> {
    let mut accounts_to_export = Vec::new();

    if account_ids.is_empty() {
        // 导出全部
        accounts_to_export = modules::list_accounts()?;
    } else {
        for id in &account_ids {
            if let Ok(account) = modules::load_account(id) {
                accounts_to_export.push(account);
            }
        }
    }

    #[derive(serde::Serialize)]
    struct SimpleAccount {
        email: String,
        refresh_token: String,
    }

    let simplified: Vec<SimpleAccount> = accounts_to_export
        .into_iter()
        .map(|account| SimpleAccount {
            email: account.email,
            refresh_token: account.token.refresh_token,
        })
        .collect();

    let json =
        serde_json::to_string_pretty(&simplified).map_err(|e| format!("序列化失败: {}", e))?;

    Ok(json)
}
