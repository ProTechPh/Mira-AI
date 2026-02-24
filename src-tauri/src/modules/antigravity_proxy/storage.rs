use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::types::{ModelCacheState, ProxyAggregateStats, ProxyRequestLog};

const PROXY_DIR: &str = "antigravity_proxy";
const AGGREGATE_FILE: &str = "aggregate_stats.json";
const LOG_FILE: &str = "request_logs.json";
const MODEL_CACHE_FILE: &str = "models_cache.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedStats {
    pub aggregate: ProxyAggregateStats,
}

pub fn proxy_data_dir() -> Result<PathBuf, String> {
    let base = crate::modules::config::get_shared_dir();
    let dir = base.join(PROXY_DIR);
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| format!("创建 Antigravity Proxy 目录失败: {}", e))?;
    }
    Ok(dir)
}

fn aggregate_path() -> Result<PathBuf, String> {
    Ok(proxy_data_dir()?.join(AGGREGATE_FILE))
}

fn logs_path() -> Result<PathBuf, String> {
    Ok(proxy_data_dir()?.join(LOG_FILE))
}

fn models_cache_path() -> Result<PathBuf, String> {
    Ok(proxy_data_dir()?.join(MODEL_CACHE_FILE))
}

pub fn load_persisted_stats() -> Result<PersistedStats, String> {
    let path = aggregate_path()?;
    if !path.exists() {
        return Ok(PersistedStats::default());
    }

    let content = fs::read_to_string(&path).map_err(|e| format!("读取 Proxy 统计失败: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("解析 Proxy 统计失败: {}", e))
}

pub fn save_persisted_stats(stats: &PersistedStats) -> Result<(), String> {
    let path = aggregate_path()?;
    let content = serde_json::to_string_pretty(stats)
        .map_err(|e| format!("序列化 Proxy 统计失败: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("写入 Proxy 统计失败: {}", e))
}

pub fn load_request_logs() -> Result<Vec<ProxyRequestLog>, String> {
    let path = logs_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path).map_err(|e| format!("读取 Proxy 日志失败: {}", e))?;
    let logs: Vec<ProxyRequestLog> =
        serde_json::from_str(&content).map_err(|e| format!("解析 Proxy 日志失败: {}", e))?;
    Ok(logs)
}

pub fn save_request_logs(logs: &[ProxyRequestLog]) -> Result<(), String> {
    let path = logs_path()?;
    let content = serde_json::to_string(logs).map_err(|e| format!("序列化 Proxy 日志失败: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("写入 Proxy 日志失败: {}", e))
}

pub fn load_model_cache() -> Result<Option<ModelCacheState>, String> {
    let path = models_cache_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path).map_err(|e| format!("读取 Proxy 模型缓存失败: {}", e))?;
    let cache: ModelCacheState =
        serde_json::from_str(&content).map_err(|e| format!("解析 Proxy 模型缓存失败: {}", e))?;
    Ok(Some(cache))
}

pub fn save_model_cache(cache: &ModelCacheState) -> Result<(), String> {
    let path = models_cache_path()?;
    let content = serde_json::to_string(cache).map_err(|e| format!("序列化 Proxy 模型缓存失败: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("写入 Proxy 模型缓存失败: {}", e))
}
