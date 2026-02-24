use crate::modules::config::{KiroProxyApiKey, KiroProxyConfig};
use axum::http::HeaderMap;

#[derive(Debug, Clone)]
pub struct ApiKeyAuthResult {
    pub valid: bool,
    pub matched: Option<KiroProxyApiKey>,
    pub reason: Option<String>,
    pub status: u16,
}

pub fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get("authorization") {
        if let Ok(raw) = value.to_str() {
            if let Some(token) = raw.strip_prefix("Bearer ") {
                let token = token.trim();
                if !token.is_empty() {
                    return Some(token.to_string());
                }
            }
        }
    }

    if let Some(value) = headers.get("x-api-key") {
        if let Ok(raw) = value.to_str() {
            let token = raw.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }

    if let Some(value) = headers.get("X-Api-Key") {
        if let Ok(raw) = value.to_str() {
            let token = raw.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }

    None
}

pub fn validate_api_key(config: &KiroProxyConfig, headers: &HeaderMap) -> ApiKeyAuthResult {
    let has_multi_keys = !config.api_keys.is_empty();
    let has_single_key = config
        .api_key
        .as_deref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);

    if !has_multi_keys && !has_single_key {
        return ApiKeyAuthResult {
            valid: true,
            matched: None,
            reason: None,
            status: 200,
        };
    }

    let Some(provided) = extract_api_key(headers) else {
        return ApiKeyAuthResult {
            valid: false,
            matched: None,
            reason: Some("Missing API key".to_string()),
            status: 401,
        };
    };

    if has_multi_keys {
        if let Some(key) = config
            .api_keys
            .iter()
            .find(|entry| entry.enabled && entry.key == provided)
            .cloned()
        {
            if let Some(limit) = key.credits_limit {
                if key.usage.total_credits >= limit {
                    return ApiKeyAuthResult {
                        valid: false,
                        matched: Some(key),
                        reason: Some("Credits limit exceeded".to_string()),
                        status: 429,
                    };
                }
            }

            return ApiKeyAuthResult {
                valid: true,
                matched: Some(key),
                reason: None,
                status: 200,
            };
        }
    }

    if has_single_key && config.api_key.as_deref() == Some(provided.as_str()) {
        return ApiKeyAuthResult {
            valid: true,
            matched: None,
            reason: None,
            status: 200,
        };
    }

    ApiKeyAuthResult {
        valid: false,
        matched: None,
        reason: Some("Invalid API key".to_string()),
        status: 401,
    }
}
