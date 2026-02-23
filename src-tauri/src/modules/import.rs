use crate::models;
use crate::modules;
use crate::utils;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use uuid::Uuid;

// ==================== 辅助结构体和函数 ====================

#[derive(Debug, Deserialize)]
pub struct OldToolAccount {
    pub email: String,
    pub name: Option<String>,
    pub token: models::TokenData,
    #[serde(default)]
    pub device_profile: Option<models::DeviceProfile>,
    #[serde(default)]
    pub device_history: Vec<models::DeviceProfileVersion>,
}

#[derive(Debug, Deserialize)]
pub struct FingerprintJsonInput {
    pub name: Option<String>,
    pub label: Option<String>,
    pub created_at: Option<i64>,
    pub profile: Option<models::DeviceProfile>,
    pub machine_id: Option<String>,
    pub mac_machine_id: Option<String>,
    pub dev_device_id: Option<String>,
    pub sqm_id: Option<String>,
    pub service_machine_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExtensionCredentialsFile {
    accounts: HashMap<String, ExtensionCredential>,
}

#[derive(Debug, Deserialize)]
struct ExtensionCredential {
    pub email: Option<String>,
    #[serde(rename = "refreshToken", alias = "refresh_token")]
    pub refresh_token: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "accessToken", alias = "access_token")]
    pub access_token: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "expiresAt", alias = "expires_at")]
    pub expires_at: Option<String>,
    #[serde(rename = "projectId", alias = "project_id")]
    pub project_id: Option<String>,
}

pub fn normalize_service_machine_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if Uuid::parse_str(trimmed).is_ok() {
        Some(trimmed.to_string())
    } else {
        None
    }
}

pub fn fingerprint_profile_full_key(profile: &models::DeviceProfile) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        profile.machine_id,
        profile.mac_machine_id,
        profile.dev_device_id,
        profile.sqm_id,
        profile.service_machine_id.trim()
    )
}

pub fn fingerprint_profile_weak_key(profile: &models::DeviceProfile) -> String {
    format!(
        "{}|{}|{}|{}",
        profile.machine_id, profile.mac_machine_id, profile.dev_device_id, profile.sqm_id
    )
}

pub fn build_fingerprint_profile_map(
    store: &modules::fingerprint::FingerprintStore,
) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for fp in &store.fingerprints {
        let weak_key = fingerprint_profile_weak_key(&fp.profile);
        map.entry(weak_key).or_insert_with(|| fp.id.clone());
        if normalize_service_machine_id(&fp.profile.service_machine_id).is_some() {
            let full_key = fingerprint_profile_full_key(&fp.profile);
            map.entry(full_key).or_insert_with(|| fp.id.clone());
        }
    }
    map
}

pub fn upsert_fingerprint_in_store(
    store: &mut modules::fingerprint::FingerprintStore,
    profile: models::DeviceProfile,
    name: String,
    created_at: Option<i64>,
    fingerprint_map: &mut HashMap<String, String>,
) -> (String, bool) {
    let mut profile = profile;
    let weak_key = fingerprint_profile_weak_key(&profile);
    let normalized_service_id = normalize_service_machine_id(&profile.service_machine_id);
    if let Some(ref service_id) = normalized_service_id {
        if *service_id != profile.service_machine_id {
            profile.service_machine_id = service_id.clone();
        }
        let full_key = fingerprint_profile_full_key(&profile);
        if let Some(id) = fingerprint_map.get(&full_key) {
            return (id.clone(), false);
        }
    } else if let Some(id) = fingerprint_map.get(&weak_key) {
        return (id.clone(), false);
    }

    if normalized_service_id.is_none() {
        modules::device::ensure_service_machine_id(&mut profile);
    }

    let full_key = fingerprint_profile_full_key(&profile);
    if let Some(id) = fingerprint_map.get(&full_key) {
        return (id.clone(), false);
    }
    let fingerprint = modules::fingerprint::Fingerprint {
        id: Uuid::new_v4().to_string(),
        name,
        profile,
        created_at: created_at.unwrap_or_else(|| chrono::Utc::now().timestamp()),
    };
    let id = fingerprint.id.clone();
    store.fingerprints.push(fingerprint);
    fingerprint_map
        .entry(full_key)
        .or_insert_with(|| id.clone());
    fingerprint_map
        .entry(weak_key)
        .or_insert_with(|| id.clone());
    (id, true)
}

