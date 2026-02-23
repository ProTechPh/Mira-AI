use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::modules;

const CLOUD_CODE_BASE_URLS: [&str; 3] = [
    "https://daily-cloudcode-pa.googleapis.com",
    "https://cloudcode-pa.googleapis.com",
    "https://daily-cloudcode-pa.sandbox.googleapis.com",
];
const STREAM_PATH: &str = "/v1internal:streamGenerateContent?alt=sse";
const FETCH_MODELS_PATH: &str = "/v1internal:fetchAvailableModels";
const USER_AGENT: &str = "antigravity";
const ANTIGRAVITY_SYSTEM_PROMPT: &str = "You are Antigravity, a powerful agentic AI coding assistant designed by the Google Deepmind team working on Advanced Agentic Coding.You are pair programming with a USER to solve their coding task. The task may require creating a new codebase, modifying or debugging an existing codebase, or simply answering a question.**Absolute paths only****Proactiveness**";
const DEFAULT_ATTEMPTS: usize = 2;
const BACKOFF_BASE_MS: u64 = 500;
const BACKOFF_MAX_MS: u64 = 4000;
static BASE_URL_ORDER: OnceLock<Mutex<Vec<&'static str>>> = OnceLock::new();

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WakeupResponse {
    pub reply: String,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    pub trace_id: Option<String>,
    pub response_id: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug)]
struct StreamParseResult {
    reply: String,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
    trace_id: Option<String>,
    response_id: Option<String>,
}

fn random_suffix(len: usize) -> String {
    let charset: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| charset[rng.gen_range(0..charset.len())] as char)
        .collect()
}

fn format_prompt_for_log(prompt: &str) -> String {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return "<empty>".to_string();
    }
    const MAX_LEN: usize = 60;
    let mut preview = trimmed.chars().take(MAX_LEN).collect::<String>();
    if trimmed.chars().count() > MAX_LEN {
        preview.push_str("...");
    }
    preview
}

fn generate_session_id() -> String {
    let timestamp = chrono::Utc::now().timestamp_millis();
    format!("sess_{}_{}", timestamp, random_suffix(6))
}

fn generate_request_id() -> String {
    let timestamp = chrono::Utc::now().timestamp_millis();
    format!("req_{}_{}", timestamp, random_suffix(6))
}

fn generate_fallback_project_id() -> String {
    format!("projects/random-{}/locations/global", random_suffix(8))
}

fn build_request_body(
    project_id: &str,
    model: &str,
    prompt: &str,
    max_output_tokens: u32,
) -> serde_json::Value {
    let request_id = generate_request_id();
    let session_id = generate_session_id();
    let mut generation_config = json!({ "temperature": 0 });
    if max_output_tokens > 0 {
        if let Some(obj) = generation_config.as_object_mut() {
            obj.insert("maxOutputTokens".to_string(), json!(max_output_tokens));
        }
    }

    json!({
        "project": project_id,
        "requestId": request_id,
        "model": model,
        "userAgent": "antigravity",
        "requestType": "agent",
        "request": {
            "contents": [
                { "role": "user", "parts": [ { "text": prompt } ] }
            ],
            "session_id": session_id,
            "systemInstruction": {
                "parts": [ { "text": ANTIGRAVITY_SYSTEM_PROMPT } ]
            },
            "generationConfig": generation_config
        }
    })
}

fn get_backoff_delay_ms(attempt: usize) -> u64 {
    if attempt < 2 {
        return 0;
    }
    let raw = BACKOFF_BASE_MS.saturating_mul(2u64.saturating_pow((attempt - 2) as u32));
    let jitter = rand::thread_rng().gen_range(0..100);
    std::cmp::min(raw + jitter, BACKOFF_MAX_MS)
}

fn get_base_url_order() -> Vec<&'static str> {
    let lock = BASE_URL_ORDER.get_or_init(|| Mutex::new(CLOUD_CODE_BASE_URLS.to_vec()));
    match lock.lock() {
        Ok(list) => list.clone(),
        Err(_) => CLOUD_CODE_BASE_URLS.to_vec(),
    }
}

fn promote_base_url(base: &'static str) {
    let lock = BASE_URL_ORDER.get_or_init(|| Mutex::new(CLOUD_CODE_BASE_URLS.to_vec()));
    if let Ok(mut list) = lock.lock() {
        if let Some(pos) = list.iter().position(|item| *item == base) {
            list.remove(pos);
            list.insert(0, base);
        }
    }
}

fn truncate_log_text(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }
    let mut preview = text.chars().take(max_len).collect::<String>();
    preview.push_str("...");
    preview
}

