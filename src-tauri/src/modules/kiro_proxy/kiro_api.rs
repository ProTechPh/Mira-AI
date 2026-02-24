use reqwest::Client;
use serde_json::Value;

use crate::models::kiro::KiroAccount;
use crate::modules::logger;
use crate::modules::kiro_proxy::account_pool::{extract_machine_id, extract_profile_arn};

use super::event_stream::parse_aws_event_stream;
use super::types::{KiroCallOutput, KiroStreamMessage, KiroStreamUsage, KiroToolUse, ProxyModelView};

const KIRO_VERSION: &str = "0.6.18";
const KIRO_DEFAULT_TIMEOUT_SECS: u64 = 75;

#[derive(Debug, Clone)]
pub struct UpstreamEndpoint {
    pub url: &'static str,
    pub origin: &'static str,
    pub amz_target: &'static str,
}

pub const ENDPOINT_CODEWHISPERER: UpstreamEndpoint = UpstreamEndpoint {
    url: "https://codewhisperer.us-east-1.amazonaws.com/generateAssistantResponse",
    origin: "AI_EDITOR",
    amz_target: "AmazonCodeWhispererStreamingService.GenerateAssistantResponse",
};

pub const ENDPOINT_AMAZONQ: UpstreamEndpoint = UpstreamEndpoint {
    url: "https://q.us-east-1.amazonaws.com/generateAssistantResponse",
    origin: "CLI",
    amz_target: "AmazonQDeveloperStreamingService.SendMessage",
};

pub fn ordered_endpoints(preferred: Option<&str>) -> Vec<UpstreamEndpoint> {
    let mut list = vec![ENDPOINT_CODEWHISPERER.clone(), ENDPOINT_AMAZONQ.clone()];
    if let Some(preferred) = preferred {
        let preferred_lower = preferred.to_ascii_lowercase();
        if preferred_lower.contains("amazonq") {
            list.swap(0, 1);
        }
    }
    list
}

fn parse_profile_region(profile_arn: Option<&str>) -> &'static str {
    let Some(profile_arn) = profile_arn else {
        return "us-east-1";
    };

    let mut segments = profile_arn.split(':');
    let prefix = segments.next().unwrap_or_default();
    if !prefix.eq_ignore_ascii_case("arn") {
        return "us-east-1";
    }

    let _partition = segments.next();
    let _service = segments.next();
    match segments.next().unwrap_or("us-east-1").trim() {
        "eu-central-1" => "eu-central-1",
        _ => "us-east-1",
    }
}

fn q_service_endpoint_for_account(account: &KiroAccount) -> String {
    let profile_arn = extract_profile_arn(account);
    let region = parse_profile_region(profile_arn.as_deref());
    format!("https://q.{}.amazonaws.com", region)
}

pub fn map_model_id(model: &str) -> String {
    let lower = model.to_ascii_lowercase();
    let mappings = [
        ("claude-sonnet-4.5", "claude-sonnet-4.5"),
        ("claude-sonnet-4-5", "claude-sonnet-4.5"),
        ("claude-sonnet-4", "claude-sonnet-4"),
        ("claude-haiku-4.5", "claude-haiku-4.5"),
        ("claude-haiku-4-5", "claude-haiku-4.5"),
        ("claude-3-5-sonnet", "claude-sonnet-4.5"),
        ("claude-3-sonnet", "claude-sonnet-4"),
        ("claude-3-haiku", "claude-haiku-4.5"),
        ("gpt-4", "claude-sonnet-4.5"),
        ("gpt-4o", "claude-sonnet-4.5"),
        ("gpt-3.5-turbo", "claude-sonnet-4.5"),
    ];

    for (key, value) in mappings {
        if lower.contains(key) {
            return value.to_string();
        }
    }

    "claude-sonnet-4.5".to_string()
}

fn social_user_agent(machine_id: Option<&str>) -> String {
    let suffix = match machine_id {
        Some(machine_id) if !machine_id.trim().is_empty() => {
            format!("KiroIDE-{}-{}", KIRO_VERSION, machine_id.trim())
        }
        _ => format!("KiroIDE-{}", KIRO_VERSION),
    };
    format!(
        "aws-sdk-js/1.0.18 ua/2.1 os/windows lang/js api/codewhispererstreaming/1.0.18 m/E {}",
        suffix
    )
}

fn social_amz_user_agent(machine_id: Option<&str>) -> String {
    match machine_id {
        Some(machine_id) if !machine_id.trim().is_empty() => {
            format!("aws-sdk-js/1.0.18 KiroIDE {} {}", KIRO_VERSION, machine_id.trim())
        }
        _ => format!("aws-sdk-js/1.0.18 KiroIDE-{}", KIRO_VERSION),
    }
}

fn cli_user_agent() -> &'static str {
    "aws-sdk-rust/1.3.9 os/macos lang/rust/1.87.0"
}

fn cli_amz_user_agent() -> &'static str {
    "aws-sdk-rust/1.3.9 ua/2.1 api/ssooidc/1.88.0 os/macos lang/rust/1.87.0 m/E app/AmazonQ-For-CLI"
}

fn is_idc(account: &KiroAccount) -> bool {
    account
        .login_provider
        .as_deref()
        .map(|provider| provider.to_ascii_lowercase().contains("idc"))
        .unwrap_or(false)
}

