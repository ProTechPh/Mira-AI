use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatus {
    pub running: bool,
    pub host: String,
    pub port: u16,
    pub started_at: Option<i64>,
    pub uptime_seconds: Option<i64>,
    pub request_count: u64,
    pub success_count: u64,
    pub failed_count: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_credits: f64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyAccountView {
    pub id: String,
    pub email: String,
    pub enabled: bool,
    pub status: Option<String>,
    pub status_reason: Option<String>,
    pub last_used: i64,
    pub request_count: u64,
    pub error_count: u64,
    pub cooldown_until: Option<i64>,
    pub profile_arn: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyModelView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRequestLog {
    pub timestamp: i64,
    pub path: String,
    pub method: String,
    pub model: Option<String>,
    pub account_id: Option<String>,
    pub account_email: Option<String>,
    pub api_key_id: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub credits: f64,
    pub response_time_ms: u64,
    pub status: u16,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyModelStats {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub credits: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyDailyStats {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub credits: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyAggregateStats {
    pub total_requests: u64,
    pub success_requests: u64,
    pub failed_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_credits: f64,
    pub by_model: HashMap<String, ProxyModelStats>,
    pub daily: HashMap<String, ProxyDailyStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyAdminStatsResponse {
    pub status: ProxyStatus,
    pub aggregate: ProxyAggregateStats,
    pub accounts: Vec<ProxyAccountView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyAdminLogsResponse {
    pub logs: Vec<ProxyRequestLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyUsageRecord {
    pub timestamp: i64,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub credits: f64,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyApiKeyUsageDaily {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub credits: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyApiKeyUsageModel {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub credits: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyApiKeyUsage {
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_credits: f64,
    pub daily: HashMap<String, ProxyApiKeyUsageDaily>,
    pub by_model: HashMap<String, ProxyApiKeyUsageModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyApiKeyView {
    pub id: String,
    pub name: String,
    pub key_preview: String,
    pub enabled: bool,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub credits_limit: Option<f64>,
    pub usage: ProxyApiKeyUsage,
    pub usage_history: Vec<ProxyUsageRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AddApiKeyInput {
    pub name: String,
    pub key: String,
    pub enabled: Option<bool>,
    pub credits_limit: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateApiKeyInput {
    pub id: String,
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub credits_limit: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KiroStreamUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub credits: f64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KiroToolUse {
    pub tool_use_id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KiroStreamMessage {
    Text { text: String },
    Thinking { text: String },
    ToolUse { tool_use: KiroToolUse },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KiroCallOutput {
    pub content: String,
    pub tool_uses: Vec<KiroToolUse>,
    pub usage: KiroStreamUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChatRequest {
    pub model: String,
    pub messages: Vec<OpenAiMessage>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(default)]
    pub tools: Option<Vec<OpenAiTool>>,
    #[serde(default)]
    pub tool_choice: Option<Value>,
    #[serde(default)]
    pub response_format: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    pub content: Value,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiTool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: OpenAiToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiToolFunction {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: OpenAiToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub messages: Vec<ClaudeMessage>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(default)]
    pub system: Option<Value>,
    #[serde(default)]
    pub tools: Option<Vec<ClaudeTool>>,
    #[serde(default)]
    pub tool_choice: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeMessage {
    pub role: String,
    pub content: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeTool {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub input_schema: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelCacheState {
    pub updated_at: i64,
    pub models: Vec<ProxyModelView>,
}
