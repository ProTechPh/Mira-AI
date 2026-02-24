use base64::Engine;
use rand::Rng;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::models::kiro::{
    KiroAccount, KiroOAuthCompletePayload, KiroOAuthStartOptions, KiroOAuthStartResponse,
};
use crate::modules::{kiro_account, logger};

const KIRO_AUTH_PORTAL_URL: &str = "https://app.kiro.dev/signin";
const KIRO_TOKEN_ENDPOINT: &str = "https://prod.us-east-1.auth.desktop.kiro.dev/oauth/token";
const KIRO_RUNTIME_DEFAULT_ENDPOINT: &str = "https://q.us-east-1.amazonaws.com";
const KIRO_ACCOUNT_STATUS_NORMAL: &str = "normal";
const KIRO_ACCOUNT_STATUS_BANNED: &str = "banned";
const KIRO_ACCOUNT_STATUS_ERROR: &str = "error";
const OAUTH_TIMEOUT_SECONDS: u64 = 600;
const OAUTH_POLL_INTERVAL_MS: u64 = 250;
const BUILDER_ID_START_URL: &str = "https://view.awsapps.com/start";
const CALLBACK_PORT_CANDIDATES: [u16; 10] = [
    3128, 4649, 6588, 8008, 9091, 49153, 50153, 51153, 52153, 53153,
];

#[derive(Clone, Debug)]
struct OAuthCallbackData {
    login_option: String,
    code: Option<String>,
    issuer_url: Option<String>,
    idc_region: Option<String>,
    path: String,
    client_id: Option<String>,
    scopes: Option<String>,
    login_hint: Option<String>,
    audience: Option<String>,
}

#[derive(Clone)]
struct PendingOAuthState {
    flow: OAuthFlow,
    login_id: String,
    expires_at: i64,
    verification_uri: String,
    verification_uri_complete: String,
    callback_url: String,
    callback_port: u16,
    state_token: String,
    code_verifier: String,
    region: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    device_code: Option<String>,
    interval_seconds: u64,
    callback_result: Option<Result<OAuthCallbackData, String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum OAuthFlow {
    Portal,
    BuilderId,
    IamSso,
}

lazy_static::lazy_static! {
    static ref PENDING_OAUTH_STATE: Arc<Mutex<Option<PendingOAuthState>>> = Arc::new(Mutex::new(None));
}

fn now_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}

fn generate_token() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..24).map(|_| rng.gen::<u8>()).collect();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn generate_code_challenge(code_verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let digest = hasher.finalize();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn normalize_non_empty(value: Option<&str>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_email(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() || !trimmed.contains('@') {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn set_payload_status(payload: &mut KiroOAuthCompletePayload, status: &str, reason: Option<String>) {
    payload.status = Some(status.to_string());
    payload.status_reason = reason.and_then(|raw| normalize_non_empty(Some(raw.as_str())));
}

fn parse_runtime_error_reason(body: &str) -> Option<String> {
    let parsed = serde_json::from_str::<Value>(body).ok()?;
    let direct_reason = pick_string(
        Some(&parsed),
        &[
            &["reason"],
            &["message"],
            &["errorMessage"],
            &["error", "message"],
            &["error", "reason"],
            &["detail"],
            &["details"],
        ],
    );
    if let Some(reason) = direct_reason.and_then(|raw| normalize_non_empty(Some(raw.as_str()))) {
        return Some(reason);
    }

    if let Some(code) = pick_string(
        Some(&parsed),
        &[&["error"], &["code"], &["errorCode"], &["error", "code"]],
    )
    .and_then(|raw| normalize_non_empty(Some(raw.as_str())))
    {
        return Some(code);
    }

    None
}

fn parse_banned_reason_from_error(err: &str) -> Option<String> {
    err.strip_prefix("BANNED:")
        .and_then(|raw| normalize_non_empty(Some(raw)))
}

fn get_path_value<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = root;
    for key in path {
        current = current.as_object()?.get(*key)?;
    }
    Some(current)
}

fn pick_string(root: Option<&Value>, paths: &[&[&str]]) -> Option<String> {
    let root = root?;
    for path in paths {
        if let Some(value) = get_path_value(root, path) {
            if let Some(text) = value.as_str() {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
            if let Some(num) = value.as_i64() {
                return Some(num.to_string());
            }
            if let Some(num) = value.as_u64() {
                return Some(num.to_string());
            }
        }
    }
    None
}

fn pick_number(root: Option<&Value>, paths: &[&[&str]]) -> Option<f64> {
    let root = root?;
    for path in paths {
        if let Some(value) = get_path_value(root, path) {
            if let Some(num) = value.as_f64() {
                if num.is_finite() {
                    return Some(num);
                }
            }
            if let Some(text) = value.as_str() {
                if let Ok(num) = text.trim().parse::<f64>() {
                    if num.is_finite() {
                        return Some(num);
                    }
                }
            }
        }
    }
    None
}

fn parse_timestamp(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(seconds) = value.as_i64() {
        return normalize_timestamp(seconds);
    }
    if let Some(seconds) = value.as_u64() {
        return normalize_timestamp(seconds as i64);
    }
    if let Some(seconds) = value.as_f64() {
        if seconds.is_finite() {
            return normalize_timestamp(seconds.round() as i64);
        }
    }
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Ok(num) = trimmed.parse::<i64>() {
            return normalize_timestamp(num);
        }
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
            return Some(dt.timestamp());
        }
        if let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
            return Some(parsed.and_utc().timestamp());
        }
        if let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y/%m/%d %H:%M:%S") {
            return Some(parsed.and_utc().timestamp());
        }
    }
    None
}

fn normalize_timestamp(raw: i64) -> Option<i64> {
    if raw <= 0 {
        return None;
    }
    if raw > 10_000_000_000 {
        return Some(raw / 1000);
    }
    Some(raw)
}

fn resolve_usage_root<'a>(usage: Option<&'a Value>) -> Option<&'a Value> {
    let usage = usage?;
    if let Some(state) = get_path_value(usage, &["kiro.resourceNotifications.usageState"]) {
        return Some(state);
    }
    if let Some(state) = get_path_value(usage, &["usageState"]) {
        return Some(state);
    }
    Some(usage)
}

fn pick_usage_breakdown<'a>(usage: Option<&'a Value>) -> Option<&'a Value> {
    let usage = usage?;
    let list = get_path_value(usage, &["usageBreakdownList"])
        .and_then(|value| value.as_array())
        .or_else(|| {
            get_path_value(usage, &["usageBreakdowns"]).and_then(|value| value.as_array())
        })?;
    if list.is_empty() {
        return None;
    }

    list.iter()
        .find(|item| {
            item.as_object()
                .and_then(|obj| obj.get("type"))
                .and_then(|value| value.as_str())
                .map(|value| value.eq_ignore_ascii_case("credit"))
                .unwrap_or(false)
        })
        .or_else(|| list.first())
}

fn days_until(timestamp: Option<i64>) -> Option<i64> {
    let ts = timestamp?;
    let now = now_timestamp();
    if ts <= now {
        return Some(0);
    }
    Some(((ts - now) as f64 / 86_400.0).ceil() as i64)
}

fn parse_profile_arn_region(profile_arn: &str) -> Option<String> {
    let mut segments = profile_arn.split(':');
    let prefix = segments.next()?.trim();
    if !prefix.eq_ignore_ascii_case("arn") {
        return None;
    }
    let _partition = segments.next()?;
    let _service = segments.next()?;
    let region = segments.next()?.trim();
    if region.is_empty() {
        None
    } else {
        Some(region.to_string())
    }
}

fn runtime_endpoint_for_region(region: Option<&str>) -> String {
    let region = region.unwrap_or("us-east-1").trim().to_ascii_lowercase();
    match region.as_str() {
        "us-east-1" => "https://q.us-east-1.amazonaws.com".to_string(),
        "eu-central-1" => "https://q.eu-central-1.amazonaws.com".to_string(),
        "us-gov-east-1" => "https://q-fips.us-gov-east-1.amazonaws.com".to_string(),
        "us-gov-west-1" => "https://q-fips.us-gov-west-1.amazonaws.com".to_string(),
        "us-iso-east-1" => "https://q.us-iso-east-1.c2s.ic.gov".to_string(),
        "us-isob-east-1" => "https://q.us-isob-east-1.sc2s.sgov.gov".to_string(),
        "us-isof-south-1" => "https://q.us-isof-south-1.csp.hci.ic.gov".to_string(),
        "us-isof-east-1" => "https://q.us-isof-east-1.csp.hci.ic.gov".to_string(),
        _ => KIRO_RUNTIME_DEFAULT_ENDPOINT.to_string(),
    }
}

