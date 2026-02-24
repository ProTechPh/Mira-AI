use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use bytes::Bytes;
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use crate::modules::kiro_proxy::kiro_api::{
    call_generate_assistant_response, call_generate_assistant_response_stream, ordered_endpoints,
};

use super::service::KiroProxyService;
use super::translator::{
    claude_to_kiro, create_claude_stream_event, create_openai_stream_chunk, kiro_to_claude_response,
    kiro_to_openai_response, openai_to_kiro,
};
use super::types::{ClaudeRequest, KiroStreamMessage, OpenAiChatRequest, ProxyRequestLog};

#[derive(Clone)]
struct AppState {
    service: Arc<KiroProxyService>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct LogsQuery {
    limit: Option<usize>,
}

pub async fn serve(
    listener: tokio::net::TcpListener,
    service: Arc<KiroProxyService>,
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
        .route("/v1/messages", post(handle_claude_messages))
        .route("/messages", post(handle_claude_messages))
        .route("/anthropic/v1/messages", post(handle_claude_messages))
        .route("/v1/messages/count_tokens", post(handle_count_tokens))
        .route("/messages/count_tokens", post(handle_count_tokens))
        .route("/api/event_logging/batch", post(handle_event_logging_batch))
        .route("/admin/stats", get(handle_admin_stats))
        .route("/admin/accounts", get(handle_admin_accounts))
        .route("/admin/logs", get(handle_admin_logs))
        .route("/admin/config", get(handle_admin_config).post(handle_admin_update_config))
        .with_state(state);

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        })
        .await
        .map_err(|e| format!("Kiro Proxy 服务退出: {}", e))
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

async fn verify_auth(
    state: &AppState,
    headers: &HeaderMap,
    path: &str,
) -> Result<Option<String>, Response> {
    if !state.service.should_authenticate_path(path).await {
        return Ok(None);
    }

    let auth = state.service.verify_admin_auth(headers).await;
    if !auth.valid {
        let status = StatusCode::from_u16(auth.status).unwrap_or(StatusCode::UNAUTHORIZED);
        return Err(json_error(
            status,
            auth.reason.unwrap_or_else(|| "Unauthorized".to_string()),
        ));
    }

    Ok(auth.matched.map(|key| key.id))
}

async fn handle_health(State(state): State<AppState>) -> impl IntoResponse {
    let status = state.service.status().await;
    let aggregate = state.service.aggregate_snapshot().await;
    Json(json!({
        "status": if status.running { "ok" } else { "stopped" },
        "running": status.running,
        "host": status.host,
        "port": status.port,
        "startedAt": status.started_at,
        "uptimeSeconds": status.uptime_seconds,
        "requests": {
            "total": aggregate.total_requests,
            "success": aggregate.success_requests,
            "failed": aggregate.failed_requests,
        },
        "tokens": {
            "input": aggregate.total_input_tokens,
            "output": aggregate.total_output_tokens,
        },
        "credits": aggregate.total_credits,
    }))
}

