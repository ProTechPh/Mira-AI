//! 分组配置相关命令

use crate::modules::group_settings::{self, GroupSettings};
use std::collections::HashMap;

/// 获取分组配置
#[tauri::command]
pub fn get_group_settings() -> Result<GroupSettings, String> {
    Ok(group_settings::load_group_settings())
}

/// 保存分组配置
#[tauri::command]
#[allow(non_snake_case)]
pub fn save_group_settings(
    groupMappings: HashMap<String, String>,
    groupNames: HashMap<String, String>,
    groupOrder: Vec<String>,
) -> Result<(), String> {
    let mut settings = group_settings::load_group_settings();
    settings.group_mappings = groupMappings;
    settings.group_names = groupNames;
    settings.group_order = groupOrder;
    settings.updated_at = chrono::Utc::now().timestamp_millis();
    settings.updated_by = group_settings::ConfigSource::Desktop;

    group_settings::update_group_settings(settings)
}

/// 设置模型的分组
#[tauri::command]
#[allow(non_snake_case)]
pub fn set_model_group(modelId: String, groupId: String) -> Result<(), String> {
    let mut settings = group_settings::load_group_settings();
    settings.set_model_group(&modelId, &groupId);
    group_settings::update_group_settings(settings)
}

/// 移除模型的分组
#[tauri::command]
#[allow(non_snake_case)]
pub fn remove_model_group(modelId: String) -> Result<(), String> {
    let mut settings = group_settings::load_group_settings();
    settings.remove_model_group(&modelId);
    group_settings::update_group_settings(settings)
}

/// 设置分组名称
#[tauri::command]
#[allow(non_snake_case)]
pub fn set_group_name(groupId: String, name: String) -> Result<(), String> {
    let mut settings = group_settings::load_group_settings();
    settings.set_group_name(&groupId, &name);
    group_settings::update_group_settings(settings)
}

/// 删除分组
#[tauri::command]
#[allow(non_snake_case)]
pub fn delete_group(groupId: String) -> Result<(), String> {
    let mut settings = group_settings::load_group_settings();
    settings.delete_group(&groupId);
    group_settings::update_group_settings(settings)
}

/// 更新分组排序
#[tauri::command]
pub fn update_group_order(order: Vec<String>) -> Result<(), String> {
    let mut settings = group_settings::load_group_settings();
    settings.set_group_order(order);
    group_settings::update_group_settings(settings)
}

/// 获取排序后的分组列表（最多4个，用于桌面端显示）
#[tauri::command]
pub fn get_display_groups() -> Result<Vec<DisplayGroup>, String> {
    let settings = group_settings::load_group_settings();
    let group_ids = settings.get_ordered_groups(Some(4));

    let groups: Vec<DisplayGroup> = group_ids
        .iter()
        .map(|gid| {
            let models = settings.get_models_in_group(gid);
            DisplayGroup {
                id: gid.clone(),
                name: settings.get_group_name(gid),
                models,
            }
        })
        .collect();

    Ok(groups)
}

/// 显示用分组信息
#[derive(serde::Serialize)]
pub struct DisplayGroup {
    pub id: String,
    pub name: String,
    pub models: Vec<String>,
}