fn decode_query_component(value: &str) -> String {
    urlencoding::decode(value)
        .map(|v| v.into_owned())
        .unwrap_or_else(|_| value.to_string())
}

fn parse_query_params(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.trim();
            if key.is_empty() {
                return None;
            }
            let raw_value = parts.next().unwrap_or("");
            Some((key.to_string(), decode_query_component(raw_value)))
        })
        .collect()
}

fn auth_success_redirect_url() -> String {
    format!(
        "{}?auth_status=success&redirect_from=KiroIDE",
        KIRO_AUTH_PORTAL_URL
    )
}

fn auth_error_redirect_url(message: &str) -> String {
    format!(
        "{}?auth_status=error&redirect_from=KiroIDE&error_message={}",
        KIRO_AUTH_PORTAL_URL,
        urlencoding::encode(message)
    )
}

fn is_mwinit_tool_available() -> bool {
    #[cfg(target_os = "windows")]
    let checker = "where.exe";
    #[cfg(not(target_os = "windows"))]
    let checker = "which";

    std::process::Command::new(checker)
        .arg("mwinit")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn build_portal_auth_url(
    state_token: &str,
    code_challenge: &str,
    redirect_uri: &str,
    from_amazon_internal: bool,
    login_option: Option<&str>,
) -> String {
    let mut url = format!(
        "{}?state={}&code_challenge={}&code_challenge_method=S256&redirect_uri={}&redirect_from=KiroIDE",
        KIRO_AUTH_PORTAL_URL,
        urlencoding::encode(state_token),
        urlencoding::encode(code_challenge),
        urlencoding::encode(redirect_uri),
    );
    if from_amazon_internal {
        url.push_str("&from_amazon_internal=true");
    }
    if let Some(login_option) = login_option.and_then(|v| normalize_non_empty(Some(v))) {
        url.push_str("&login_option=");
        url.push_str(urlencoding::encode(login_option.as_str()).as_ref());
    }
    url
}

fn find_available_callback_port() -> Result<u16, String> {
    for port in CALLBACK_PORT_CANDIDATES {
        if let Ok(listener) = std::net::TcpListener::bind(("127.0.0.1", port)) {
            drop(listener);
            return Ok(port);
        }
    }
    Err("Local callback port is in use. Close the occupying process and retry.".to_string())
}

fn set_callback_result_for_login(
    expected_login_id: &str,
    expected_state: &str,
    result: Result<OAuthCallbackData, String>,
) {
    if let Ok(mut guard) = PENDING_OAUTH_STATE.lock() {
        if let Some(state) = guard.as_mut() {
            if state.login_id == expected_login_id && state.state_token == expected_state {
                state.callback_result = Some(result);
            }
        }
    }
}

fn extract_profile_arn_from_payload(payload: &KiroOAuthCompletePayload) -> Option<String> {
    extract_profile_arn(
        payload.kiro_auth_token_raw.as_ref(),
        payload.kiro_profile_raw.as_ref(),
    )
}

fn provider_from_login_option(login_option: &str) -> Option<String> {
    match login_option.trim().to_ascii_lowercase().as_str() {
        "google" => Some("Google".to_string()),
        "github" => Some("Github".to_string()),
        "builderid" | "awsidc" => Some("BuilderId".to_string()),
        "enterprise" | "iam_sso" | "iamsso" => Some("Enterprise".to_string()),
        _ => None,
    }
}

fn build_token_exchange_redirect_uri(
    base_callback_url: &str,
    callback: &OAuthCallbackData,
) -> String {
    let callback_path = if callback.path.starts_with('/') {
        callback.path.clone()
    } else {
        format!("/{}", callback.path)
    };
    format!(
        "{}{}?login_option={}",
        base_callback_url.trim_end_matches('/'),
        callback_path,
        urlencoding::encode(callback.login_option.as_str()),
    )
}

fn inject_callback_context_into_token(token: &mut Value, callback: &OAuthCallbackData) {
    if !token.is_object() {
        *token = json!({});
    }
    let Some(obj) = token.as_object_mut() else {
        return;
    };

    if !callback.login_option.trim().is_empty() {
        obj.entry("login_option".to_string())
            .or_insert_with(|| Value::String(callback.login_option.clone()));
    }

    if let Some(provider) = provider_from_login_option(&callback.login_option) {
        obj.entry("provider".to_string())
            .or_insert_with(|| Value::String(provider.clone()));
        obj.entry("loginProvider".to_string())
            .or_insert_with(|| Value::String(provider.clone()));
        obj.entry("authMethod".to_string())
            .or_insert_with(|| Value::String("social".to_string()));
    }

    if let Some(value) = callback.issuer_url.as_ref() {
        obj.entry("issuer_url".to_string())
            .or_insert_with(|| Value::String(value.clone()));
    }
    if let Some(value) = callback.idc_region.as_ref() {
        obj.entry("idc_region".to_string())
            .or_insert_with(|| Value::String(value.clone()));
    }
    if let Some(value) = callback.client_id.as_ref() {
        obj.entry("client_id".to_string())
            .or_insert_with(|| Value::String(value.clone()));
    }
    if let Some(value) = callback.scopes.as_ref() {
        obj.entry("scopes".to_string())
            .or_insert_with(|| Value::String(value.clone()));
    }
    if let Some(value) = callback.login_hint.as_ref() {
        obj.entry("login_hint".to_string())
            .or_insert_with(|| Value::String(value.clone()));
    }
    if let Some(value) = callback.audience.as_ref() {
        obj.entry("audience".to_string())
            .or_insert_with(|| Value::String(value.clone()));
    }

    // Auth service returns expiresIn; convert to expiresAt to match Kiro local cache shape.
    let has_expires_at = obj.contains_key("expiresAt") || obj.contains_key("expires_at");
    if !has_expires_at {
        let expires_in_seconds = obj
            .get("expiresIn")
            .and_then(|value| {
                value
                    .as_i64()
                    .or_else(|| value.as_u64().map(|n| n as i64))
                    .or_else(|| {
                        value
                            .as_str()
                            .and_then(|raw| raw.trim().parse::<i64>().ok())
                    })
            })
            .unwrap_or(0);
        if expires_in_seconds > 0 {
            let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in_seconds);
            obj.insert(
                "expiresAt".to_string(),
                Value::String(expires_at.to_rfc3339()),
            );
        }
    }
}

fn unwrap_token_response(mut response: Value) -> Value {
    if let Some(data) = response
        .as_object_mut()
        .and_then(|obj| obj.remove("data"))
        .filter(|value| value.is_object())
    {
        data
    } else {
        response
    }
}