fn process_stream_object(
    obj: &serde_json::Value,
    reply_parts: &mut Vec<String>,
    prompt_tokens: &mut Option<u32>,
    completion_tokens: &mut Option<u32>,
    total_tokens: &mut Option<u32>,
    trace_id: &mut Option<String>,
    response_id: &mut Option<String>,
) {
    let candidate = obj
        .get("response")
        .and_then(|value| value.get("candidates"))
        .and_then(|value| value.get(0))
        .or_else(|| obj.get("candidates").and_then(|value| value.get(0)));

    if let Some(parts) = candidate
        .and_then(|value| value.get("content"))
        .and_then(|value| value.get("parts"))
        .and_then(|value| value.as_array())
    {
        for part in parts {
            if part.get("thought").and_then(|value| value.as_bool()) == Some(true) {
                continue;
            }
            if let Some(text) = part.get("text").and_then(|value| value.as_str()) {
                if !text.is_empty() {
                    reply_parts.push(text.to_string());
                }
            }
        }
    }

    if prompt_tokens.is_none() || completion_tokens.is_none() || total_tokens.is_none() {
        let usage = obj
            .get("response")
            .and_then(|value| value.get("usageMetadata"))
            .or_else(|| obj.get("usageMetadata"));
        if let Some(usage) = usage {
            if prompt_tokens.is_none() {
                *prompt_tokens = usage
                    .get("promptTokenCount")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as u32);
            }
            if completion_tokens.is_none() {
                *completion_tokens = usage
                    .get("candidatesTokenCount")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as u32);
            }
            if total_tokens.is_none() {
                *total_tokens = usage
                    .get("totalTokenCount")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as u32);
            }
        }
    }

    if trace_id.is_none() {
        *trace_id = obj
            .get("traceId")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());
    }
    if response_id.is_none() {
        *response_id = obj
            .get("response")
            .and_then(|value| value.get("responseId"))
            .or_else(|| obj.get("responseId"))
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());
    }
}

fn parse_stream_result(text: &str) -> Result<StreamParseResult, String> {
    let mut reply_parts: Vec<String> = Vec::new();
    let mut prompt_tokens: Option<u32> = None;
    let mut completion_tokens: Option<u32> = None;
    let mut total_tokens: Option<u32> = None;
    let mut trace_id: Option<String> = None;
    let mut response_id: Option<String> = None;
    let mut got_event = false;
    let mut last_data: Option<serde_json::Value> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let payload = if trimmed.starts_with("data:") {
            let payload = trimmed.trim_start_matches("data:").trim();
            if payload.is_empty() || payload == "[DONE]" {
                continue;
            }
            Some(payload)
        } else if trimmed.starts_with('{') || trimmed.starts_with('[') {
            Some(trimmed)
        } else {
            None
        };

        if let Some(payload) = payload {
            got_event = true;
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) {
                process_stream_object(
                    &value,
                    &mut reply_parts,
                    &mut prompt_tokens,
                    &mut completion_tokens,
                    &mut total_tokens,
                    &mut trace_id,
                    &mut response_id,
                );
                last_data = Some(value);
            }
        }
    }

    if !got_event {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
            got_event = true;
            process_stream_object(
                &value,
                &mut reply_parts,
                &mut prompt_tokens,
                &mut completion_tokens,
                &mut total_tokens,
                &mut trace_id,
                &mut response_id,
            );
        }
    }

    if !got_event {
        return Err("Cloud Code stream received no data".to_string());
    }

    if reply_parts.is_empty() {
        if let Some(value) = last_data.as_ref() {
            process_stream_object(
                value,
                &mut reply_parts,
                &mut prompt_tokens,
                &mut completion_tokens,
                &mut total_tokens,
                &mut trace_id,
                &mut response_id,
            );
        }
    }

    let reply = if reply_parts.is_empty() {
        "(无回复)".to_string()
    } else {
        reply_parts.join("")
    };
    if completion_tokens.is_none() {
        completion_tokens = Some(0);
    }

    Ok(StreamParseResult {
        reply,
        prompt_tokens,
        completion_tokens,
        total_tokens,
        trace_id,
        response_id,
    })
}

