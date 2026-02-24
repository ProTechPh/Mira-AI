use crate::modules::config::AntigravityProxyConfig;
use axum::http::HeaderMap;

#[derive(Debug, Clone)]
pub struct ApiKeyAuthResult {
    pub valid: bool,
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
    None
}

pub fn validate_api_key(config: &AntigravityProxyConfig, headers: &HeaderMap) -> ApiKeyAuthResult {
    if !config.auth_enabled {
        return ApiKeyAuthResult {
            valid: true,
            reason: None,
            status: 200,
        };
    }

    let expected = config.api_key.as_deref().unwrap_or("").trim();
    if expected.is_empty() {
        return ApiKeyAuthResult {
            valid: false,
            reason: Some("Proxy auth enabled but API key is not configured".to_string()),
            status: 401,
        };
    }

    let Some(provided) = extract_api_key(headers) else {
        return ApiKeyAuthResult {
            valid: false,
            reason: Some("Missing API key".to_string()),
            status: 401,
        };
    };

    if provided != expected {
        return ApiKeyAuthResult {
            valid: false,
            reason: Some("Invalid API key".to_string()),
            status: 401,
        };
    }

    ApiKeyAuthResult {
        valid: true,
        reason: None,
        status: 200,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::config::AntigravityProxyConfig;

    #[test]
    fn auth_toggle_off_allows_request() {
        let cfg = AntigravityProxyConfig {
            auth_enabled: false,
            api_key: None,
            ..AntigravityProxyConfig::default()
        };
        let headers = HeaderMap::new();
        let result = validate_api_key(&cfg, &headers);
        assert!(result.valid);
    }
}