fn with_common_headers(
    builder: reqwest::RequestBuilder,
    endpoint: &UpstreamEndpoint,
    account: &KiroAccount,
) -> reqwest::RequestBuilder {
    let idc = is_idc(account);
    let machine_id = extract_machine_id(account);
    builder
        .header("Accept", "*/*")
        .header("Content-Type", "application/json")
        .header("X-Amz-Target", endpoint.amz_target)
        .header(
            "User-Agent",
            if idc {
                cli_user_agent().to_string()
            } else {
                social_user_agent(machine_id.as_deref())
            },
        )
        .header(
            "X-Amz-User-Agent",
            if idc {
                cli_amz_user_agent().to_string()
            } else {
                social_amz_user_agent(machine_id.as_deref())
            },
        )
        .header(
            "x-amzn-kiro-agent-mode",
            if idc { "vibe" } else { "spec" },
        )
        .header("x-amzn-codewhisperer-optout", "true")
        .header("Amz-Sdk-Request", "attempt=1; max=3")
        .header("Authorization", format!("Bearer {}", account.access_token))
}

fn client(timeout_secs: Option<u64>) -> Result<Client, String> {
    Client::builder()
        .gzip(true)
        .timeout(std::time::Duration::from_secs(
            timeout_secs.unwrap_or(KIRO_DEFAULT_TIMEOUT_SECS),
        ))
        .build()
        .map_err(|e| format!("创建 Kiro 请求客户端失败: {}", e))
}

fn apply_endpoint_origin(mut payload: Value, endpoint: &UpstreamEndpoint) -> Value {
    if let Some(origin) = payload
        .pointer_mut("/conversationState/currentMessage/userInputMessage/origin")
    {
        *origin = Value::String(endpoint.origin.to_string());
    }
    payload
}

pub async fn call_generate_assistant_response_stream(
    account: &KiroAccount,
    endpoint: &UpstreamEndpoint,
    payload: Value,
    input_chars: usize,
    on_message: impl FnMut(KiroStreamMessage),
) -> Result<KiroStreamUsage, String> {
    let payload = apply_endpoint_origin(payload, endpoint);
    let client = client(None)?;

    let response = with_common_headers(client.post(endpoint.url), endpoint, account)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("请求 Kiro 上游失败: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("UPSTREAM_STATUS:{}:{}", status.as_u16(), body));
    }

    parse_aws_event_stream(response, input_chars, on_message).await
}

pub async fn call_generate_assistant_response(
    account: &KiroAccount,
    endpoint: &UpstreamEndpoint,
    payload: Value,
) -> Result<KiroCallOutput, String> {
    let input_chars = payload.to_string().chars().count();
    let mut content = String::new();
    let mut tool_uses: Vec<KiroToolUse> = Vec::new();

    let usage = call_generate_assistant_response_stream(
        account,
        endpoint,
        payload,
        input_chars,
        |event| match event {
            KiroStreamMessage::Text { text } => {
                content.push_str(&text);
            }
            KiroStreamMessage::Thinking { text } => {
                content.push_str(&text);
            }
            KiroStreamMessage::ToolUse { tool_use } => {
                tool_uses.push(tool_use);
            }
        },
    )
    .await?;

    Ok(KiroCallOutput {
        content,
        tool_uses,
        usage,
    })
}

pub async fn list_available_models(account: &KiroAccount) -> Result<Vec<ProxyModelView>, String> {
    let base = q_service_endpoint_for_account(account);
    let endpoint = format!("{}/ListAvailableModels", base);
    logger::log_info(&format!(
        "[Kiro Models] Fetch start: account_id={}, endpoint={}",
        account.id, endpoint
    ));

    let client = client(Some(20))?;
    let response = client
        .get(endpoint)
        .query(&[("origin", "AI_EDITOR"), ("maxResults", "100")])
        .header("Authorization", format!("Bearer {}", account.access_token))
        .send()
        .await
        .map_err(|e| {
            let err = format!("请求 ListAvailableModels 失败: {}", e);
            logger::log_warn(&format!(
                "[Kiro Models] Fetch request error: account_id={}, error={}",
                account.id, err
            ));
            err
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        logger::log_warn(&format!(
            "[Kiro Models] Fetch failed: account_id={}, status={}, body={}",
            account.id,
            status.as_u16(),
            body
        ));
        return Err(format!("UPSTREAM_STATUS:{}:{}", status.as_u16(), body));
    }

    let json: Value = response
        .json()
        .await
        .map_err(|e| format!("解析 ListAvailableModels 响应失败: {}", e))?;

    let mut models = Vec::new();
    if let Some(items) = json.get("models").and_then(|v| v.as_array()) {
        for item in items {
            let id = item
                .get("modelId")
                .or_else(|| item.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if id.is_empty() {
                continue;
            }
            let name = item
                .get("modelName")
                .or_else(|| item.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string();
            let description = item
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            models.push(ProxyModelView {
                id,
                name,
                description,
                source: "kiro-api".to_string(),
            });
        }
    }
    logger::log_info(&format!(
        "[Kiro Models] Fetch success: account_id={}, count={}",
        account.id,
        models.len()
    ));

    Ok(models)
}