async fn exchange_code_for_token(
    callback: &OAuthCallbackData,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<Value, String> {
    let code = callback
        .code
        .as_deref()
        .and_then(|value| normalize_non_empty(Some(value)))
        .ok_or_else(|| "Kiro callback missing code, cannot complete login".to_string())?;

    let response = reqwest::Client::new()
        .post(KIRO_TOKEN_ENDPOINT)
        .header("Content-Type", "application/json")
        .json(&json!({
            "code": code,
            "code_verifier": code_verifier,
            "redirect_uri": redirect_uri
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to request Kiro oauth/token: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "<no-body>".to_string());
    if !status.is_success() {
        return Err(format!(
            "Kiro oauth/token returned error: status={}, body={}",
            status, body
        ));
    }

    let mut token = unwrap_token_response(
        serde_json::from_str::<Value>(&body)
            .map_err(|e| format!("Failed to parse Kiro oauth/token response: {} (body={})", e, body))?,
    );
    inject_callback_context_into_token(&mut token, callback);
    Ok(token)
}

fn normalize_provider_option(value: Option<String>) -> String {
    value
        .and_then(|v| normalize_non_empty(Some(v.as_str())))
        .unwrap_or_else(|| "google".to_string())
        .to_ascii_lowercase()
}

fn normalize_region_option(value: Option<String>) -> String {
    value
        .and_then(|v| normalize_non_empty(Some(v.as_str())))
        .unwrap_or_else(|| "us-east-1".to_string())
        .to_ascii_lowercase()
}

fn oauth_scopes() -> Vec<&'static str> {
    vec![
        "codewhisperer:completions",
        "codewhisperer:analysis",
        "codewhisperer:conversations",
        "codewhisperer:transformations",
        "codewhisperer:taskassist",
    ]
}

async fn register_oidc_client(
    region: &str,
    grant_types: &[&str],
    issuer_url: &str,
    redirect_uris: Option<Vec<String>>,
) -> Result<(String, String), String> {
    let endpoint = format!("https://oidc.{}.amazonaws.com/client/register", region);
    let mut payload = json!({
        "clientName": "Mira Tools",
        "clientType": "public",
        "scopes": oauth_scopes(),
        "grantTypes": grant_types,
        "issuerUrl": issuer_url,
    });
    if let Some(redirect_uris) = redirect_uris {
        payload["redirectUris"] = serde_json::Value::Array(
            redirect_uris
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        );
    }

    let response = reqwest::Client::new()
        .post(endpoint)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("OIDC client register failed: {}", e))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "OIDC client register failed: status={}, body={}",
            status.as_u16(),
            body
        ));
    }
    let parsed = serde_json::from_str::<Value>(&body)
        .map_err(|e| format!("Failed to parse OIDC client register response: {}", e))?;
    let client_id = pick_string(Some(&parsed), &[&["clientId"]])
        .ok_or_else(|| "OIDC client register missing clientId".to_string())?;
    let client_secret = pick_string(Some(&parsed), &[&["clientSecret"]])
        .ok_or_else(|| "OIDC client register missing clientSecret".to_string())?;
    Ok((client_id, client_secret))
}

async fn start_oidc_device_authorization(
    region: &str,
    client_id: &str,
    client_secret: &str,
    start_url: &str,
) -> Result<(String, String, String, u64, u64), String> {
    let endpoint = format!("https://oidc.{}.amazonaws.com/device_authorization", region);
    let response = reqwest::Client::new()
        .post(endpoint)
        .header("Content-Type", "application/json")
        .json(&json!({
            "clientId": client_id,
            "clientSecret": client_secret,
            "startUrl": start_url
        }))
        .send()
        .await
        .map_err(|e| format!("OIDC device authorization failed: {}", e))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "OIDC device authorization failed: status={}, body={}",
            status.as_u16(),
            body
        ));
    }
    let parsed = serde_json::from_str::<Value>(&body)
        .map_err(|e| format!("Failed to parse OIDC device authorization response: {}", e))?;
    let device_code = pick_string(Some(&parsed), &[&["deviceCode"]])
        .ok_or_else(|| "OIDC response missing deviceCode".to_string())?;
    let user_code = pick_string(Some(&parsed), &[&["userCode"]]).unwrap_or_default();
    let verification_uri_complete =
        pick_string(Some(&parsed), &[&["verificationUriComplete"], &["verificationUri"]])
            .ok_or_else(|| "OIDC response missing verificationUri".to_string())?;
    let interval = pick_string(Some(&parsed), &[&["interval"]])
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(5);
    let expires_in = pick_string(Some(&parsed), &[&["expiresIn"]])
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(OAUTH_TIMEOUT_SECONDS);

    Ok((
        device_code,
        user_code,
        verification_uri_complete.clone(),
        interval,
        expires_in,
    ))
}

async fn exchange_oidc_device_token(
    region: &str,
    client_id: &str,
    client_secret: &str,
    device_code: &str,
) -> Result<Option<Value>, String> {
    let endpoint = format!("https://oidc.{}.amazonaws.com/token", region);
    let response = reqwest::Client::new()
        .post(endpoint)
        .header("Content-Type", "application/json")
        .json(&json!({
            "clientId": client_id,
            "clientSecret": client_secret,
            "grantType": "urn:ietf:params:oauth:grant-type:device_code",
            "deviceCode": device_code
        }))
        .send()
        .await
        .map_err(|e| format!("OIDC token exchange failed: {}", e))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if status.is_success() {
        let parsed = serde_json::from_str::<Value>(&body)
            .map_err(|e| format!("Failed to parse OIDC token response: {}", e))?;
        return Ok(Some(parsed));
    }
    let parsed = serde_json::from_str::<Value>(&body).unwrap_or_default();
    let error_name = pick_string(Some(&parsed), &[&["error"], &["code"]]).unwrap_or_default();
    if error_name.eq_ignore_ascii_case("authorization_pending") {
        return Ok(None);
    }
    if error_name.eq_ignore_ascii_case("slow_down") {
        return Ok(None);
    }
    Err(format!(
        "OIDC token exchange failed: status={}, body={}",
        status.as_u16(),
        body
    ))
}

async fn exchange_oidc_authorization_code(
    region: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
    code_verifier: &str,
) -> Result<Value, String> {
    let endpoint = format!("https://oidc.{}.amazonaws.com/token", region);
    let response = reqwest::Client::new()
        .post(endpoint)
        .header("Content-Type", "application/json")
        .json(&json!({
            "clientId": client_id,
            "clientSecret": client_secret,
            "grantType": "authorization_code",
            "redirectUri": redirect_uri,
            "code": code,
            "codeVerifier": code_verifier
        }))
        .send()
        .await
        .map_err(|e| format!("OIDC authorization_code exchange failed: {}", e))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "OIDC authorization_code exchange failed: status={}, body={}",
            status.as_u16(),
            body
        ));
    }
    serde_json::from_str::<Value>(&body)
        .map_err(|e| format!("Failed to parse OIDC authorization_code response: {}", e))
}