pub fn format_import_name(base: &str, label: Option<&str>, created_at: Option<i64>) -> String {
    if let Some(label) = label {
        let trimmed = label.trim();
        if !trimmed.is_empty() {
            return format!("{base} - {trimmed}");
        }
    }
    if let Some(ts) = created_at {
        return format!("{base} - {ts}");
    }
    format!("{base} - 导入")
}

pub fn select_account_profile(
    account: &OldToolAccount,
) -> Option<(models::DeviceProfile, Option<String>, Option<i64>)> {
    let current = account.device_history.iter().find(|v| v.is_current);
    if let Some(profile) = account.device_profile.clone() {
        let label = current.map(|v| v.label.clone());
        let created_at = current.map(|v| v.created_at);
        return Some((profile, label, created_at));
    }
    if let Some(entry) = current {
        return Some((
            entry.profile.clone(),
            Some(entry.label.clone()),
            Some(entry.created_at),
        ));
    }
    account.device_history.last().map(|entry| {
        (
            entry.profile.clone(),
            Some(entry.label.clone()),
            Some(entry.created_at),
        )
    })
}

pub fn extract_profile_from_input(input: &FingerprintJsonInput) -> Option<models::DeviceProfile> {
    if let Some(profile) = input.profile.clone() {
        return Some(profile);
    }
    let machine_id = input.machine_id.clone()?;
    let mac_machine_id = input.mac_machine_id.clone()?;
    let dev_device_id = input.dev_device_id.clone()?;
    let sqm_id = input.sqm_id.clone()?;
    Some(models::DeviceProfile {
        machine_id,
        mac_machine_id,
        dev_device_id,
        sqm_id,
        service_machine_id: input.service_machine_id.clone().unwrap_or_default(),
    })
}

pub fn resolve_json_import_name(
    name: Option<&str>,
    label: Option<&str>,
    created_at: Option<i64>,
    index: usize,
) -> String {
    if let Some(value) = name {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    if let Some(value) = label {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return format!("导入指纹 - {trimmed}");
        }
    }
    if let Some(ts) = created_at {
        return format!("导入指纹 - {ts}");
    }
    format!("导入指纹 - {}", index + 1)
}

// ==================== 导入命令逻辑 ====================

