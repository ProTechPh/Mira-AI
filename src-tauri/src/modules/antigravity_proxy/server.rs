use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use bytes::Bytes;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use crate::models::Account;
use crate::modules::account;
use crate::modules::quota;

use super::service::AntigravityProxyService;
use super::translator::{create_openai_chunk, to_cloud_code_payload};
use super::types::{OpenAiChatRequest, ProxyRequestLog};

#[derive(Clone)]
struct AppState {
    service: Arc<AntigravityProxyService>,
}

pub async fn serve(
    listener: tokio::net::TcpListener,
    service: Arc<AntigravityProxyService>,
    shutdown_rx: oneshot::Receiver<()>,
) -> Result<(), String> {
    let state = AppState { service };

    let app = Router::new()
        .route("/", get(handle_health))
        .route("/health", get(handle_health))
        .route("/v1/models", get(handle_models))
        .route("/models", get(handle_models))
        .route("/v1/chat/completions", post(handle_openai_chat))
        .route("/chat/completions", post(handle_openai_chat))
        .with_state(state);

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        })
        .await
        .map_err(|e| format!("Antigravity Proxy 服务退出: {}", e))
}

fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    let payload = json!({
        "error": {
            "message": message.into(),
            "type": "error",
            "code": status.as_u16(),
        }
    });
    (status, Json(payload)).into_response()
}

async fn verify_auth(state: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    let auth = state.service.verify_auth(headers).await;
    if auth.valid {
        return Ok(());
    }

    let status = StatusCode::from_u16(auth.status).unwrap_or(StatusCode::UNAUTHORIZED);
    Err(json_error(
        status,
        auth.reason.unwrap_or_else(|| "Unauthorized".to_string()),
    ))
}

async fn handle_health(State(state): State<AppState>) -> impl IntoResponse {
    let status = state.service.status().await;
    Json(json!({
        "status": if status.running { "ok" } else { "stopped" },
        "running": status.running,
        "host": status.host,
        "port": status.port,
        "startedAt": status.started_at,
        "uptimeSeconds": status.uptime_seconds,
        "requests": {
            "total": status.request_count,
            "success": status.success_count,
            "failed": status.failed_count,
        },
        "tokens": {
            "input": status.total_input_tokens,
            "output": status.total_output_tokens,
        }
    }))
}

async fn handle_models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = verify_auth(&state, &headers).await {
        return resp;
    }

    match state.service.get_models().await {
        Ok(models) => {
            let now = chrono::Utc::now().timestamp();
            let data = models
                .into_iter()
                .map(|item| {
                    json!({
                        "id": item.id,
                        "object": "model",
                        "created": now,
                        "owned_by": item.source,
                        "name": item.name,
                        "description": item.description,
                    })
                })
                .collect::<Vec<_>>();

            Json(json!({ "object": "list", "data": data })).into_response()
        }
        Err(err) => json_error(StatusCode::BAD_GATEWAY, err),
    }
}