async fn start_callback_server(
    callback_port: u16,
    expected_login_id: String,
    expected_state: String,
) -> Result<(), String> {
    use tiny_http::{Header, Response, Server};

    let server = Server::http(format!("127.0.0.1:{}", callback_port))
        .map_err(|e| format!("Failed to start Kiro OAuth callback service: {}", e))?;
    let started = std::time::Instant::now();

    logger::log_info(&format!(
        "[Kiro OAuth] Local callback service started: login_id={}, port={}",
        expected_login_id, callback_port
    ));

    loop {
        let should_stop = {
            let guard = PENDING_OAUTH_STATE
                .lock()
                .map_err(|_| "OAuth state lock unavailable".to_string())?;
            match guard.as_ref() {
                Some(state) => {
                    state.login_id != expected_login_id || state.state_token != expected_state
                }
                None => true,
            }
        };
        if should_stop {
            break;
        }

        if started.elapsed().as_secs() > OAUTH_TIMEOUT_SECONDS {
            set_callback_result_for_login(
                &expected_login_id,
                &expected_state,
                Err("Timed out waiting for Kiro login, please start authorization again".to_string()),
            );
            break;
        }

        if let Ok(Some(request)) = server.try_recv() {
            let raw_url = request.url().to_string();
            let (path, query) = match raw_url.split_once('?') {
                Some((path, query)) => (path, query),
                None => (raw_url.as_str(), ""),
            };

            if path == "/cancel" {
                set_callback_result_for_login(
                    &expected_login_id,
                    &expected_state,
                    Err("Login canceled".to_string()),
                );
                let _ = request.respond(Response::from_string("cancelled").with_status_code(200));
                break;
            }

            if path != "/oauth/callback" && path != "/signin/callback" {
                let _ = request.respond(Response::from_string("Not Found").with_status_code(404));
                continue;
            }

            let params = parse_query_params(query);
            let error_code = params.get("error").cloned();
            let error_description = params
                .get("error_description")
                .cloned()
                .unwrap_or_else(String::new);
            if let Some(error_code) = error_code {
                let message = if error_description.trim().is_empty() {
                    format!("Authorization failed: {}", error_code)
                } else {
                    format!("Authorization failed: {} ({})", error_code, error_description)
                };
                set_callback_result_for_login(
                    &expected_login_id,
                    &expected_state,
                    Err(message.clone()),
                );
                let redirect = auth_error_redirect_url(&message);
                let response = Header::from_bytes(&b"Location"[..], redirect.as_bytes())
                    .ok()
                    .map(|header| Response::empty(302).with_header(header))
                    .unwrap_or_else(|| Response::empty(400));
                let _ = request.respond(response);
                break;
            }

            let callback_state = params.get("state").cloned().unwrap_or_default();
            if callback_state.is_empty() || callback_state != expected_state {
                let message = "Authorization state validation failed, please start login again".to_string();
                set_callback_result_for_login(
                    &expected_login_id,
                    &expected_state,
                    Err(message.clone()),
                );
                let redirect = auth_error_redirect_url(&message);
                let response = Header::from_bytes(&b"Location"[..], redirect.as_bytes())
                    .ok()
                    .map(|header| Response::empty(302).with_header(header))
                    .unwrap_or_else(|| Response::empty(400));
                let _ = request.respond(response);
                break;
            }

            let login_option = params
                .get("login_option")
                .or_else(|| params.get("loginOption"))
                .and_then(|value| normalize_non_empty(Some(value.as_str())))
                .unwrap_or_default()
                .to_ascii_lowercase();

            let callback = OAuthCallbackData {
                login_option,
                code: params
                    .get("code")
                    .and_then(|value| normalize_non_empty(Some(value.as_str()))),
                issuer_url: params
                    .get("issuer_url")
                    .or_else(|| params.get("issuerUrl"))
                    .and_then(|value| normalize_non_empty(Some(value.as_str()))),
                idc_region: params
                    .get("idc_region")
                    .or_else(|| params.get("idcRegion"))
                    .and_then(|value| normalize_non_empty(Some(value.as_str()))),
                path: path.to_string(),
                client_id: params
                    .get("client_id")
                    .or_else(|| params.get("clientId"))
                    .and_then(|value| normalize_non_empty(Some(value.as_str()))),
                scopes: params
                    .get("scopes")
                    .or_else(|| params.get("scope"))
                    .and_then(|value| normalize_non_empty(Some(value.as_str()))),
                login_hint: params
                    .get("login_hint")
                    .or_else(|| params.get("loginHint"))
                    .and_then(|value| normalize_non_empty(Some(value.as_str()))),
                audience: params
                    .get("audience")
                    .and_then(|value| normalize_non_empty(Some(value.as_str()))),
            };

            logger::log_info(&format!(
                "[Kiro OAuth] Callback received: login_id={}, path={}, login_option={}, has_code={}",
                expected_login_id,
                callback.path,
                callback.login_option,
                callback
                    .code
                    .as_ref()
                    .map(|v| !v.is_empty())
                    .unwrap_or(false)
            ));

            set_callback_result_for_login(&expected_login_id, &expected_state, Ok(callback));
            let redirect = auth_success_redirect_url();
            let response = Header::from_bytes(&b"Location"[..], redirect.as_bytes())
                .ok()
                .map(|header| Response::empty(302).with_header(header))
                .unwrap_or_else(|| Response::empty(200));
            let _ = request.respond(response);
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(120)).await;
    }

    Ok(())
}

fn decode_jwt_claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .ok()?;
    serde_json::from_slice::<Value>(&decoded).ok()
}

fn extract_usage_payload(
    usage: Option<&Value>,
) -> (
    Option<String>,
    Option<String>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<i64>,
    Option<i64>,
) {
    let usage = resolve_usage_root(usage);

    let plan_name = pick_string(
        usage,
        &[
            &["planName"],
            &["currentPlanName"],
            &["subscriptionInfo", "subscriptionName"],
            &["subscriptionInfo", "subscriptionTitle"],
            &["usageBreakdowns", "planName"],
            &["freeTrialUsage", "planName"],
            &["plan", "name"],
        ],
    );

    let plan_tier = pick_string(
        usage,
        &[
            &["planTier"],
            &["tier"],
            &["subscriptionInfo", "type"],
            &["usageBreakdowns", "tier"],
            &["plan", "tier"],
        ],
    );

    let mut credits_total = pick_number(
        usage,
        &[
            &["estimatedUsage", "total"],
            &["estimatedUsage", "creditsTotal"],
            &["usageBreakdowns", "plan", "totalCredits"],
            &["usageBreakdowns", "covered", "total"],
            &["usageBreakdownList", "0", "usageLimitWithPrecision"],
            &["usageBreakdownList", "0", "usageLimit"],
            &["credits", "total"],
            &["totalCredits"],
        ],
    );

    let mut credits_used = pick_number(
        usage,
        &[
            &["estimatedUsage", "used"],
            &["estimatedUsage", "creditsUsed"],
            &["usageBreakdowns", "plan", "usedCredits"],
            &["usageBreakdowns", "covered", "used"],
            &["usageBreakdownList", "0", "currentUsageWithPrecision"],
            &["usageBreakdownList", "0", "currentUsage"],
            &["credits", "used"],
            &["usedCredits"],
        ],
    );

    let mut bonus_total = pick_number(
        usage,
        &[
            &["bonusCredits", "total"],
            &["bonus", "total"],
            &["usageBreakdowns", "bonus", "total"],
            &[
                "usageBreakdownList",
                "0",
                "freeTrialInfo",
                "usageLimitWithPrecision",
            ],
            &["usageBreakdownList", "0", "freeTrialInfo", "usageLimit"],
        ],
    );

    let mut bonus_used = pick_number(
        usage,
        &[
            &["bonusCredits", "used"],
            &["bonus", "used"],
            &["usageBreakdowns", "bonus", "used"],
            &[
                "usageBreakdownList",
                "0",
                "freeTrialInfo",
                "currentUsageWithPrecision",
            ],
            &["usageBreakdownList", "0", "freeTrialInfo", "currentUsage"],
        ],
    );

    let mut usage_reset_at = parse_timestamp(
        usage
            .and_then(|value| get_path_value(value, &["resetAt"]))
            .or_else(|| usage.and_then(|value| get_path_value(value, &["resetTime"])))
            .or_else(|| usage.and_then(|value| get_path_value(value, &["resetOn"])))
            .or_else(|| usage.and_then(|value| get_path_value(value, &["nextDateReset"])))
            .or_else(|| {
                usage.and_then(|value| get_path_value(value, &["usageBreakdowns", "resetAt"]))
            }),
    );

    let mut bonus_expire_days = pick_number(
        usage,
        &[
            &["bonusCredits", "expiryDays"],
            &["bonusCredits", "expireDays"],
            &["bonus", "expiryDays"],
            &["usageBreakdownList", "0", "freeTrialInfo", "daysRemaining"],
        ],
    )
    .map(|value| value.round() as i64);

    let breakdown = pick_usage_breakdown(usage);
    let free_trial = breakdown.and_then(|value| {
        get_path_value(value, &["freeTrialUsage"])
            .or_else(|| get_path_value(value, &["freeTrialInfo"]))
    });

    let plan_name = plan_name.or_else(|| {
        pick_string(
            breakdown,
            &[
                &["displayName"],
                &["displayNamePlural"],
                &["type"],
                &["unit"],
            ],
        )
    });

    let plan_tier =
        plan_tier.or_else(|| pick_string(breakdown, &[&["currency"], &["type"], &["unit"]]));

    if credits_total.is_none() {
        credits_total = pick_number(
            breakdown,
            &[
                &["usageLimitWithPrecision"],
                &["usageLimit"],
                &["limit"],
                &["total"],
                &["totalCredits"],
            ],
        );
    }
    if credits_used.is_none() {
        credits_used = pick_number(
            breakdown,
            &[
                &["currentUsageWithPrecision"],
                &["currentUsage"],
                &["used"],
                &["usedCredits"],
            ],
        );
    }

    if bonus_total.is_none() {
        bonus_total = pick_number(
            free_trial,
            &[
                &["usageLimitWithPrecision"],
                &["usageLimit"],
                &["limit"],
                &["total"],
                &["totalCredits"],
            ],
        );
    }
    if bonus_used.is_none() {
        bonus_used = pick_number(
            free_trial,
            &[
                &["currentUsageWithPrecision"],
                &["currentUsage"],
                &["used"],
                &["usedCredits"],
            ],
        );
    }

    if usage_reset_at.is_none() {
        usage_reset_at = parse_timestamp(
            breakdown
                .and_then(|value| get_path_value(value, &["resetDate"]))
                .or_else(|| breakdown.and_then(|value| get_path_value(value, &["resetAt"]))),
        );
    }

    if bonus_expire_days.is_none() {
        bonus_expire_days = pick_number(
            free_trial,
            &[&["daysRemaining"], &["expiryDays"], &["expireDays"]],
        )
        .map(|value| value.round() as i64)
        .or_else(|| {
            days_until(parse_timestamp(
                free_trial.and_then(|value| get_path_value(value, &["expiryDate"])),
            ))
        })
        .or_else(|| {
            days_until(parse_timestamp(
                free_trial.and_then(|value| get_path_value(value, &["freeTrialExpiry"])),
            ))
        });
    }

    (
        plan_name,
        plan_tier,
        credits_total,
        credits_used,
        bonus_total,
        bonus_used,
        usage_reset_at,
        bonus_expire_days,
    )
}

