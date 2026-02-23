//! 托盘平台布局配置
//! 用于控制托盘中平台的显示与排序模式

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const TRAY_LAYOUT_FILE: &str = "tray_layout.json";

pub const PLATFORM_ANTIGRAVITY: &str = "antigravity";
pub const PLATFORM_CODEX: &str = "codex";
pub const PLATFORM_GITHUB_COPILOT: &str = "github-copilot";
pub const PLATFORM_WINDSURF: &str = "windsurf";
pub const PLATFORM_KIRO: &str = "kiro";

pub const SUPPORTED_PLATFORM_IDS: [&str; 5] = [
    PLATFORM_ANTIGRAVITY,
    PLATFORM_CODEX,
    PLATFORM_GITHUB_COPILOT,
    PLATFORM_WINDSURF,
    PLATFORM_KIRO,
];

pub const SORT_MODE_AUTO: &str = "auto";
pub const SORT_MODE_MANUAL: &str = "manual";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayLayoutConfig {
    #[serde(default = "default_sort_mode")]
    pub sort_mode: String,
    #[serde(default = "default_order")]
    pub ordered_platform_ids: Vec<String>,
    #[serde(default = "default_tray_platforms")]
    pub tray_platform_ids: Vec<String>,
}

fn default_sort_mode() -> String {
    SORT_MODE_AUTO.to_string()
}

fn default_order() -> Vec<String> {
    SUPPORTED_PLATFORM_IDS
        .iter()
        .map(|id| (*id).to_string())
        .collect()
}

fn default_tray_platforms() -> Vec<String> {
    default_order()
}

impl Default for TrayLayoutConfig {
    fn default() -> Self {
        Self {
            sort_mode: default_sort_mode(),
            ordered_platform_ids: default_order(),
            tray_platform_ids: default_tray_platforms(),
        }
    }
}

fn get_tray_layout_path() -> Result<PathBuf, String> {
    Ok(crate::modules::account::get_data_dir()?.join(TRAY_LAYOUT_FILE))
}

fn is_supported_platform_id(id: &str) -> bool {
    SUPPORTED_PLATFORM_IDS.contains(&id)
}

fn sanitize_platform_ids(ids: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    for id in ids {
        let trimmed = id.trim();
        if trimmed.is_empty() || !is_supported_platform_id(trimmed) {
            continue;
        }
        if result.iter().any(|existing| existing == trimmed) {
            continue;
        }
        result.push(trimmed.to_string());
    }
    result
}

fn normalize_order(ids: &[String]) -> Vec<String> {
    let mut ordered = sanitize_platform_ids(ids);
    for default_id in SUPPORTED_PLATFORM_IDS {
        if !ordered.iter().any(|id| id == default_id) {
            ordered.push(default_id.to_string());
        }
    }
    ordered
}

fn contains_platform(ids: &[String], target: &str) -> bool {
    ids.iter().any(|id| id == target)
}

fn normalize_tray_platforms(ids: &[String], raw_order_has_kiro: bool) -> Vec<String> {
    let mut sanitized = sanitize_platform_ids(ids);

    // 兼容旧版本（无 Kiro）配置：
    // 仅当旧配置明确包含历史四平台且未出现 Kiro 时，自动补上 Kiro 到托盘显示列表。
    // 若配置本身已包含 Kiro（或顺序已是新版），则尊重用户当前选择，不强制补回。
    let has_kiro = contains_platform(&sanitized, PLATFORM_KIRO);
    let has_legacy_all = contains_platform(&sanitized, PLATFORM_ANTIGRAVITY)
        && contains_platform(&sanitized, PLATFORM_CODEX)
        && contains_platform(&sanitized, PLATFORM_GITHUB_COPILOT)
        && contains_platform(&sanitized, PLATFORM_WINDSURF);
    let is_legacy_default = sanitized.len() == 4 && has_legacy_all;

    if !raw_order_has_kiro && !has_kiro && is_legacy_default {
        sanitized.push(PLATFORM_KIRO.to_string());
    }

    sanitized
}

fn normalize_sort_mode(raw: &str) -> String {
    match raw.trim() {
        SORT_MODE_MANUAL => SORT_MODE_MANUAL.to_string(),
        _ => SORT_MODE_AUTO.to_string(),
    }
}

fn normalize_config(config: TrayLayoutConfig) -> TrayLayoutConfig {
    let raw_order_has_kiro = config
        .ordered_platform_ids
        .iter()
        .any(|id| id.trim() == PLATFORM_KIRO);
    TrayLayoutConfig {
        sort_mode: normalize_sort_mode(&config.sort_mode),
        ordered_platform_ids: normalize_order(&config.ordered_platform_ids),
        tray_platform_ids: normalize_tray_platforms(&config.tray_platform_ids, raw_order_has_kiro),
    }
}

pub fn load_tray_layout() -> TrayLayoutConfig {
    let path = match get_tray_layout_path() {
        Ok(path) => path,
        Err(_) => return TrayLayoutConfig::default(),
    };

    if !path.exists() {
        return TrayLayoutConfig::default();
    }

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return TrayLayoutConfig::default(),
    };

    match serde_json::from_str::<TrayLayoutConfig>(&content) {
        Ok(config) => normalize_config(config),
        Err(_) => TrayLayoutConfig::default(),
    }
}

pub fn save_tray_layout(
    sort_mode: String,
    ordered_platform_ids: Vec<String>,
    tray_platform_ids: Vec<String>,
) -> Result<TrayLayoutConfig, String> {
    let normalized = normalize_config(TrayLayoutConfig {
        sort_mode,
        ordered_platform_ids,
        tray_platform_ids,
    });

    let path = get_tray_layout_path()?;
    let content = serde_json::to_string_pretty(&normalized)
        .map_err(|e| format!("序列化托盘布局配置失败: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("保存托盘布局配置失败: {}", e))?;
    Ok(normalized)
}