async fn send_stream_request(
    client: &reqwest::Client,
    access_token: &str,
    body: &serde_json::Value,
) -> Result<StreamParseResult, String> {
    let mut last_error: Option<String> = None;
    for base in get_base_url_order() {
        for attempt in 1..=DEFAULT_ATTEMPTS {
            let url = format!("{}{}", base, STREAM_PATH);
            crate::modules::logger::log_info(&format!(
                "[Wakeup] 发送请求: url={}, attempt={}/{}",
                url, attempt, DEFAULT_ATTEMPTS
            ));
            let response = client
                .post(&url)
                .bearer_auth(access_token)
                .header(reqwest::header::USER_AGENT, USER_AGENT)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .header(reqwest::header::ACCEPT_ENCODING, "gzip")
                .json(body)
                .send()
                .await;

            match response {
                Ok(res) => {
                    let status = res.status();
                    if status.is_success() {
                        let text = res.text().await.unwrap_or_default();
                        crate::modules::logger::log_info(&format!(
                            "[Wakeup] stream响应: {}",
                            truncate_log_text(&text, 2000)
                        ));
                        match parse_stream_result(&text) {
                            Ok(parsed) => {
                                promote_base_url(base);
                                crate::modules::logger::log_info(&format!(
                                    "[Wakeup] 请求成功: url={}, status={}",
                                    url, status
                                ));
                                return Ok(parsed);
                            }
                            Err(err) => {
                                last_error = Some(err.clone());
                                crate::modules::logger::log_warn(&format!(
                                    "[Wakeup] 解析响应失败: url={}, error={}",
                                    url, err
                                ));
                                if attempt < DEFAULT_ATTEMPTS {
                                    let delay = get_backoff_delay_ms(attempt + 1);
                                    if delay > 0 {
                                        crate::modules::logger::log_info(&format!(
                                            "[Wakeup] 准备重试: delay={}ms",
                                            delay
                                        ));
                                        tokio::time::sleep(std::time::Duration::from_millis(delay))
                                            .await;
                                    }
                                    continue;
                                }
                            }
                        }
                    } else {
                        if status == reqwest::StatusCode::UNAUTHORIZED {
 crate::modules::logger::log_error("[Wakeup] (401)");
                            return Err("Authorization expired".to_string());
                        }
                        if status == reqwest::StatusCode::FORBIDDEN {
 crate::modules::logger::log_error("[Wakeup] (403)");
                            return Err("Cloud Code access forbidden".to_string());
                        }
                        let text = res.text().await.unwrap_or_default();
                        let retryable = status == reqwest::StatusCode::TOO_MANY_REQUESTS
                            || status.as_u16() >= 500;
                        let message = format!("唤醒请求失败: {} - {}", status, text);
                        last_error = Some(message.clone());
                        crate::modules::logger::log_warn(&format!(
                            "[Wakeup] 请求失败: url={}, status={}, retryable={}",
                            url, status, retryable
                        ));
                        if retryable && attempt < DEFAULT_ATTEMPTS {
                            let delay = get_backoff_delay_ms(attempt + 1);
                            if delay > 0 {
                                crate::modules::logger::log_info(&format!(
                                    "[Wakeup] 准备重试: delay={}ms",
                                    delay
                                ));
                                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                            }
                            continue;
                        }
                    }
                }
                Err(err) => {
                    last_error = Some(format!("唤醒请求失败: {}", err));
                    crate::modules::logger::log_warn(&format!(
                        "[Wakeup] 网络错误: url={}, error={}",
                        url, err
                    ));
                    if attempt < DEFAULT_ATTEMPTS {
                        let delay = get_backoff_delay_ms(attempt + 1);
                        if delay > 0 {
                            crate::modules::logger::log_info(&format!(
                                "[Wakeup] 准备重试: delay={}ms",
                                delay
                            ));
                            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        }
                        continue;
                    }
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "唤醒请求失败".to_string()))
}

/// 触发单个账号的唤醒请求
pub async fn trigger_wakeup(
    account_id: &str,
    model: &str,
    prompt: &str,
    max_output_tokens: u32,
) -> Result<WakeupResponse, String> {
    let mut account = modules::load_account(account_id)?;
    crate::modules::logger::log_info(&format!(
        "[Wakeup] 开始唤醒: email={}, model={}, max_tokens={}, prompt={}",
        account.email,
        model,
        max_output_tokens,
        format_prompt_for_log(prompt)
    ));
    let mut token = modules::oauth::ensure_fresh_token(&account.token).await?;

    let (project_id, _) =
        modules::quota::fetch_project_id_for_token(&token, &account.email).await;
    let final_project_id = project_id
        .clone()
        .or_else(|| token.project_id.clone())
        .unwrap_or_else(generate_fallback_project_id);
 crate::modules::logger::log_info(&format!("[Wakeup] ID: {}", final_project_id));

    if token.project_id.is_none() && project_id.is_some() {
        token.project_id = project_id.clone();
    }

    if token.access_token != account.token.access_token
        || token.expiry_timestamp != account.token.expiry_timestamp
        || token.project_id != account.token.project_id
    {
        account.token = token.clone();
        let _ = modules::save_account(&account);
    }

    let client = crate::utils::http::create_client(15);
    let body = build_request_body(&final_project_id, model, prompt, max_output_tokens);
    let started = std::time::Instant::now();

    match send_stream_request(&client, &token.access_token, &body).await {
        Ok(parsed) => {
            let duration_ms = started.elapsed().as_millis() as u64;
            crate::modules::logger::log_info(&format!(
                "[Wakeup] 唤醒完成: duration={}ms",
                duration_ms
            ));
            Ok(WakeupResponse {
                reply: parsed.reply,
                prompt_tokens: parsed.prompt_tokens,
                completion_tokens: parsed.completion_tokens,
                total_tokens: parsed.total_tokens,
                trace_id: parsed.trace_id,
                response_id: parsed.response_id,
                duration_ms,
            })
        }
        Err(err) => {
 crate::modules::logger::log_error(&format!("[Wakeup] failed: {}", err));
            Err(err)
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableModel {
    pub id: String,
    pub display_name: String,
    pub model_constant: Option<String>,
    pub recommended: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct AvailableModelsResponse {
    models: Option<HashMap<String, AvailableModelMeta>>,
}

#[derive(Debug, Deserialize)]
struct AvailableModelMeta {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "model")]
    model_constant: Option<String>,
    #[serde(rename = "recommended")]
    recommended: Option<bool>,
}

/// 获取可用模型列表（用于唤醒配置）
pub async fn fetch_available_models() -> Result<Vec<AvailableModel>, String> {
    let current = modules::get_current_account()?;
    let account = if let Some(account) = current {
        account
    } else {
        let accounts = modules::list_accounts()?;
        accounts
            .into_iter()
            .next()
            .ok_or_else(|| "未找到可用账号".to_string())?
    };

    let token = modules::oauth::ensure_fresh_token(&account.token).await?;
    if token.access_token != account.token.access_token
        || token.expiry_timestamp != account.token.expiry_timestamp
    {
        let mut updated = account.clone();
        updated.token = token.clone();
        let _ = modules::save_account(&updated);
    }

    let payload = json!({});

    let client = crate::utils::http::create_client(15);
    let mut last_error: Option<String> = None;
    let mut data: Option<AvailableModelsResponse> = None;
    'outer: for base in CLOUD_CODE_BASE_URLS {
        for attempt in 1..=DEFAULT_ATTEMPTS {
            let url = format!("{}{}", base, FETCH_MODELS_PATH);
            let response = client
                .post(url)
                .bearer_auth(&token.access_token)
                .header(reqwest::header::USER_AGENT, USER_AGENT)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .header(reqwest::header::ACCEPT_ENCODING, "gzip")
                .json(&payload)
                .send()
                .await;

            match response {
                Ok(res) => {
                    if res.status().is_success() {
                        let parsed: AvailableModelsResponse = res
                            .json()
                            .await
                            .map_err(|e| format!("解析模型列表失败: {}", e))?;
                        data = Some(parsed);
                        break 'outer;
                    }
                    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
                        return Err("Authorization expired".to_string());
                    }
                    if res.status() == reqwest::StatusCode::FORBIDDEN {
                        return Err("Cloud Code access forbidden".to_string());
                    }
                    let status = res.status();
                    let text = res.text().await.unwrap_or_default();
                    let retryable =
                        status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.as_u16() >= 500;
                    last_error = Some(format!("获取模型列表失败: {} - {}", status, text));
                    if retryable && attempt < DEFAULT_ATTEMPTS {
                        let delay = get_backoff_delay_ms(attempt + 1);
                        if delay > 0 {
                            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        }
                        continue;
                    }
                }
                Err(err) => {
                    last_error = Some(format!("获取模型列表失败: {}", err));
                    if attempt < DEFAULT_ATTEMPTS {
                        let delay = get_backoff_delay_ms(attempt + 1);
                        if delay > 0 {
                            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        }
                        continue;
                    }
                }
            }
        }
    }

    let data = data.ok_or_else(|| last_error.unwrap_or_else(|| "获取模型列表失败".to_string()))?;

    let mut models = Vec::new();
    if let Some(entries) = data.models {
        for (id, meta) in entries {
            let display_name = meta.display_name.clone().unwrap_or_else(|| id.clone());
            models.push(AvailableModel {
                id,
                display_name,
                model_constant: meta.model_constant.clone(),
                recommended: meta.recommended,
            });
        }
    }

    models.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(models)
}