fn extract_profile_arn(auth_token: Option<&Value>, profile: Option<&Value>) -> Option<String> {
    pick_string(
        profile,
        &[
            &["arn"],
            &["profileArn"],
            &["profile", "arn"],
            &["account", "arn"],
        ],
    )
    .or_else(|| pick_string(auth_token, &[&["profileArn"], &["profile_arn"], &["arn"]]))
}

fn extract_profile_name(auth_token: Option<&Value>, profile: Option<&Value>) -> Option<String> {
    pick_string(
        profile,
        &[
            &["name"],
            &["profileName"],
            &["provider"],
            &["loginProvider"],
        ],
    )
    .or_else(|| pick_string(auth_token, &[&["provider"], &["loginProvider"]]))
}

pub(crate) fn build_payload_from_snapshot(
    auth_token: Value,
    profile: Option<Value>,
    usage: Option<Value>,
) -> Result<KiroOAuthCompletePayload, String> {
    let access_token = pick_string(
        Some(&auth_token),
        &[
            &["accessToken"],
            &["access_token"],
            &["token"],
            &["idToken"],
            &["id_token"],
            &["accessTokenJwt"],
        ],
    )
    .ok_or_else(|| "Kiro local auth info missing access token".to_string())?;

    let refresh_token = pick_string(
        Some(&auth_token),
        &[&["refreshToken"], &["refresh_token"], &["refreshTokenJwt"]],
    );
    let token_type = pick_string(
        Some(&auth_token),
        &[&["tokenType"], &["token_type"], &["authType"]],
    )
    .or_else(|| Some("Bearer".to_string()));

    let expires_at = parse_timestamp(
        get_path_value(&auth_token, &["expiresAt"])
            .or_else(|| get_path_value(&auth_token, &["expires_at"]))
            .or_else(|| get_path_value(&auth_token, &["expiry"]))
            .or_else(|| get_path_value(&auth_token, &["expiration"])),
    );

    let profile_arn = extract_profile_arn(Some(&auth_token), profile.as_ref());
    let profile_name = extract_profile_name(Some(&auth_token), profile.as_ref());

    let id_token_claims = pick_string(
        Some(&auth_token),
        &[
            &["idToken"],
            &["id_token"],
            &["idTokenJwt"],
            &["id_token_jwt"],
        ],
    )
    .and_then(|raw| decode_jwt_claims(&raw));
    let access_token_claims = pick_string(
        Some(&auth_token),
        &[
            &["accessToken"],
            &["access_token"],
            &["token"],
            &["accessTokenJwt"],
        ],
    )
    .and_then(|raw| decode_jwt_claims(&raw));

    let email = normalize_email(pick_string(
        profile.as_ref(),
        &[
            &["email"],
            &["user", "email"],
            &["account", "email"],
            &["primaryEmail"],
        ],
    ))
    .or_else(|| {
        normalize_email(pick_string(
            Some(&auth_token),
            &[&["email"], &["userEmail"]],
        ))
    })
    .or_else(|| {
        normalize_email(pick_string(
            id_token_claims.as_ref(),
            &[&["email"], &["upn"], &["preferred_username"]],
        ))
    })
    .or_else(|| {
        normalize_email(pick_string(
            access_token_claims.as_ref(),
            &[&["email"], &["upn"], &["preferred_username"]],
        ))
    })
    .or_else(|| {
        normalize_email(pick_string(
            Some(&auth_token),
            &[&["login_hint"], &["loginHint"]],
        ))
    })
    .unwrap_or_default();

    let user_id = pick_string(
        profile.as_ref(),
        &[
            &["userId"],
            &["user_id"],
            &["id"],
            &["sub"],
            &["account", "id"],
        ],
    )
    .or_else(|| {
        pick_string(
            Some(&auth_token),
            &[&["userId"], &["user_id"], &["sub"], &["accountId"]],
        )
    })
    .or_else(|| {
        pick_string(
            id_token_claims.as_ref(),
            &[&["sub"], &["user_id"], &["uid"]],
        )
    })
    .or_else(|| {
        pick_string(
            access_token_claims.as_ref(),
            &[&["sub"], &["user_id"], &["uid"]],
        )
    })
    .or_else(|| profile_arn.clone());

    let login_provider = pick_string(
        profile.as_ref(),
        &[
            &["loginProvider"],
            &["provider"],
            &["authProvider"],
            &["signedInWith"],
        ],
    )
    .or_else(|| {
        pick_string(
            Some(&auth_token),
            &[&["login_option"], &["provider"], &["loginProvider"]],
        )
    })
    .or_else(|| profile_name.clone())
    .map(|raw| provider_from_login_option(&raw).unwrap_or(raw));

    let idc_region = pick_string(
        Some(&auth_token),
        &[&["idc_region"], &["idcRegion"], &["region"]],
    );
    let issuer_url = pick_string(
        Some(&auth_token),
        &[&["issuer_url"], &["issuerUrl"], &["issuer"]],
    );
    let client_id = pick_string(Some(&auth_token), &[&["client_id"], &["clientId"]]);
    let scopes = pick_string(Some(&auth_token), &[&["scopes"], &["scope"]]);
    let login_hint = pick_string(Some(&auth_token), &[&["login_hint"], &["loginHint"]])
        .or_else(|| normalize_non_empty(Some(email.as_str())));

    let mut normalized_profile = profile.unwrap_or_else(|| json!({}));
    if !normalized_profile.is_object() {
        normalized_profile = json!({});
    }
    if let Some(obj) = normalized_profile.as_object_mut() {
        if let Some(arn) = profile_arn.clone() {
            obj.entry("arn".to_string())
                .or_insert_with(|| Value::String(arn));
        }
        if let Some(name) = profile_name.clone().or_else(|| login_provider.clone()) {
            obj.entry("name".to_string())
                .or_insert_with(|| Value::String(name));
        }
    }

    let normalized_profile = if normalized_profile
        .as_object()
        .map(|obj| !obj.is_empty())
        .unwrap_or(false)
    {
        Some(normalized_profile)
    } else {
        None
    };

    let (
        plan_name,
        plan_tier,
        credits_total,
        credits_used,
        bonus_total,
        bonus_used,
        usage_reset_at,
        bonus_expire_days,
    ) = extract_usage_payload(usage.as_ref());

    Ok(KiroOAuthCompletePayload {
        email,
        user_id,
        login_provider,
        access_token,
        refresh_token,
        token_type,
        expires_at,
        idc_region,
        issuer_url,
        client_id,
        scopes,
        login_hint,
        plan_name,
        plan_tier,
        credits_total,
        credits_used,
        bonus_total,
        bonus_used,
        usage_reset_at,
        bonus_expire_days,
        kiro_auth_token_raw: Some(auth_token),
        kiro_profile_raw: normalized_profile,
        kiro_usage_raw: usage,
        status: None,
        status_reason: None,
    })
}