async fn handle_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = verify_auth(&state, &headers, "/v1/models").await {
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

fn parse_upstream_status(error: &str) -> Option<u16> {
    let prefix = "UPSTREAM_STATUS:";
    let rest = error.strip_prefix(prefix)?;
    let status_text = rest.split(':').next()?;
    status_text.parse::<u16>().ok()
}

async fn try_non_stream_openai(
    state: &AppState,
    mut request: OpenAiChatRequest,
    api_key_id: Option<&str>,
) -> Result<(Value, String, String, u64, u64, f64, u16), String> {
    request.model = state
        .service
        .model_mapping_for(&request.model, api_key_id)
        .await;

    let config = state.service.current_config_snapshot().await;
    let mut last_error: Option<String> = None;

    for attempt in 0..config.max_retries.max(1) {
        let Some(pool_account) = state.service.take_next_account().await else {
            return Err("未找到可用 Kiro 账号".to_string());
        };
        let mut account = pool_account.account;

        for endpoint in ordered_endpoints(config.preferred_endpoint.as_deref()) {
            let payload = openai_to_kiro(&request, super::account_pool::extract_profile_arn(&account), config.disable_tools);

            match call_generate_assistant_response(&account, &endpoint, payload).await {
                Ok(result) => {
                    state.service.mark_account_success(&account.id).await;
                    let response =
                        kiro_to_openai_response(result.content, result.tool_uses, result.usage.clone(), request.model.clone());
                    return Ok((
                        response,
                        account.id,
                        account.email,
                        result.usage.input_tokens,
                        result.usage.output_tokens,
                        result.usage.credits,
                        StatusCode::OK.as_u16(),
                    ));
                }
                Err(err) => {
                    last_error = Some(err.clone());
                    let status_code = parse_upstream_status(&err);
                    if matches!(status_code, Some(401 | 403)) {
                        if let Ok(updated) = state.service.refresh_account_token(&account.id).await {
                            account = updated;
                            continue;
                        }
                    }

                    if matches!(status_code, Some(429)) {
                        state.service.mark_account_error(&account.id, true).await;
                        continue;
                    }

                    if status_code.unwrap_or(500) >= 500 {
                        state.service.mark_account_error(&account.id, false).await;
                        tokio::time::sleep(std::time::Duration::from_millis(
                            config.retry_delay_ms.saturating_mul((attempt + 1) as u64),
                        ))
                        .await;
                        continue;
                    }
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| "请求 Kiro 上游失败".to_string()))
}

async fn handle_openai_chat(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    Json(request): Json<OpenAiChatRequest>,
) -> Response {
    let request_path = "/v1/chat/completions";
    let auth = match verify_auth(&state, &headers, request_path).await {
        Ok(api_key_id) => api_key_id,
        Err(resp) => return resp,
    };

    let is_stream = request.stream.unwrap_or(false);

    state
        .service
        .emit_request_event(json!({
            "path": request_path,
            "method": method.as_str(),
            "model": request.model,
            "stream": is_stream,
        }))
        .await;

    if !is_stream {
        let started = std::time::Instant::now();
        match try_non_stream_openai(&state, request.clone(), auth.as_deref()).await {
            Ok((response, account_id, account_email, input_tokens, output_tokens, credits, status)) => {
                let _ = state
                    .service
                    .apply_api_key_usage(
                        auth.as_deref(),
                        credits,
                        input_tokens,
                        output_tokens,
                        Some(request.model.as_str()),
                        request_path,
                    )
                    .await;

                let response_time_ms = started.elapsed().as_millis() as u64;
                let log = ProxyRequestLog {
                    timestamp: chrono::Utc::now().timestamp(),
                    path: request_path.to_string(),
                    method: method.to_string(),
                    model: Some(request.model.clone()),
                    account_id: Some(account_id),
                    account_email: Some(account_email),
                    api_key_id: auth.clone(),
                    input_tokens,
                    output_tokens,
                    credits,
                    response_time_ms,
                    status,
                    success: true,
                    error: None,
                };
                let _ = state
                    .service
                    .record_request_log(
                        log,
                        Some(json!({
                            "path": request_path,
                            "status": status,
                            "model": request.model,
                            "inputTokens": input_tokens,
                            "outputTokens": output_tokens,
                            "credits": credits,
                            "responseTimeMs": response_time_ms,
                        })),
                    )
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
                    api_key_id: auth,
                    input_tokens: 0,
                    output_tokens: 0,
                    credits: 0.0,
                    response_time_ms: started.elapsed().as_millis() as u64,
                    status: status.as_u16(),
                    success: false,
                    error: Some(err.clone()),
                };
                let _ = state
                    .service
                    .record_request_log(
                        log,
                        Some(json!({
                            "path": request_path,
                            "status": status.as_u16(),
                            "error": err,
                            "model": request.model,
                        })),
                    )
                    .await;
                return json_error(status, err);
            }
        }
    }

    handle_openai_stream(state, request, auth).await
}

async fn handle_openai_stream(
    state: AppState,
    mut request: OpenAiChatRequest,
    api_key_id: Option<String>,
) -> Response {
    request.model = state
        .service
        .model_mapping_for(&request.model, api_key_id.as_deref())
        .await;

    let config = state.service.current_config_snapshot().await;
    let Some(pool_account) = state.service.take_next_account().await else {
        return json_error(StatusCode::SERVICE_UNAVAILABLE, "No available accounts");
    };

    let account = pool_account.account;
    let account_id = account.id.clone();
    let account_email = account.email.clone();
    let model = request.model.clone();
    let request_path = "/v1/chat/completions".to_string();
    let started_at = std::time::Instant::now();
    let (tx, rx) = mpsc::unbounded_channel::<Result<Bytes, Infallible>>();

    let service = Arc::clone(&state.service);

    tokio::spawn(async move {
        let stream_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
        let _ = tx.send(Ok(Bytes::from(format!(
            "data: {}\n\n",
            create_openai_stream_chunk(
                &stream_id,
                &model,
                json!({ "role": "assistant" }),
                None,
                None,
            )
            .to_string()
        ))));

        let mut input_tokens = 0u64;
        let mut output_tokens = 0u64;
        let mut credits = 0.0f64;
        let mut tool_calls = 0usize;

        let payload = openai_to_kiro(
            &request,
            super::account_pool::extract_profile_arn(&account),
            config.disable_tools,
        );
        let input_chars = payload.to_string().chars().count();

        let call_result = call_generate_assistant_response_stream(
            &account,
            &ordered_endpoints(config.preferred_endpoint.as_deref())[0],
            payload,
            input_chars,
            |msg| {
                let line = match msg {
                    KiroStreamMessage::Text { text } => {
                        create_openai_stream_chunk(&stream_id, &model, json!({ "content": text }), None, None)
                    }
                    KiroStreamMessage::Thinking { text } => {
                        let format = config.thinking_output_format.as_str();
                        if format == "thinking" {
                            create_openai_stream_chunk(
                                &stream_id,
                                &model,
                                json!({ "content": format!("<thinking>{}</thinking>", text) }),
                                None,
                                None,
                            )
                        } else if format == "think" {
                            create_openai_stream_chunk(
                                &stream_id,
                                &model,
                                json!({ "content": format!("<think>{}</think>", text) }),
                                None,
                                None,
                            )
                        } else {
                            create_openai_stream_chunk(
                                &stream_id,
                                &model,
                                json!({ "reasoning_content": text }),
                                None,
                                None,
                            )
                        }
                    }
                    KiroStreamMessage::ToolUse { tool_use } => {
                        let idx = tool_calls;
                        tool_calls += 1;
                        create_openai_stream_chunk(
                            &stream_id,
                            &model,
                            json!({
                                "tool_calls": [{
                                    "index": idx,
                                    "id": tool_use.tool_use_id,
                                    "type": "function",
                                    "function": {
                                        "name": tool_use.name,
                                        "arguments": tool_use.input.to_string(),
                                    }
                                }]
                            }),
                            None,
                            None,
                        )
                    }
                };

                let _ = tx.send(Ok(Bytes::from(format!("data: {}\n\n", line.to_string()))));
            },
        )
        .await;

        match call_result {
            Ok(usage) => {
                input_tokens = usage.input_tokens;
                output_tokens = usage.output_tokens;
                credits = usage.credits;
                service.mark_account_success(&account_id).await;

                let finish = create_openai_stream_chunk(
                    &stream_id,
                    &model,
                    json!({}),
                    Some(if tool_calls > 0 { "tool_calls" } else { "stop" }),
                    Some(usage),
                );

                let _ = tx.send(Ok(Bytes::from(format!("data: {}\n\n", finish.to_string()))));
                let _ = tx.send(Ok(Bytes::from("data: [DONE]\n\n")));

                let _ = service
                    .apply_api_key_usage(
                        api_key_id.as_deref(),
                        credits,
                        input_tokens,
                        output_tokens,
                        Some(model.as_str()),
                        request_path.as_str(),
                    )
                    .await;

                let log = ProxyRequestLog {
                    timestamp: chrono::Utc::now().timestamp(),
                    path: request_path.clone(),
                    method: "POST".to_string(),
                    model: Some(model.clone()),
                    account_id: Some(account_id.clone()),
                    account_email: Some(account_email.clone()),
                    api_key_id: api_key_id.clone(),
                    input_tokens,
                    output_tokens,
                    credits,
                    response_time_ms: started_at.elapsed().as_millis() as u64,
                    status: 200,
                    success: true,
                    error: None,
                };
                let _ = service
                    .record_request_log(
                        log,
                        Some(json!({
                            "path": request_path,
                            "status": 200,
                            "model": model,
                            "inputTokens": input_tokens,
                            "outputTokens": output_tokens,
                            "credits": credits,
                        })),
                    )
                    .await;
            }
            Err(err) => {
                let status = parse_upstream_status(&err).unwrap_or(500);
                service
                    .mark_account_error(&account_id, status == 429)
                    .await;
                let _ = tx.send(Ok(Bytes::from(format!(
                    "data: {}\n\n",
                    json!({ "error": { "message": err } }).to_string()
                ))));

                let log = ProxyRequestLog {
                    timestamp: chrono::Utc::now().timestamp(),
                    path: request_path.clone(),
                    method: "POST".to_string(),
                    model: Some(model.clone()),
                    account_id: Some(account_id.clone()),
                    account_email: Some(account_email.clone()),
                    api_key_id: api_key_id.clone(),
                    input_tokens,
                    output_tokens,
                    credits,
                    response_time_ms: started_at.elapsed().as_millis() as u64,
                    status,
                    success: false,
                    error: Some(err.clone()),
                };
                let _ = service
                    .record_request_log(
                        log,
                        Some(json!({
                            "path": request_path,
                            "status": status,
                            "model": model,
                            "error": err,
                        })),
                    )
                    .await;
            }
        }
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

async fn try_non_stream_claude(
    state: &AppState,
    mut request: ClaudeRequest,
    api_key_id: Option<&str>,
) -> Result<(Value, String, String, u64, u64, f64), String> {
    request.model = state
        .service
        .model_mapping_for(&request.model, api_key_id)
        .await;

    let config = state.service.current_config_snapshot().await;
    let mut last_error: Option<String> = None;

    for attempt in 0..config.max_retries.max(1) {
        let Some(pool_account) = state.service.take_next_account().await else {
            return Err("未找到可用 Kiro 账号".to_string());
        };

        let mut account = pool_account.account;

        for endpoint in ordered_endpoints(config.preferred_endpoint.as_deref()) {
            let payload = claude_to_kiro(&request, super::account_pool::extract_profile_arn(&account));
            match call_generate_assistant_response(&account, &endpoint, payload).await {
                Ok(result) => {
                    state.service.mark_account_success(&account.id).await;
                    let response =
                        kiro_to_claude_response(result.content, result.tool_uses, result.usage.clone(), request.model.clone());
                    return Ok((
                        response,
                        account.id,
                        account.email,
                        result.usage.input_tokens,
                        result.usage.output_tokens,
                        result.usage.credits,
                    ));
                }
                Err(err) => {
                    last_error = Some(err.clone());
                    let status_code = parse_upstream_status(&err);
                    if matches!(status_code, Some(401 | 403)) {
                        if let Ok(updated) = state.service.refresh_account_token(&account.id).await {
                            account = updated;
                            continue;
                        }
                    }

                    if matches!(status_code, Some(429)) {
                        state.service.mark_account_error(&account.id, true).await;
                        continue;
                    }

                    if status_code.unwrap_or(500) >= 500 {
                        state.service.mark_account_error(&account.id, false).await;
                        tokio::time::sleep(std::time::Duration::from_millis(
                            config.retry_delay_ms.saturating_mul((attempt + 1) as u64),
                        ))
                        .await;
                        continue;
                    }
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| "请求 Kiro 上游失败".to_string()))
}

async fn handle_claude_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    method: Method,
    Json(request): Json<ClaudeRequest>,
) -> Response {
    let request_path = "/v1/messages";
    let auth = match verify_auth(&state, &headers, request_path).await {
        Ok(api_key_id) => api_key_id,
        Err(resp) => return resp,
    };

    let is_stream = request.stream.unwrap_or(false);
    state
        .service
        .emit_request_event(json!({
            "path": request_path,
            "method": method.as_str(),
            "model": request.model,
            "stream": is_stream,
        }))
        .await;

    if !is_stream {
        let started = std::time::Instant::now();
        return match try_non_stream_claude(&state, request.clone(), auth.as_deref()).await {
            Ok((response, account_id, account_email, input_tokens, output_tokens, credits)) => {
                let _ = state
                    .service
                    .apply_api_key_usage(
                        auth.as_deref(),
                        credits,
                        input_tokens,
                        output_tokens,
                        Some(request.model.as_str()),
                        request_path,
                    )
                    .await;

                let log = ProxyRequestLog {
                    timestamp: chrono::Utc::now().timestamp(),
                    path: request_path.to_string(),
                    method: method.to_string(),
                    model: Some(request.model.clone()),
                    account_id: Some(account_id),
                    account_email: Some(account_email),
                    api_key_id: auth,
                    input_tokens,
                    output_tokens,
                    credits,
                    response_time_ms: started.elapsed().as_millis() as u64,
                    status: 200,
                    success: true,
                    error: None,
                };
                let _ = state
                    .service
                    .record_request_log(
                        log,
                        Some(json!({
                            "path": request_path,
                            "status": 200,
                            "model": request.model,
                            "inputTokens": input_tokens,
                            "outputTokens": output_tokens,
                            "credits": credits,
                        })),
                    )
                    .await;
                Json(response).into_response()
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
                    api_key_id: auth,
                    input_tokens: 0,
                    output_tokens: 0,
                    credits: 0.0,
                    response_time_ms: started.elapsed().as_millis() as u64,
                    status: status.as_u16(),
                    success: false,
                    error: Some(err.clone()),
                };
                let _ = state
                    .service
                    .record_request_log(
                        log,
                        Some(json!({
                            "path": request_path,
                            "status": status.as_u16(),
                            "model": request.model,
                            "error": err,
                        })),
                    )
                    .await;
                json_error(status, err)
            }
        };
    }

    handle_claude_stream(state, request, auth).await
}

async fn handle_claude_stream(
    state: AppState,
    mut request: ClaudeRequest,
    api_key_id: Option<String>,
) -> Response {
    request.model = state
        .service
        .model_mapping_for(&request.model, api_key_id.as_deref())
        .await;

    let config = state.service.current_config_snapshot().await;
    let Some(pool_account) = state.service.take_next_account().await else {
        return json_error(StatusCode::SERVICE_UNAVAILABLE, "No available accounts");
    };

    let account = pool_account.account;
    let account_id = account.id.clone();
    let account_email = account.email.clone();
    let request_path = "/v1/messages".to_string();
    let model = request.model.clone();
    let started_at = std::time::Instant::now();

    let (tx, rx) = mpsc::unbounded_channel::<Result<Bytes, Infallible>>();
    let service = Arc::clone(&state.service);

    tokio::spawn(async move {
        let message_id = format!("msg_{}", uuid::Uuid::new_v4());
        let _ = tx.send(Ok(Bytes::from(format!(
            "event: message_start\ndata: {}\n\n",
            create_claude_stream_event(
                "message_start",
                json!({
                    "message": {
                        "id": message_id,
                        "type": "message",
                        "role": "assistant",
                        "content": [],
                        "model": model,
                        "stop_reason": Value::Null,
                        "stop_sequence": Value::Null,
                        "usage": {"input_tokens": 0, "output_tokens": 0}
                    }
                })
            )
            .to_string()
        ))));

        let payload = claude_to_kiro(&request, super::account_pool::extract_profile_arn(&account));
        let input_chars = payload.to_string().chars().count();

        let mut content_block_index = 0usize;
        let mut input_tokens = 0u64;
        let mut output_tokens = 0u64;
        let mut credits = 0.0f64;
        let mut has_text_block = false;
        let mut has_tool_calls = false;

        let call_result = call_generate_assistant_response_stream(
            &account,
            &ordered_endpoints(config.preferred_endpoint.as_deref())[0],
            payload,
            input_chars,
            |msg| match msg {
                KiroStreamMessage::Text { text } => {
                    if !has_text_block {
                        let start_event = create_claude_stream_event(
                            "content_block_start",
                            json!({
                                "index": content_block_index,
                                "content_block": {"type": "text", "text": ""}
                            }),
                        );
                        let _ = tx.send(Ok(Bytes::from(format!(
                            "event: content_block_start\ndata: {}\n\n",
                            start_event
                        ))));
                        has_text_block = true;
                    }

                    let delta_event = create_claude_stream_event(
                        "content_block_delta",
                        json!({
                            "index": content_block_index,
                            "delta": {"type": "text_delta", "text": text}
                        }),
                    );
                    let _ = tx.send(Ok(Bytes::from(format!(
                        "event: content_block_delta\ndata: {}\n\n",
                        delta_event
                    ))));
                }
                KiroStreamMessage::Thinking { text } => {
                    let format = config.thinking_output_format.as_str();
                    let wrapped = if format == "thinking" {
                        format!("<thinking>{}</thinking>", text)
                    } else if format == "think" {
                        format!("<think>{}</think>", text)
                    } else {
                        String::new()
                    };

                    if !wrapped.is_empty() {
                        if !has_text_block {
                            let start_event = create_claude_stream_event(
                                "content_block_start",
                                json!({
                                    "index": content_block_index,
                                    "content_block": {"type": "text", "text": ""}
                                }),
                            );
                            let _ = tx.send(Ok(Bytes::from(format!(
                                "event: content_block_start\ndata: {}\n\n",
                                start_event
                            ))));
                            has_text_block = true;
                        }

                        let delta_event = create_claude_stream_event(
                            "content_block_delta",
                            json!({
                                "index": content_block_index,
                                "delta": {"type": "text_delta", "text": wrapped}
                            }),
                        );
                        let _ = tx.send(Ok(Bytes::from(format!(
                            "event: content_block_delta\ndata: {}\n\n",
                            delta_event
                        ))));
                    }
                }
                KiroStreamMessage::ToolUse { tool_use } => {
                    has_tool_calls = true;
                    if has_text_block {
                        let stop_event = create_claude_stream_event(
                            "content_block_stop",
                            json!({ "index": content_block_index }),
                        );
                        let _ = tx.send(Ok(Bytes::from(format!(
                            "event: content_block_stop\ndata: {}\n\n",
                            stop_event
                        ))));
                        content_block_index += 1;
                        has_text_block = false;
                    }

                    let start_event = create_claude_stream_event(
                        "content_block_start",
                        json!({
                            "index": content_block_index,
                            "content_block": {
                                "type": "tool_use",
                                "id": tool_use.tool_use_id,
                                "name": tool_use.name,
                                "input": {}
                            }
                        }),
                    );
                    let _ = tx.send(Ok(Bytes::from(format!(
                        "event: content_block_start\ndata: {}\n\n",
                        start_event
                    ))));

                    let delta_event = create_claude_stream_event(
                        "content_block_delta",
                        json!({
                            "index": content_block_index,
                            "delta": {
                                "type": "input_json_delta",
                                "partial_json": tool_use.input.to_string(),
                            }
                        }),
                    );
                    let _ = tx.send(Ok(Bytes::from(format!(
                        "event: content_block_delta\ndata: {}\n\n",
                        delta_event
                    ))));

                    let stop_event = create_claude_stream_event(
                        "content_block_stop",
                        json!({ "index": content_block_index }),
                    );
                    let _ = tx.send(Ok(Bytes::from(format!(
                        "event: content_block_stop\ndata: {}\n\n",
                        stop_event
                    ))));
                    content_block_index += 1;
                }
            },
        )
        .await;

        match call_result {
            Ok(usage) => {
                input_tokens = usage.input_tokens;
                output_tokens = usage.output_tokens;
                credits = usage.credits;
                service.mark_account_success(&account_id).await;

                if has_text_block {
                    let stop_event = create_claude_stream_event(
                        "content_block_stop",
                        json!({ "index": content_block_index }),
                    );
                    let _ = tx.send(Ok(Bytes::from(format!(
                        "event: content_block_stop\ndata: {}\n\n",
                        stop_event
                    ))));
                }

                let stop_reason = if has_tool_calls { "tool_use" } else { "end_turn" };
                let delta_event = create_claude_stream_event(
                    "message_delta",
                    json!({
                        "delta": {"stop_reason": stop_reason, "stop_sequence": Value::Null},
                        "usage": {"input_tokens": input_tokens, "output_tokens": output_tokens}
                    }),
                );
                let _ = tx.send(Ok(Bytes::from(format!(
                    "event: message_delta\ndata: {}\n\n",
                    delta_event
                ))));

                let stop_event = create_claude_stream_event("message_stop", json!({}));
                let _ = tx.send(Ok(Bytes::from(format!(
                    "event: message_stop\ndata: {}\n\n",
                    stop_event
                ))));

                let _ = service
                    .apply_api_key_usage(
                        api_key_id.as_deref(),
                        credits,
                        input_tokens,
                        output_tokens,
                        Some(model.as_str()),
                        request_path.as_str(),
                    )
                    .await;

                let log = ProxyRequestLog {
                    timestamp: chrono::Utc::now().timestamp(),
                    path: request_path.clone(),
                    method: "POST".to_string(),
                    model: Some(model.clone()),
                    account_id: Some(account_id.clone()),
                    account_email: Some(account_email.clone()),
                    api_key_id: api_key_id.clone(),
                    input_tokens,
                    output_tokens,
                    credits,
                    response_time_ms: started_at.elapsed().as_millis() as u64,
                    status: 200,
                    success: true,
                    error: None,
                };
                let _ = service
                    .record_request_log(
                        log,
                        Some(json!({
                            "path": request_path,
                            "status": 200,
                            "model": model,
                            "inputTokens": input_tokens,
                            "outputTokens": output_tokens,
                            "credits": credits,
                        })),
                    )
                    .await;
            }
            Err(err) => {
                let status = parse_upstream_status(&err).unwrap_or(500);
                service
                    .mark_account_error(&account_id, status == 429)
                    .await;

                let error_event = create_claude_stream_event(
                    "error",
                    json!({ "error": { "type": "api_error", "message": err } }),
                );
                let _ = tx.send(Ok(Bytes::from(format!(
                    "event: error\ndata: {}\n\n",
                    error_event
                ))));

                let log = ProxyRequestLog {
                    timestamp: chrono::Utc::now().timestamp(),
                    path: request_path.clone(),
                    method: "POST".to_string(),
                    model: Some(model.clone()),
                    account_id: Some(account_id.clone()),
                    account_email: Some(account_email.clone()),
                    api_key_id: api_key_id.clone(),
                    input_tokens,
                    output_tokens,
                    credits,
                    response_time_ms: started_at.elapsed().as_millis() as u64,
                    status,
                    success: false,
                    error: Some(err.clone()),
                };
                let _ = service
                    .record_request_log(
                        log,
                        Some(json!({
                            "path": request_path,
                            "status": status,
                            "model": model,
                            "error": err,
                        })),
                    )
                    .await;
            }
        }
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

async fn handle_count_tokens(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Err(resp) = verify_auth(&state, &headers, "/v1/messages/count_tokens").await {
        return resp;
    }

    let mut chars = 0usize;
    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        for msg in messages {
            if let Some(content) = msg.get("content") {
                chars += match content {
                    Value::String(raw) => raw.chars().count(),
                    Value::Array(parts) => parts
                        .iter()
                        .map(|part| {
                            part.get("text")
                                .and_then(|v| v.as_str())
                                .map(|v| v.chars().count())
                                .unwrap_or(0)
                        })
                        .sum(),
                    other => other.to_string().chars().count(),
                };
            }
        }
    }

    if let Some(system) = body.get("system") {
        chars += system.to_string().chars().count();
    }

    let estimated = ((chars as f64) / 4.0).ceil() as u64;
    Json(json!({ "input_tokens": estimated })).into_response()
}

async fn handle_event_logging_batch(
    State(state): State<AppState>,
    headers: HeaderMap,
    _: Json<Value>,
) -> Response {
    if let Err(resp) = verify_auth(&state, &headers, "/api/event_logging/batch").await {
        return resp;
    }

    Json(json!({ "status": "ok" })).into_response()
}

async fn handle_admin_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = verify_auth(&state, &headers, "/admin/stats").await {
        return resp;
    }

    Json(state.service.get_stats().await).into_response()
}

async fn handle_admin_accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = verify_auth(&state, &headers, "/admin/accounts").await {
        return resp;
    }

    let accounts = state.service.get_accounts().await;
    Json(json!({
        "total": accounts.len(),
        "available": accounts
            .iter()
            .filter(|item| item.cooldown_until.map(|v| v <= chrono::Utc::now().timestamp()).unwrap_or(true))
            .count(),
        "accounts": accounts,
    }))
    .into_response()
}

async fn handle_admin_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LogsQuery>,
) -> Response {
    if let Err(resp) = verify_auth(&state, &headers, "/admin/logs").await {
        return resp;
    }

    Json(state.service.get_logs(query.limit).await).into_response()
}

async fn handle_admin_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = verify_auth(&state, &headers, "/admin/config").await {
        return resp;
    }

    Json(state.service.get_config().await).into_response()
}

async fn handle_admin_update_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(config): Json<crate::modules::config::KiroProxyConfig>,
) -> Response {
    if let Err(resp) = verify_auth(&state, &headers, "/admin/config").await {
        return resp;
    }

    match state.service.update_config_without_restart(config).await {
        Ok(config) => Json(json!({ "success": true, "config": config })).into_response(),
        Err(err) => json_error(StatusCode::BAD_REQUEST, err),
    }
}
