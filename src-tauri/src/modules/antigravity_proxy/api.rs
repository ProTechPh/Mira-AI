use futures_util::StreamExt;
use serde_json::{json, Value};

use crate::models::Account;

use super::translator::{parse_content_from_response, parse_usage};
use super::types::{OpenAiUsage, ProxyModelView};

const CLOUD_CODE_DAILY_BASE_URL: &str = "https://daily-cloudcode-pa.googleapis.com";
const CLOUD_CODE_PROD_BASE_URL: &str = "https://cloudcode-pa.googleapis.com";
const FETCH_MODELS_PATH: &str = "/v1internal:fetchAvailableModels";
const GENERATE_PATH: &str = "/v1internal:generateContent";
const STREAM_PATH: &str = "/v1internal:streamGenerateContent?alt=sse";
const USER_AGENT: &str = "antigravity";

fn env_var_trimmed(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn resolve_cloud_code_base_url(account: &Account) -> String {
    if let Some(override_url) = env_var_trimmed("ANTIGRAVITY_CLOUD_CODE_URL_OVERRIDE") {
        return override_url;
    }
    if account.token.is_gcp_tos.unwrap_or(false) {
        return CLOUD_CODE_PROD_BASE_URL.to_string();
    }
    CLOUD_CODE_DAILY_BASE_URL.to_string()
}

fn create_client(timeout_secs: u64) -> reqwest::Client {
    crate::utils::http::create_client(timeout_secs)
}

pub async fn fetch_models(account: &Account) -> Result<Vec<ProxyModelView>, String> {
    let client = create_client(15);
    let url = format!("{}{}", resolve_cloud_code_base_url(account), FETCH_MODELS_PATH);
    let payload = if let Some(project) = account.token.project_id.as_ref() {
        json!({ "project": project })
    } else {
        json!({})
    };

    let response = client
        .post(url)
        .bearer_auth(&account.token.access_token)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::ACCEPT_ENCODING, "gzip")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("fetchAvailableModels 请求失败: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("UPSTREAM_STATUS:{}:{}", status.as_u16(), body));
    }

    let value: Value = response
        .json()
        .await
        .map_err(|e| format!("fetchAvailableModels 响应解析失败: {}", e))?;

    let mut models = Vec::new();
    if let Some(map) = value.get("models").and_then(|v| v.as_object()) {
        for (id, meta) in map {
            let name = meta
                .get("displayName")
                .and_then(|v| v.as_str())
                .unwrap_or(id)
                .to_string();
            models.push(ProxyModelView {
                id: id.clone(),
                name,
                description: "".to_string(),
                source: "antigravity-api".to_string(),
            });
        }
    }

    models.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(models)
}

pub async fn generate_content(account: &Account, payload: &Value) -> Result<(String, OpenAiUsage), String> {
    let client = create_client(90);
    let url = format!("{}{}", resolve_cloud_code_base_url(account), GENERATE_PATH);

    let response = client
        .post(url)
        .bearer_auth(&account.token.access_token)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::ACCEPT_ENCODING, "gzip")
        .json(payload)
        .send()
        .await
        .map_err(|e| format!("generateContent 请求失败: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("UPSTREAM_STATUS:{}:{}", status.as_u16(), body));
    }

    let value: Value = response
        .json()
        .await
        .map_err(|e| format!("generateContent 响应解析失败: {}", e))?;
    let content = parse_content_from_response(&value);
    let usage = parse_usage(value.get("response").and_then(|v| v.get("usageMetadata")));
    Ok((content, usage))
}

pub async fn stream_generate_content(
    account: &Account,
    payload: &Value,
    mut on_text: impl FnMut(String),
) -> Result<OpenAiUsage, String> {
    let client = create_client(120);
    let url = format!("{}{}", resolve_cloud_code_base_url(account), STREAM_PATH);

    let response = client
        .post(url)
        .bearer_auth(&account.token.access_token)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::ACCEPT_ENCODING, "gzip")
        .json(payload)
        .send()
        .await
        .map_err(|e| format!("streamGenerateContent 请求失败: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("UPSTREAM_STATUS:{}:{}", status.as_u16(), body));
    }

    let mut usage = OpenAiUsage::default();
    let mut pending = String::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| format!("读取流响应失败: {}", e))?;
        let text = String::from_utf8_lossy(&bytes);
        pending.push_str(&text);

        while let Some(pos) = pending.find('\n') {
            let line: String = pending.drain(..=pos).collect();
            let trimmed = line.trim();
            if !trimmed.starts_with("data:") {
                continue;
            }
            let payload_text = trimmed.trim_start_matches("data:").trim();
            if payload_text.is_empty() || payload_text == "[DONE]" {
                continue;
            }

            let Ok(value) = serde_json::from_str::<Value>(payload_text) else {
                continue;
            };

            let content = parse_content_from_response(&value);
            if !content.is_empty() {
                on_text(content);
            }

            let parsed_usage = parse_usage(value.get("response").and_then(|v| v.get("usageMetadata")));
            if parsed_usage.total_tokens > 0 || parsed_usage.prompt_tokens > 0 || parsed_usage.completion_tokens > 0 {
                usage = parsed_usage;
            }
        }
    }

    Ok(usage)
}