pub fn payload_from_account(account: &KiroAccount) -> KiroOAuthCompletePayload {
    KiroOAuthCompletePayload {
        email: account.email.clone(),
        user_id: account.user_id.clone(),
        login_provider: account.login_provider.clone(),
        access_token: account.access_token.clone(),
        refresh_token: account.refresh_token.clone(),
        token_type: account.token_type.clone(),
        expires_at: account.expires_at,
        idc_region: account.idc_region.clone(),
        issuer_url: account.issuer_url.clone(),
        client_id: account.client_id.clone(),
        scopes: account.scopes.clone(),
        login_hint: account.login_hint.clone(),
        plan_name: account.plan_name.clone(),
        plan_tier: account.plan_tier.clone(),
        credits_total: account.credits_total,
        credits_used: account.credits_used,
        bonus_total: account.bonus_total,
        bonus_used: account.bonus_used,
        usage_reset_at: account.usage_reset_at,
        bonus_expire_days: account.bonus_expire_days,
        kiro_auth_token_raw: account.kiro_auth_token_raw.clone(),
        kiro_profile_raw: account.kiro_profile_raw.clone(),
        kiro_usage_raw: account.kiro_usage_raw.clone(),
        status: account.status.clone(),
        status_reason: account.status_reason.clone(),
    }
}

pub fn build_payload_from_local_files() -> Result<KiroOAuthCompletePayload, String> {
    let auth_token = kiro_account::read_local_auth_token_json()?.ok_or_else(|| {
        "Kiro login info not found on this machine (~/.aws/sso/cache/kiro-auth-token.json)".to_string()
    })?;
    let profile = kiro_account::read_local_profile_json()?;
    let usage = kiro_account::read_local_usage_snapshot()?;
    build_payload_from_snapshot(auth_token, profile, usage)
}