async fn try_non_stream_openai(
    state: &AppState,
    request: &OpenAiChatRequest,
) -> Result<(Value, String, String, u64, u64), String> {
    let config = state.service.current_config_snapshot().await;
    let mut last_error: Option<String> = None;

    for attempt in 0..config.max_retries.max(1) {
        let Some(pool_account) = state.service.take_next_account().await else {
            return Err("未找到可用 Antigravity 账号".to_string());
        };

        let mut account = pool_account.account;
        ensure_project_id_for_account(&mut account, false).await?;

        let payload_with_project = to_cloud_code_payload(
            request,
            account.token.project_id.as_deref(),
            account.token.session_id.as_deref(),
        )?;

        let mut refreshed_once = false;
        let mut retried_without_project = false;
        let mut payload = payload_with_project.clone();

        loop {
            match super::api::generate_content(&account, &payload).await {
                Ok((content, usage)) => {
                    state.service.mark_account_success(&account.id).await;
                    let response = json!({
                        "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                        "object": "chat.completion",
                        "created": chrono::Utc::now().timestamp(),
                        "model": request.model,
                        "choices": [{
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": content,
                            },
                            "finish_reason": "stop"
                        }],
                        "usage": usage,
                    });
                    return Ok((response, account.id, account.email, usage.prompt_tokens, usage.completion_tokens));
                }
                Err(err) => {
                    last_error = Some(err.clone());
                    let status_code = parse_upstream_status(&err);

                    if !retried_without_project
                        && err.contains("Invalid project resource name")
                    {
                        retried_without_project = true;
                        ensure_project_id_for_account(&mut account, true).await?;
                        payload = to_cloud_code_payload(
                            request,
                            account.token.project_id.as_deref(),
                            account.token.session_id.as_deref(),
                        )?;
                        continue;
                    }

                    if matches!(status_code, Some(401 | 403)) && !refreshed_once {
                        if let Ok(updated) = state.service.refresh_account_token(&account.id).await {
                            account = updated;
                            refreshed_once = true;
                            continue;
                        }
                    }

                    if matches!(status_code, Some(429)) {
                        state.service.mark_account_error(&account.id, true).await;
                        break;
                    }

                    if status_code.unwrap_or(500) >= 500 {
                        state.service.mark_account_error(&account.id, false).await;
                        tokio::time::sleep(std::time::Duration::from_millis(
                            config.retry_delay_ms.saturating_mul((attempt + 1) as u64),
                        ))
                        .await;
                        break;
                    }

                    return Err(err);
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| "请求 Antigravity 上游失败".to_string()))
}

async fn handle_openai_chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    Json(request): Json<OpenAiChatRequest>,
) -> Response {
    if let Err(resp) = verify_auth(&state, &headers).await {
        return resp;
    }

    if request.model.trim().is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "model is required");
    }
    if request.messages.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "messages is required");
    }

    let started = std::time::Instant::now();
    let request_path = "/v1/chat/completions";
    let stream = request.stream.unwrap_or(false);

    if !stream {
        match try_non_stream_openai(&state, &request).await {
            Ok((response, account_id, account_email, input_tokens, output_tokens)) => {
                let log = ProxyRequestLog {
                    timestamp: chrono::Utc::now().timestamp(),
                    path: request_path.to_string(),
                    method: method.to_string(),
                    model: Some(request.model.clone()),
                    account_id: Some(account_id),
                    account_email: Some(account_email),
                    input_tokens,
                    output_tokens,
                    response_time_ms: started.elapsed().as_millis() as u64,
                    status: 200,
                    success: true,
                    error: None,
                };
                let _ = state
                    .service
                    .record_request_log(log, Some(json!({ "path": request_path, "status": 200, "model": request.model })))
                    .await;

                return Json(response).into_response();
            }
            Err(err) => {
                let status = StatusCode::from_u16(parse_upstream_status(&err).unwrap_or(500))
                    .unwrap_or(StatusCode::BAD_GATEWAY);
                let log = ProxyRequestLog {
                    timestamp: chrono::Utc::now().timestamp(),
                    path: request_path.to_string(),
                    method: method.to_string(),
                    model: Some(request.model.clone()),
                    account_id: None,
                    account_email: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    response_time_ms: started.elapsed().as_millis() as u64,
                    status: status.as_u16(),
                    success: false,
                    error: Some(err.clone()),
                };
                let _ = state
                    .service
                    .record_request_log(log, Some(json!({ "path": request_path, "status": status.as_u16(), "error": err.clone(), "model": request.model })))
                    .await;
                return json_error(status, err);
            }
        }
    }

    handle_openai_stream(state, request).await
}

