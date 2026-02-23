use crate::models;
use crate::modules;

#[tauri::command]
pub async fn get_device_profiles(
    account_id: String,
) -> Result<modules::account::DeviceProfiles, String> {
    modules::get_device_profiles(&account_id)
}

#[tauri::command]
pub async fn bind_device_profile(
    account_id: String,
    mode: String,
) -> Result<models::DeviceProfile, String> {
    modules::bind_device_profile(&account_id, &mode)
}

#[tauri::command]
pub async fn list_device_versions(
    account_id: String,
) -> Result<modules::account::DeviceProfiles, String> {
    modules::list_device_versions(&account_id)
}

#[tauri::command]
pub async fn restore_device_version(
    account_id: String,
    version_id: String,
) -> Result<models::DeviceProfile, String> {
    modules::restore_device_version(&account_id, &version_id)
}

#[tauri::command]
pub async fn delete_device_version(account_id: String, version_id: String) -> Result<(), String> {
    modules::delete_device_version(&account_id, &version_id)
}

#[tauri::command]
pub async fn restore_original_device() -> Result<String, String> {
    modules::device::restore_original_device()
}

#[tauri::command]
pub async fn open_device_folder() -> Result<(), String> {
    modules::device::open_device_folder()
}

#[tauri::command]
pub async fn bind_device_profile_with_profile(
    account_id: String,
    profile: models::DeviceProfile,
) -> Result<models::DeviceProfile, String> {
    modules::bind_device_profile_with_profile(&account_id, profile)
}

/// 生成新设备指纹（预览）
#[tauri::command]
pub async fn preview_generate_profile() -> Result<models::DeviceProfile, String> {
    Ok(modules::device::generate_profile())
}

/// 预览当前设备指纹（读取 storage.json）
#[tauri::command]
pub async fn preview_current_profile() -> Result<models::DeviceProfile, String> {
    let storage_path = modules::device::get_storage_path()?;
    modules::device::read_profile(&storage_path)
}

// ==================== 指纹管理命令 ====================

#[tauri::command]
pub async fn list_fingerprints() -> Result<Vec<modules::fingerprint::FingerprintWithStats>, String>
{
    modules::fingerprint::list_fingerprints_with_stats()
}

#[tauri::command]
pub async fn get_fingerprint(
    fingerprint_id: String,
) -> Result<modules::fingerprint::Fingerprint, String> {
    modules::fingerprint::get_fingerprint(&fingerprint_id)
}

#[tauri::command]
pub async fn generate_new_fingerprint(
    name: String,
) -> Result<modules::fingerprint::Fingerprint, String> {
    modules::fingerprint::generate_fingerprint(name)
}

#[tauri::command]
pub async fn capture_current_fingerprint(
    name: String,
) -> Result<modules::fingerprint::Fingerprint, String> {
    modules::fingerprint::capture_fingerprint(name)
}

#[tauri::command]
pub async fn create_fingerprint_with_profile(
    name: String,
    profile: models::DeviceProfile,
) -> Result<modules::fingerprint::Fingerprint, String> {
    modules::fingerprint::create_fingerprint_with_profile(name, profile)
}

#[tauri::command]
pub async fn apply_fingerprint(fingerprint_id: String) -> Result<String, String> {
    modules::fingerprint::apply_fingerprint(&fingerprint_id)
}

#[tauri::command]
pub async fn delete_fingerprint(fingerprint_id: String) -> Result<(), String> {
    modules::fingerprint::delete_fingerprint(&fingerprint_id)
}

#[tauri::command]
pub async fn rename_fingerprint(fingerprint_id: String, name: String) -> Result<(), String> {
    modules::fingerprint::rename_fingerprint(&fingerprint_id, name)
}

#[tauri::command]
pub async fn get_current_fingerprint_id() -> Result<Option<String>, String> {
    modules::fingerprint::get_current_fingerprint_id()
}