async fn fetch_usage_limits_via_runtime(
    access_token: &str,
    profile_arn: Option<&str>,
    idc_region: Option<&str>,
    is_email_required: bool,
) -> Result<Value, String> {
    let region = profile_arn
        .and_then(parse_profile_arn_region)
        .or_else(|| idc_region.and_then(|v| normalize_non_empty(Some(v))))
        .unwrap_or_else(|| "us-east-1".to_string());
    let endpoint = runtime_endpoint_for_region(Some(region.as_str()));
    let mut url = format!(
        "{}/getUsageLimits?origin=AI_EDITOR&resourceType=AGENTIC_REQUEST",
        endpoint.trim_end_matches('/'),
    );
    if let Some(profile_arn) = profile_arn.and_then(|v| normalize_non_empty(Some(v))) {
        url.push_str("&profileArn=");
        url.push_str(urlencoding::encode(profile_arn.as_str()).as_ref());
    }
    if is_email_required {
        url.push_str("&isEmailRequired=true");
    }

    let response = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token.trim()))
        .send()
        .await
        .map_err(|e| format!("Failed to request Kiro runtime usage: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "<no-body>".to_string());

    if !status.is_success() {
        let reason = parse_runtime_error_reason(&body)
            .or_else(|| (status == reqwest::StatusCode::FORBIDDEN).then(|| body.clone()));
        if let Some(reason) = reason {
            return Err(format!("BANNED:{}", reason));
        }
        return Err(format!(
            "Kiro runtime usage returned error: status={}, body={}",
            status, body
        ));
    }

    serde_json::from_str::<Value>(&body)
        .map_err(|e| format!("Failed to parse Kiro runtime usage response: {}", e))
}

fn apply_runtime_usage_to_payload(payload: &mut KiroOAuthCompletePayload, usage: Value) {
    if let Some(email) = normalize_email(pick_string(
        Some(&usage),
        &[&["userInfo", "email"], &["email"]],
    )) {
        payload.email = email.clone();
        if payload
            .login_hint
            .as_deref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .is_none()
        {
            payload.login_hint = Some(email);
        }
    }

    if let Some(user_id) = pick_string(
        Some(&usage),
        &[&["userInfo", "userId"], &["userId"], &["user_id"], &["sub"]],
    )
    .and_then(|value| normalize_non_empty(Some(value.as_str())))
    {
        payload.user_id = Some(user_id);
    }

    if let Some(provider) = pick_string(
        Some(&usage),
        &[
            &["userInfo", "provider", "label"],
            &["userInfo", "provider", "name"],
            &["userInfo", "provider", "id"],
            &["userInfo", "providerId"],
            &["provider", "label"],
            &["provider", "name"],
            &["provider", "id"],
        ],
    )
    .and_then(|value| normalize_non_empty(Some(value.as_str())))
    {
        payload.login_provider = Some(provider_from_login_option(&provider).unwrap_or(provider));
    }

    let (
        plan_name,
        plan_tier,
        credits_total,
        credits_used,
        bonus_total,
        bonus_used,
        usage_reset_at,
        bonus_expire_days,
    ) = extract_usage_payload(Some(&usage));

    if let Some(value) = plan_name {
        payload.plan_name = Some(value);
    }
    if let Some(value) = plan_tier {
        payload.plan_tier = Some(value);
    }
    if let Some(value) = credits_total {
        payload.credits_total = Some(value);
    }
    if let Some(value) = credits_used {
        payload.credits_used = Some(value);
    }
    if let Some(value) = bonus_total {
        payload.bonus_total = Some(value);
    }
    if let Some(value) = bonus_used {
        payload.bonus_used = Some(value);
    }
    if let Some(value) = usage_reset_at {
        payload.usage_reset_at = Some(value);
    }
    if let Some(value) = bonus_expire_days {
        payload.bonus_expire_days = Some(value);
    }

    payload.kiro_usage_raw = Some(usage);
}

pub async fn enrich_payload_with_runtime_usage(
    mut payload: KiroOAuthCompletePayload,
) -> KiroOAuthCompletePayload {
    let profile_arn = extract_profile_arn_from_payload(&payload);

    let first_try = fetch_usage_limits_via_runtime(
        payload.access_token.as_str(),
        profile_arn.as_deref(),
        payload.idc_region.as_deref(),
        true,
    )
    .await;

    match first_try {
        Ok(usage) => {
            apply_runtime_usage_to_payload(&mut payload, usage);
            set_payload_status(&mut payload, KIRO_ACCOUNT_STATUS_NORMAL, None);
            return payload;
        }
        Err(err) => {
            if let Some(reason) = parse_banned_reason_from_error(&err) {
                set_payload_status(&mut payload, KIRO_ACCOUNT_STATUS_BANNED, Some(reason));
                return payload;
            }
            set_payload_status(&mut payload, KIRO_ACCOUNT_STATUS_ERROR, Some(err.clone()));
            logger::log_warn(&format!(
                "[Kiro Refresh] Runtime usage request failed (system-managed refresh only): {}",
                err
            ));
        }
    }

    payload
}

pub async fn refresh_payload_for_account(
    account: &KiroAccount,
) -> Result<KiroOAuthCompletePayload, String> {
    // 不执行远程 refreshToken：交给系统/Kiro 客户端维护 token。
    Ok(enrich_payload_with_runtime_usage(payload_from_account(account)).await)
}

pub async fn start_login(options: Option<KiroOAuthStartOptions>) -> Result<KiroOAuthStartResponse, String> {
    if let Ok(mut guard) = PENDING_OAUTH_STATE.lock() {
        if let Some(state) = guard.as_ref() {
            if state.expires_at > now_timestamp() && state.callback_result.is_none() {
                return Ok(KiroOAuthStartResponse {
                    login_id: state.login_id.clone(),
                    user_code: String::new(),
                    verification_uri: state.verification_uri.clone(),
                    verification_uri_complete: Some(state.verification_uri_complete.clone()),
                    expires_in: (state.expires_at - now_timestamp()).max(0) as u64,
                    interval_seconds: state.interval_seconds.max(1),
                    callback_url: normalize_non_empty(Some(state.callback_url.as_str())),
                });
            }
            *guard = None;
        }
    }

    let options = options.unwrap_or_default();
    let provider = normalize_provider_option(options.provider);
    let region = normalize_region_option(options.region);
    let selected_start_url = options
        .start_url
        .and_then(|value| normalize_non_empty(Some(value.as_str())));

    if matches!(
        provider.as_str(),
        "builderid" | "awsidc" | "internal"
    ) {
        let start_url = BUILDER_ID_START_URL.to_string();
        let (client_id, client_secret) = register_oidc_client(
            &region,
            &["urn:ietf:params:oauth:grant-type:device_code", "refresh_token"],
            &start_url,
            None,
        )
        .await?;
        let (device_code, user_code, verification_uri_complete, interval_seconds, expires_in) =
            start_oidc_device_authorization(
                &region,
                &client_id,
                &client_secret,
                &start_url,
            )
            .await?;

        let pending = PendingOAuthState {
            flow: OAuthFlow::BuilderId,
            login_id: generate_token(),
            expires_at: now_timestamp() + expires_in as i64,
            verification_uri: verification_uri_complete.clone(),
            verification_uri_complete: verification_uri_complete.clone(),
            callback_url: String::new(),
            callback_port: 0,
            state_token: String::new(),
            code_verifier: String::new(),
            region: Some(region),
            client_id: Some(client_id),
            client_secret: Some(client_secret),
            device_code: Some(device_code),
            interval_seconds,
            callback_result: None,
        };

        if let Ok(mut guard) = PENDING_OAUTH_STATE.lock() {
            *guard = Some(pending.clone());
        }

        return Ok(KiroOAuthStartResponse {
            login_id: pending.login_id,
            user_code,
            verification_uri: pending.verification_uri.clone(),
            verification_uri_complete: Some(pending.verification_uri_complete),
            expires_in,
            interval_seconds: interval_seconds.max(1),
            callback_url: None,
        });
    }

    if matches!(
        provider.as_str(),
        "enterprise" | "iam_sso" | "iamsso"
    ) {
        let start_url = selected_start_url.unwrap_or_else(|| BUILDER_ID_START_URL.to_string());
        if !start_url.to_ascii_lowercase().starts_with("https://") {
            return Err("SSO Start URL must start with https://".to_string());
        }

        let callback_port = find_available_callback_port()?;
        let callback_url = format!("http://localhost:{}", callback_port);
        let state_token = generate_token();
        let code_verifier = generate_token();
        let code_challenge = generate_code_challenge(&code_verifier);

        let redirect_uri = format!("{}/oauth/callback", callback_url.trim_end_matches('/'));
        let (client_id, client_secret) = register_oidc_client(
            &region,
            &["authorization_code", "refresh_token"],
            &start_url,
            Some(vec![redirect_uri.clone()]),
        )
        .await?;

        let authorize_query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("response_type", "code")
            .append_pair("client_id", client_id.as_str())
            .append_pair("redirect_uri", redirect_uri.as_str())
            .append_pair("scopes", oauth_scopes().join(",").as_str())
            .append_pair("state", state_token.as_str())
            .append_pair("code_challenge", code_challenge.as_str())
            .append_pair("code_challenge_method", "S256")
            .finish();
        let verification_uri_complete =
            format!("https://oidc.{}.amazonaws.com/authorize?{}", region, authorize_query);

        let pending = PendingOAuthState {
            flow: OAuthFlow::IamSso,
            login_id: generate_token(),
            expires_at: now_timestamp() + OAUTH_TIMEOUT_SECONDS as i64,
            verification_uri: verification_uri_complete.clone(),
            verification_uri_complete: verification_uri_complete.clone(),
            callback_url: callback_url.clone(),
            callback_port,
            state_token: state_token.clone(),
            code_verifier,
            region: Some(region),
            client_id: Some(client_id),
            client_secret: Some(client_secret),
            device_code: None,
            interval_seconds: 1,
            callback_result: None,
        };

        if let Ok(mut guard) = PENDING_OAUTH_STATE.lock() {
            *guard = Some(pending.clone());
        }

        let expected_login_id = pending.login_id.clone();
        let expected_state = pending.state_token.clone();
        let callback_port = pending.callback_port;
        tokio::spawn(async move {
            if let Err(err) = start_callback_server(
                callback_port,
                expected_login_id.clone(),
                expected_state.clone(),
            )
            .await
            {
                logger::log_error(&format!(
                    "[Kiro OAuth][IAM SSO] callback service error: login_id={}, error={}",
                    expected_login_id, err
                ));
                set_callback_result_for_login(
                    &expected_login_id,
                    &expected_state,
                    Err(format!("Local callback service error: {}", err)),
                );
            }
        });

        return Ok(KiroOAuthStartResponse {
            login_id: pending.login_id,
            user_code: String::new(),
            verification_uri: pending.verification_uri.clone(),
            verification_uri_complete: Some(pending.verification_uri_complete),
            expires_in: OAUTH_TIMEOUT_SECONDS,
            interval_seconds: 1,
            callback_url: Some(callback_url),
        });
    }

    let callback_port = find_available_callback_port()?;
    let callback_url = format!("http://localhost:{}", callback_port);
    let state_token = generate_token();
    let code_verifier = generate_token();
    let code_challenge = generate_code_challenge(&code_verifier);
    let portal_login_option = match provider.as_str() {
        "github" => Some("github"),
        "google" => Some("google"),
        _ => None,
    };
    let verification_uri_complete = build_portal_auth_url(
        &state_token,
        &code_challenge,
        &callback_url,
        is_mwinit_tool_available(),
        portal_login_option,
    );

    let pending = PendingOAuthState {
        flow: OAuthFlow::Portal,
        login_id: generate_token(),
        expires_at: now_timestamp() + OAUTH_TIMEOUT_SECONDS as i64,
        verification_uri: KIRO_AUTH_PORTAL_URL.to_string(),
        verification_uri_complete,
        callback_url: callback_url.clone(),
        callback_port,
        state_token: state_token.clone(),
        code_verifier,
        region: None,
        client_id: None,
        client_secret: None,
        device_code: None,
        interval_seconds: 1,
        callback_result: None,
    };

    if let Ok(mut guard) = PENDING_OAUTH_STATE.lock() {
        *guard = Some(pending.clone());
    }

    let expected_login_id = pending.login_id.clone();
    let expected_state = state_token.clone();
    let callback_port = pending.callback_port;
    tokio::spawn(async move {
        if let Err(err) = start_callback_server(
            callback_port,
            expected_login_id.clone(),
            expected_state.clone(),
        )
        .await
        {
            logger::log_error(&format!(
                "[Kiro OAuth] Local callback service error: login_id={}, error={}",
                expected_login_id, err
            ));
            set_callback_result_for_login(
                &expected_login_id,
                &expected_state,
                Err(format!("Local callback service error: {}", err)),
            );
        }
    });

    logger::log_info(&format!(
        "[Kiro OAuth] Login session created: login_id={}, callback_url={}, expires_in={}s",
        pending.login_id, pending.callback_url, OAUTH_TIMEOUT_SECONDS
    ));

    Ok(KiroOAuthStartResponse {
        login_id: pending.login_id,
        user_code: String::new(),
        verification_uri: pending.verification_uri.clone(),
        verification_uri_complete: Some(pending.verification_uri_complete),
        expires_in: OAUTH_TIMEOUT_SECONDS,
        interval_seconds: 1,
        callback_url: Some(callback_url),
    })
}

pub async fn complete_login(login_id: &str) -> Result<KiroOAuthCompletePayload, String> {
    loop {
        let state = {
            let guard = PENDING_OAUTH_STATE
                .lock()
                .map_err(|_| "OAuth state lock unavailable".to_string())?;
            guard.clone()
        };

        let state = state.ok_or_else(|| "Login flow was canceled, please start authorization again".to_string())?;
        if state.login_id != login_id {
            return Err("Login session has changed, please refresh and retry".to_string());
        }
        if state.expires_at <= now_timestamp() {
            let _ = cancel_login(Some(login_id));
            return Err("Timed out waiting for Kiro login, please start authorization again".to_string());
        }

        if state.flow == OAuthFlow::BuilderId {
            let region = state
                .region
                .clone()
                .unwrap_or_else(|| "us-east-1".to_string());
            let client_id = state
                .client_id
                .clone()
                .ok_or_else(|| "Missing OIDC clientId".to_string())?;
            let client_secret = state
                .client_secret
                .clone()
                .ok_or_else(|| "Missing OIDC clientSecret".to_string())?;
            let device_code = state
                .device_code
                .clone()
                .ok_or_else(|| "Missing OIDC deviceCode".to_string())?;

            if let Some(mut token) =
                exchange_oidc_device_token(&region, &client_id, &client_secret, &device_code).await?
            {
                if !token.is_object() {
                    token = json!({});
                }
                if let Some(obj) = token.as_object_mut() {
                    obj.entry("provider".to_string())
                        .or_insert_with(|| Value::String("BuilderId".to_string()));
                    obj.entry("loginProvider".to_string())
                        .or_insert_with(|| Value::String("BuilderId".to_string()));
                    obj.entry("authMethod".to_string())
                        .or_insert_with(|| Value::String("IdC".to_string()));
                    obj.entry("login_option".to_string())
                        .or_insert_with(|| Value::String("builderid".to_string()));
                    obj.entry("idc_region".to_string())
                        .or_insert_with(|| Value::String(region.clone()));
                    obj.entry("clientId".to_string())
                        .or_insert_with(|| Value::String(client_id.clone()));
                }
                let _ = cancel_login(Some(login_id));
                let payload = build_payload_from_snapshot(token, None, None)?;
                return Ok(enrich_payload_with_runtime_usage(payload).await);
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(
                state.interval_seconds.max(1),
            ))
            .await;
            continue;
        }

        if let Some(result) = state.callback_result.clone() {
            let _ = cancel_login(Some(login_id));
            let callback = result?;

            let auth_token = if state.flow == OAuthFlow::IamSso {
                let region = state
                    .region
                    .clone()
                    .unwrap_or_else(|| "us-east-1".to_string());
                let client_id = state
                    .client_id
                    .clone()
                    .ok_or_else(|| "Missing OIDC clientId".to_string())?;
                let client_secret = state
                    .client_secret
                    .clone()
                    .ok_or_else(|| "Missing OIDC clientSecret".to_string())?;
                let code = callback
                    .code
                    .as_deref()
                    .and_then(|value| normalize_non_empty(Some(value)))
                    .ok_or_else(|| "Callback missing authorization code".to_string())?;
                let redirect_uri = build_token_exchange_redirect_uri(&state.callback_url, &callback);
                let mut token = exchange_oidc_authorization_code(
                    &region,
                    &client_id,
                    &client_secret,
                    &redirect_uri,
                    &code,
                    &state.code_verifier,
                )
                .await?;
                if !token.is_object() {
                    token = json!({});
                }
                if let Some(obj) = token.as_object_mut() {
                    obj.entry("provider".to_string())
                        .or_insert_with(|| Value::String("Enterprise".to_string()));
                    obj.entry("loginProvider".to_string())
                        .or_insert_with(|| Value::String("Enterprise".to_string()));
                    obj.entry("authMethod".to_string())
                        .or_insert_with(|| Value::String("IdC".to_string()));
                    obj.entry("login_option".to_string())
                        .or_insert_with(|| Value::String("enterprise".to_string()));
                    obj.entry("idc_region".to_string())
                        .or_insert_with(|| Value::String(region.clone()));
                    obj.entry("clientId".to_string())
                        .or_insert_with(|| Value::String(client_id.clone()));
                }
                token
            } else {
                let login_option = callback.login_option.trim().to_ascii_lowercase();
                if callback.code.is_none() {
                    let reason = match login_option.as_str() {
                        "builderid" | "awsidc" | "internal" => {
                            "Current login method requires Kiro client follow-up auth flow and is not supported for direct import. Please use BuilderId/Enterprise flow."
                        }
                        "external_idp" => {
                            "Current login method is External IdP and no authorization code was returned. Automatic import is not supported."
                        }
                        _ => "Callback missing authorization code, cannot complete login.",
                    };
                    return Err(reason.to_string());
                }
                let redirect_uri = build_token_exchange_redirect_uri(&state.callback_url, &callback);
                exchange_code_for_token(&callback, &state.code_verifier, &redirect_uri).await?
            };
            let payload = build_payload_from_snapshot(auth_token, None, None)?;
            return Ok(enrich_payload_with_runtime_usage(payload).await);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(OAUTH_POLL_INTERVAL_MS)).await;
    }
}

pub fn cancel_login(login_id: Option<&str>) -> Result<(), String> {
    let mut state = PENDING_OAUTH_STATE
        .lock()
        .map_err(|_| "OAuth state lock unavailable".to_string())?;

    match (state.as_ref(), login_id) {
        (Some(current), Some(input)) if current.login_id != input => {
            return Err("Login session mismatch, cancel failed".to_string());
        }
        (Some(_), _) => {
            *state = None;
        }
        (None, _) => {}
    }
    Ok(())
}

pub async fn build_payload_from_token(token: &str) -> Result<KiroOAuthCompletePayload, String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err("Token cannot be empty".to_string());
    }

    let mut snapshot = json!({
        "accessToken": trimmed,
        "tokenType": "Bearer"
    });
    if let Some(obj) = snapshot.as_object_mut() {
        if let Some(claims) = decode_jwt_claims(trimmed) {
            if let Some(email) = pick_string(
                Some(&claims),
                &[&["email"], &["upn"], &["preferred_username"]],
            ) {
                obj.insert("email".to_string(), Value::String(email.clone()));
                obj.insert("login_hint".to_string(), Value::String(email));
            }
            if let Some(user_id) = pick_string(Some(&claims), &[&["sub"], &["user_id"], &["uid"]]) {
                obj.insert("userId".to_string(), Value::String(user_id.clone()));
                obj.insert("sub".to_string(), Value::String(user_id));
            }
            if let Some(name) = pick_string(Some(&claims), &[&["name"], &["nickname"]]) {
                obj.insert("provider".to_string(), Value::String(name));
            }
        }
    }

    let payload = build_payload_from_snapshot(snapshot, None, None)?;
    Ok(enrich_payload_with_runtime_usage(payload).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_payload_from_snapshot_supports_kiro_raw_json_shape() {
        let auth_token = json!({
            "email": "3493729266@qq.com",
            "accessToken": "test_access_token",
            "refreshToken": "test_refresh_token",
            "expiresAt": "2026/02/19 02:01:47",
            "provider": "Github",
            "userId": "user-123",
            "profileArn": "arn:aws:codewhisperer:us-east-1:699475941385:profile/EHGA3GRVQMUK"
        });
        let usage = json!({
            "nextDateReset": 1772323200,
            "subscriptionInfo": {
                "subscriptionTitle": "KIRO FREE",
                "type": "Q_DEVELOPER_STANDALONE_FREE"
            },
            "usageBreakdownList": [
                {
                    "usageLimitWithPrecision": 50,
                    "currentUsageWithPrecision": 0,
                    "freeTrialInfo": {
                        "currentUsageWithPrecision": 189.24,
                        "usageLimitWithPrecision": 500,
                        "freeTrialExpiry": 4_102_444_800_i64
                    }
                }
            ],
            "userInfo": {
                "email": "3493729266@qq.com",
                "userId": "user-123"
            }
        });

        let payload =
            build_payload_from_snapshot(auth_token, None, Some(usage)).expect("payload should parse");

        let expected_expires_at = chrono::NaiveDateTime::parse_from_str(
            "2026/02/19 02:01:47",
            "%Y/%m/%d %H:%M:%S",
        )
        .expect("valid datetime")
        .and_utc()
        .timestamp();

        assert_eq!(payload.email, "3493729266@qq.com");
        assert_eq!(payload.user_id.as_deref(), Some("user-123"));
        assert_eq!(payload.login_provider.as_deref(), Some("Github"));
        assert_eq!(payload.access_token, "test_access_token");
        assert_eq!(payload.refresh_token.as_deref(), Some("test_refresh_token"));
        assert_eq!(payload.expires_at, Some(expected_expires_at));
        assert_eq!(payload.plan_name.as_deref(), Some("KIRO FREE"));
        assert_eq!(
            payload.plan_tier.as_deref(),
            Some("Q_DEVELOPER_STANDALONE_FREE")
        );
        assert_eq!(payload.credits_total, Some(50.0));
        assert_eq!(payload.credits_used, Some(0.0));
        assert_eq!(payload.bonus_total, Some(500.0));
        assert!(
            payload
                .bonus_used
                .map(|value| (value - 189.24).abs() < 0.0001)
                .unwrap_or(false),
            "bonus_used should parse from freeTrialInfo.currentUsageWithPrecision"
        );
        assert_eq!(payload.usage_reset_at, Some(1772323200));
        assert!(
            payload.bonus_expire_days.unwrap_or(-1) > 0,
            "bonus_expire_days should derive from freeTrialExpiry"
        );
    }
}