async fn handle_openai_stream(state: AppState, request: OpenAiChatRequest) -> Response {
    let config = state.service.current_config_snapshot().await;
    let started_at = std::time::Instant::now();
    let request_path = "/v1/chat/completions".to_string();
    let (tx, rx) = mpsc::unbounded_channel::<Result<Bytes, Infallible>>();
    let service = Arc::clone(&state.service);

    tokio::spawn(async move {
        let stream_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
        let _ = tx.send(Ok(Bytes::from(format!(
            "data: {}\n\n",
            create_openai_chunk(&stream_id, &request.model, json!({ "role": "assistant" }), None, None)
        ))));

        let mut final_input_tokens = 0u64;
        let mut final_output_tokens = 0u64;
        let mut last_err: Option<String> = None;

        for attempt in 0..config.max_retries.max(1) {
            let Some(pool_account) = service.take_next_account().await else {
                last_err = Some("未找到可用 Antigravity 账号".to_string());
                break;
            };
            let mut account = pool_account.account;

            if let Err(err) = ensure_project_id_for_account(&mut account, false).await {
                last_err = Some(err);
                break;
            }

            let payload_with_project = match to_cloud_code_payload(
                &request,
                account.token.project_id.as_deref(),
                account.token.session_id.as_deref(),
            ) {
                Ok(v) => v,
                Err(err) => {
                    last_err = Some(err);
                    break;
                }
            };
            let mut refreshed_once = false;
            let mut retried_without_project = false;
            let mut payload = payload_with_project.clone();
            loop {
                match super::api::stream_generate_content(&account, &payload, |text| {
                    let line = create_openai_chunk(&stream_id, &request.model, json!({ "content": text }), None, None);
                    let _ = tx.send(Ok(Bytes::from(format!("data: {}\n\n", line.to_string()))));
                })
                .await
                {
                    Ok(usage) => {
                        final_input_tokens = usage.prompt_tokens;
                        final_output_tokens = usage.completion_tokens;
                        service.mark_account_success(&account.id).await;

                        let finish = create_openai_chunk(
                            &stream_id,
                            &request.model,
                            json!({}),
                            Some("stop"),
                            Some(usage),
                        );
                        let _ = tx.send(Ok(Bytes::from(format!("data: {}\n\n", finish.to_string()))));
                        let _ = tx.send(Ok(Bytes::from("data: [DONE]\n\n")));

                        let log = ProxyRequestLog {
                            timestamp: chrono::Utc::now().timestamp(),
                            path: request_path.clone(),
                            method: "POST".to_string(),
                            model: Some(request.model.clone()),
                            account_id: Some(account.id.clone()),
                            account_email: Some(account.email.clone()),
                            input_tokens: final_input_tokens,
                            output_tokens: final_output_tokens,
                            response_time_ms: started_at.elapsed().as_millis() as u64,
                            status: 200,
                            success: true,
                            error: None,
                        };
                        let _ = service
                            .record_request_log(log, Some(json!({ "path": request_path, "status": 200, "model": request.model })))
                            .await;
                        return;
                    }
                    Err(err) => {
                        last_err = Some(err.clone());
                        let status_code = parse_upstream_status(&err);

                        if !retried_without_project
                            && err.contains("Invalid project resource name")
                        {
                            retried_without_project = true;
                            if let Err(refresh_err) = ensure_project_id_for_account(&mut account, true).await {
                                last_err = Some(refresh_err);
                                break;
                            }
                            payload = match to_cloud_code_payload(
                                &request,
                                account.token.project_id.as_deref(),
                                account.token.session_id.as_deref(),
                            ) {
                                Ok(v) => v,
                                Err(payload_err) => {
                                    last_err = Some(payload_err);
                                    break;
                                }
                            };
                            continue;
                        }

                        if matches!(status_code, Some(401 | 403)) && !refreshed_once {
                            if let Ok(updated) = service.refresh_account_token(&account.id).await {
                                account = updated;
                                refreshed_once = true;
                                continue;
                            }
                        }

                        if matches!(status_code, Some(429)) {
                            service.mark_account_error(&account.id, true).await;
                            break;
                        }

                        if status_code.unwrap_or(500) >= 500 {
                            service.mark_account_error(&account.id, false).await;
                            tokio::time::sleep(std::time::Duration::from_millis(
                                config.retry_delay_ms.saturating_mul((attempt + 1) as u64),
                            ))
                            .await;
                            break;
                        }

                        let status = status_code.unwrap_or(500);
                        let _ = tx.send(Ok(Bytes::from(format!(
                            "data: {}\n\n",
                            json!({"error": { "message": err, "code": status }}).to_string()
                        ))));
                        let _ = tx.send(Ok(Bytes::from("data: [DONE]\n\n")));
                        return;
                    }
                }
            }
        }

        let err = last_err.unwrap_or_else(|| "请求 Antigravity 上游失败".to_string());
        let status = parse_upstream_status(&err).unwrap_or(500);
        let _ = tx.send(Ok(Bytes::from(format!(
            "data: {}\n\n",
            json!({"error": { "message": err.clone(), "code": status }}).to_string()
        ))));
        let _ = tx.send(Ok(Bytes::from("data: [DONE]\n\n")));

        let log = ProxyRequestLog {
            timestamp: chrono::Utc::now().timestamp(),
            path: request_path,
            method: "POST".to_string(),
            model: Some(request.model.clone()),
            account_id: None,
            account_email: None,
            input_tokens: final_input_tokens,
            output_tokens: final_output_tokens,
            response_time_ms: started_at.elapsed().as_millis() as u64,
            status,
            success: false,
            error: Some(err),
        };
        let _ = service
            .record_request_log(log, Some(json!({ "path": "/v1/chat/completions", "status": status, "model": request.model })))
            .await;
    });

    let stream = UnboundedReceiverStream::new(rx).map(|item| item);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("Connection", "keep-alive")
        .body(body)
        .unwrap_or_else(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "stream build failed"))
}

fn parse_upstream_status(error: &str) -> Option<u16> {
    let prefix = "UPSTREAM_STATUS:";
    let rest = error.strip_prefix(prefix)?;
    let status_text = rest.split(':').next()?;
    status_text.parse::<u16>().ok()
}

fn is_invalid_project_id(value: Option<&str>) -> bool {
    let Some(raw) = value.map(str::trim) else {
        return true;
    };
    raw.is_empty() || raw == "projects" || raw == "projects/" || raw.ends_with("projects/")
}

fn generate_fallback_project_id() -> String {
    format!("projects/random-{}/locations/global", uuid::Uuid::new_v4().simple())
}

async fn ensure_project_id_for_account(account_data: &mut Account, force_refresh: bool) -> Result<(), String> {
    if !force_refresh && !is_invalid_project_id(account_data.token.project_id.as_deref()) {
        return Ok(());
    }

    let (project_id, _tier) = quota::fetch_project_id_for_token(&account_data.token, &account_data.email).await;
    let resolved = project_id
        .or_else(|| account_data.token.project_id.clone())
        .filter(|value| !is_invalid_project_id(Some(value.as_str())))
        .unwrap_or_else(generate_fallback_project_id);

    if account_data.token.project_id.as_deref() != Some(resolved.as_str()) {
        account_data.token.project_id = Some(resolved);
        account::save_account(account_data)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_from_upstream_error() {
        assert_eq!(parse_upstream_status("UPSTREAM_STATUS:429:rate"), Some(429));
        assert_eq!(parse_upstream_status("bad"), None);
    }
}