/// 从旧版 ~/.antigravity_tools/ 导入账号
pub async fn import_from_old_tools_logic() -> Result<Vec<models::Account>, String> {
    use std::fs;

    let home = dirs::home_dir().ok_or("无法获取用户主目录")?;
    let old_dir = home.join(".antigravity_tools");

    if !old_dir.exists() {
        return Err("未找到旧版数据目录 ~/.antigravity_tools/".to_string());
    }

    let old_accounts_dir = old_dir.join("accounts");
    if !old_accounts_dir.exists() {
        return Err("未找到旧版账号目录 ~/.antigravity_tools/accounts/".to_string());
    }

 modules::logger::log_info("start importaccount...");

    let mut imported = Vec::new();
    let mut fingerprint_store = modules::fingerprint::load_fingerprint_store()?;
    let mut fingerprint_map = build_fingerprint_profile_map(&fingerprint_store);
    let mut fingerprint_dirty = false;

    // 读取旧版索引
    let old_index_path = old_dir.join("accounts.json");
    if old_index_path.exists() {
        let content =
            fs::read_to_string(&old_index_path).map_err(|e| format!("读取旧版索引失败: {}", e))?;

        let old_index: models::AccountIndex =
            serde_json::from_str(&content).map_err(|e| format!("解析旧版索引失败: {}", e))?;

        for summary in old_index.accounts {
            let old_account_path = old_accounts_dir.join(format!("{}.json", summary.id));
            if old_account_path.exists() {
                match fs::read_to_string(&old_account_path) {
                    Ok(account_content) => {
                        match serde_json::from_str::<OldToolAccount>(&account_content) {
                            Ok(old_account) => {
                                // 使用 upsert 导入（避免重复）
                                match modules::upsert_account(
                                    old_account.email.clone(),
                                    old_account.name.clone(),
                                    old_account.token.clone(),
                                ) {
                                    Ok(mut new_account) => {
                                        if let Some((profile, label, created_at)) =
                                            select_account_profile(&old_account)
                                        {
                                            let base = old_account
                                                .name
                                                .as_deref()
                                                .unwrap_or(&old_account.email);
                                            let name = format_import_name(
                                                base,
                                                label.as_deref(),
                                                created_at,
                                            );
                                            let (fp_id, inserted) = upsert_fingerprint_in_store(
                                                &mut fingerprint_store,
                                                profile,
                                                name,
                                                created_at,
                                                &mut fingerprint_map,
                                            );
                                            if inserted {
                                                fingerprint_dirty = true;
                                            }
                                            new_account.fingerprint_id = Some(fp_id);
                                            if let Err(e) = modules::save_account(&new_account) {
                                                modules::logger::log_error(&format!(
                                                    "更新账号指纹失败 {}: {}",
                                                    new_account.email, e
                                                ));
                                            }
                                        }
                                        modules::logger::log_info(&format!(
                                            "导入账号: {}",
                                            new_account.email
                                        ));
                                        imported.push(new_account);
                                    }
                                    Err(e) => {
                                        modules::logger::log_error(&format!(
                                            "导入账号失败 {}: {}",
                                            old_account.email, e
                                        ));
                                    }
                                }
                            }
                            Err(e) => {
                                modules::logger::log_error(&format!(
                                    "解析账号文件失败 {:?}: {}",
                                    old_account_path, e
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        modules::logger::log_error(&format!(
                            "读取账号文件失败 {:?}: {}",
                            old_account_path, e
                        ));
                    }
                }
            }
        }
    }
    if fingerprint_dirty {
        modules::fingerprint::save_fingerprint_store(&fingerprint_store)?;
    }

 modules::logger::log_info(&format!("importcompleted，import {} account", imported.len()));

    // 广播数据变更通知
    if !imported.is_empty() {
        modules::websocket::broadcast_data_changed("import_from_old_tools");
    }

    Ok(imported)
}

/// 从旧版 ~/.antigravity_tools/ 导入指纹（不导入账号）
pub async fn import_fingerprints_from_old_tools_logic() -> Result<usize, String> {
    use std::fs;

    let home = dirs::home_dir().ok_or("无法获取用户主目录")?;
    let old_dir = home.join(".antigravity_tools");

    if !old_dir.exists() {
        return Err("未找到旧版数据目录 ~/.antigravity_tools/".to_string());
    }

    let old_accounts_dir = old_dir.join("accounts");
    if !old_accounts_dir.exists() {
        return Err("未找到旧版账号目录 ~/.antigravity_tools/accounts/".to_string());
    }

 modules::logger::log_info("start import...");

    let mut imported_count = 0;
    let mut fingerprint_store = modules::fingerprint::load_fingerprint_store()?;
    let mut fingerprint_map = build_fingerprint_profile_map(&fingerprint_store);
    let mut fingerprint_dirty = false;

    let old_index_path = old_dir.join("accounts.json");
    if old_index_path.exists() {
        let content =
            fs::read_to_string(&old_index_path).map_err(|e| format!("读取旧版索引失败: {}", e))?;

        let old_index: models::AccountIndex =
            serde_json::from_str(&content).map_err(|e| format!("解析旧版索引失败: {}", e))?;

        for summary in old_index.accounts {
            let old_account_path = old_accounts_dir.join(format!("{}.json", summary.id));
            if !old_account_path.exists() {
                continue;
            }
            match fs::read_to_string(&old_account_path) {
                Ok(account_content) => {
                    match serde_json::from_str::<OldToolAccount>(&account_content) {
                        Ok(old_account) => {
                            let base = old_account.name.as_deref().unwrap_or(&old_account.email);

                            for version in &old_account.device_history {
                                let name = format_import_name(
                                    base,
                                    Some(version.label.as_str()),
                                    Some(version.created_at),
                                );
                                let (_, inserted) = upsert_fingerprint_in_store(
                                    &mut fingerprint_store,
                                    version.profile.clone(),
                                    name,
                                    Some(version.created_at),
                                    &mut fingerprint_map,
                                );
                                if inserted {
                                    imported_count += 1;
                                    fingerprint_dirty = true;
                                }
                            }

                            if let Some((profile, label, created_at)) =
                                select_account_profile(&old_account)
                            {
                                let name = format_import_name(base, label.as_deref(), created_at);
                                let (_, inserted) = upsert_fingerprint_in_store(
                                    &mut fingerprint_store,
                                    profile,
                                    name,
                                    created_at,
                                    &mut fingerprint_map,
                                );
                                if inserted {
                                    imported_count += 1;
                                    fingerprint_dirty = true;
                                }
                            }
                        }
                        Err(e) => {
                            modules::logger::log_error(&format!(
                                "解析账号文件失败 {:?}: {}",
                                old_account_path, e
                            ));
                        }
                    }
                }
                Err(e) => {
                    modules::logger::log_error(&format!(
                        "读取账号文件失败 {:?}: {}",
                        old_account_path, e
                    ));
                }
            }
        }
    }

    if fingerprint_dirty {
        modules::fingerprint::save_fingerprint_store(&fingerprint_store)?;
    }

 modules::logger::log_info(&format!("importcompleted，import {} ", imported_count));
    Ok(imported_count)
}

/// 从 JSON 导入指纹
pub async fn import_fingerprints_from_json_logic(json_content: String) -> Result<usize, String> {
    let trimmed = json_content.trim();
    if trimmed.is_empty() {
        return Err("JSON 内容为空".to_string());
    }

    let value: serde_json::Value =
        serde_json::from_str(trimmed).map_err(|e| format!("JSON 格式错误: {}", e))?;

    let mut candidates: Vec<(
        Option<String>,
        Option<String>,
        models::DeviceProfile,
        Option<i64>,
    )> = Vec::new();

    if value.is_object() {
        let obj = value.as_object().ok_or("JSON 格式错误")?;
        if obj.contains_key("fingerprints") || obj.contains_key("original_baseline") {
            let store: modules::fingerprint::FingerprintStore =
                serde_json::from_value(value).map_err(|e| format!("解析指纹存储失败: {}", e))?;
            if let Some(baseline) = store.original_baseline {
                candidates.push((
                    Some(baseline.name),
                    None,
                    baseline.profile,
                    Some(baseline.created_at),
                ));
            }
            for fp in store.fingerprints {
                candidates.push((Some(fp.name), None, fp.profile, Some(fp.created_at)));
            }
        } else {
            let input: FingerprintJsonInput =
                serde_json::from_value(value).map_err(|e| format!("解析指纹数据失败: {}", e))?;
            if let Some(profile) = extract_profile_from_input(&input) {
                candidates.push((input.name, input.label, profile, input.created_at));
            }
        }
    } else if let Some(list) = value.as_array() {
        for item in list {
            let input: FingerprintJsonInput = serde_json::from_value(item.clone())
                .map_err(|e| format!("解析指纹数据失败: {}", e))?;
            if let Some(profile) = extract_profile_from_input(&input) {
                candidates.push((input.name, input.label, profile, input.created_at));
            }
        }
    } else {
        return Err("JSON 格式错误".to_string());
    }

    if candidates.is_empty() {
        return Err("未找到可导入的指纹数据".to_string());
    }

    let mut fingerprint_store = modules::fingerprint::load_fingerprint_store()?;
    let mut fingerprint_map = build_fingerprint_profile_map(&fingerprint_store);
    let mut imported_count = 0;

    for (idx, (name, label, profile, created_at)) in candidates.into_iter().enumerate() {
        let display_name =
            resolve_json_import_name(name.as_deref(), label.as_deref(), created_at, idx);
        let (_, inserted) = upsert_fingerprint_in_store(
            &mut fingerprint_store,
            profile,
            display_name,
            created_at,
            &mut fingerprint_map,
        );
        if inserted {
            imported_count += 1;
        }
    }

    if imported_count > 0 {
        modules::fingerprint::save_fingerprint_store(&fingerprint_store)?;
    }

    Ok(imported_count)
}

/// 从本地 Antigravity 客户端导入当前账号
pub async fn import_from_local_logic() -> Result<models::Account, String> {
    use base64::{engine::general_purpose, Engine as _};

 modules::logger::log_info("start local Antigravity import...");

    // 读取 state.vscdb
    let db_path = modules::db::get_db_path()?;
    let conn =
        rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;

    // 读取 protobuf 数据
    let state_data: String = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = ?",
            ["jetskiStateSync.agentManagerInitState"],
            |row| row.get(0),
        )
        .map_err(|_| "未找到登录状态，请确保 Antigravity 客户端已登录")?;

    // Base64 解码
    let blob = general_purpose::STANDARD
        .decode(&state_data)
        .map_err(|e| format!("Base64 解码失败: {}", e))?;

    // 解析 protobuf 获取 refresh_token（Field 6）
    let refresh_token =
        utils::protobuf::extract_refresh_token(&blob).ok_or("无法从本地数据解析 refresh_token")?;

    if refresh_token.is_empty() {
        return Err("本地 refresh_token 为空".to_string());
    }

    modules::logger::log_info(&format!(
        "获取到本地 refresh_token (len={})",
        refresh_token.len()
    ));

    // 使用 refresh_token 获取新的 access_token
    let token_response = modules::oauth::refresh_access_token(&refresh_token).await?;

    // 获取用户信息
    let user_info = modules::oauth::get_user_info(&token_response.access_token).await?;
    let email = user_info.email.clone();

    // 构建 TokenData
    let token = models::TokenData::new(
        token_response.access_token,
        token_response.refresh_token.unwrap_or(refresh_token),
        token_response.expires_in,
        Some(email.clone()),
        None,
        None,
    );

    // 添加或更新账号
    let account = modules::upsert_account(email.clone(), user_info.get_display_name(), token)?;

    modules::logger::log_info(&format!("localaccountimportsucceeded: {}", email));

    // 广播数据变更通知
    modules::websocket::broadcast_data_changed("import_from_local");

    Ok(account)
}

/// 从 JSON 导入账号
pub async fn import_from_json_logic(json_content: String) -> Result<Vec<models::Account>, String> {
 modules::logger::log_info("start JSON importaccount...");

    // 简化格式: [{"email": "xxx", "refresh_token": "..."}]
    #[derive(serde::Deserialize)]
    struct SimpleAccount {
        email: String,
        refresh_token: String,
    }

    // 尝试解析为简化格式数组
    let simple_accounts: Result<Vec<SimpleAccount>, _> = serde_json::from_str(&json_content)
        .or_else(|_| {
            // 单个简化账号
            serde_json::from_str::<SimpleAccount>(&json_content).map(|a| vec![a])
        });

    if let Ok(accounts) = simple_accounts {
        let mut imported = Vec::new();

        for simple in accounts {
            modules::logger::log_info(&format!("in progress: importaccount: {}", simple.email));

            // 使用 refresh_token 获取 access_token
            match modules::oauth::refresh_access_token(&simple.refresh_token).await {
                Ok(token_response) => {
                    // 构建 TokenData
                    let token = models::TokenData::new(
                        token_response.access_token,
                        token_response.refresh_token.unwrap_or(simple.refresh_token),
                        token_response.expires_in,
                        Some(simple.email.clone()),
                        None,
                        None,
                    );

                    match modules::upsert_account(simple.email.clone(), None, token) {
                        Ok(new_account) => {
                            modules::logger::log_info(&format!(
                                "导入账号成功: {}",
                                new_account.email
                            ));
                            imported.push(new_account);
                        }
                        Err(e) => {
                            modules::logger::log_error(&format!(
                                "保存账号失败 {}: {}",
                                simple.email, e
                            ));
                        }
                    }
                }
                Err(e) => {
                    modules::logger::log_error(&format!("refresh Token failed {}: {}", simple.email, e));
                }
            }
        }

 modules::logger::log_info(&format!("JSON importcompleted，import {} account", imported.len()));
        return Ok(imported);
    }

    // 尝试解析为完整账号格式（向后兼容）
    let accounts: Vec<models::Account> = serde_json::from_str(&json_content)
        .or_else(|_| serde_json::from_str::<models::Account>(&json_content).map(|a| vec![a]))
        .map_err(|e| format!("JSON 格式错误: {}", e))?;

    let mut imported = Vec::new();

    for old_account in accounts {
        match modules::upsert_account(
            old_account.email.clone(),
            old_account.name.clone(),
            old_account.token.clone(),
        ) {
            Ok(new_account) => {
                modules::logger::log_info(&format!("importaccount: {}", new_account.email));
                imported.push(new_account);
            }
            Err(e) => {
                modules::logger::log_error(&format!("importaccountfailed {}: {}", old_account.email, e));
            }
        }
    }

 modules::logger::log_info(&format!("JSON importcompleted，import {} account", imported.len()));

    // 广播数据变更通知
    if !imported.is_empty() {
        modules::websocket::broadcast_data_changed("import_from_json");
    }

    Ok(imported)
}

/// 从插件共享目录导入账号（credentials.json）
pub async fn import_from_extension_credentials() -> Result<usize, String> {
    let data_dir = modules::config::get_data_dir()?;
    let file_path = data_dir.join("credentials.json");

    if !file_path.exists() {
        return Ok(0);
    }

    let content =
        fs::read_to_string(&file_path).map_err(|e| format!("读取 credentials.json 失败: {}", e))?;

    let parsed: ExtensionCredentialsFile =
        serde_json::from_str(&content).map_err(|e| format!("解析 credentials.json 失败: {}", e))?;

    if parsed.accounts.is_empty() {
        return Ok(0);
    }

    // 现有账号 refresh_token，用于去重
    let existing_accounts = modules::list_accounts()?;
    let mut existing_tokens = HashMap::new();
    for acc in existing_accounts {
        existing_tokens.insert(acc.email.clone(), acc.token.refresh_token.clone());
    }

    let mut imported_count = 0;

    for (key, item) in parsed.accounts {
        let email = item.email.unwrap_or_else(|| key.clone());
        let refresh_token = match item.refresh_token {
            Some(token) if !token.trim().is_empty() => token,
            _ => continue,
        };

        if let Some(existing) = existing_tokens.get(&email) {
            if existing == &refresh_token {
                continue;
            }
        }

        match modules::oauth::refresh_access_token(&refresh_token).await {
            Ok(token_response) => {
                let user_info = modules::oauth::get_user_info(&token_response.access_token).await?;
                let token = models::TokenData::new(
                    token_response.access_token,
                    token_response.refresh_token.unwrap_or(refresh_token),
                    token_response.expires_in,
                    Some(user_info.email.clone()),
                    item.project_id.clone(),
                    None,
                );

                match modules::upsert_account(
                    user_info.email.clone(),
                    user_info.get_display_name(),
                    token,
                ) {
                    Ok(_) => {
                        imported_count += 1;
                    }
                    Err(e) => {
                        modules::logger::log_error(&format!("importaccountfailed {}: {}", email, e));
                    }
                }
            }
            Err(e) => {
                modules::logger::log_error(&format!("refresh Token failed {}: {}", email, e));
            }
        }
    }

    if imported_count > 0 {
        modules::websocket::broadcast_data_changed("extension_sync");
    }

    Ok(imported_count)
}
