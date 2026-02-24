use serde_json::{json, Value};

use super::kiro_api::map_model_id;
use super::types::{
    ClaudeMessage, ClaudeRequest, KiroStreamUsage, KiroToolUse, OpenAiChatRequest, OpenAiMessage,
};

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn parse_data_url_image(url: &str) -> Option<Value> {
    if !url.starts_with("data:") {
        return None;
    }

    let marker = ";base64,";
    let split_at = url.find(marker)?;
    let meta = &url[5..split_at];
    let data = &url[split_at + marker.len()..];

    let format = if let Some((mime, _)) = meta.split_once(';') {
        mime.rsplit('/').next().unwrap_or("png")
    } else {
        meta.rsplit('/').next().unwrap_or("png")
    };

    if data.trim().is_empty() {
        return None;
    }

    Some(json!({
        "format": format,
        "source": { "bytes": data },
    }))
}

fn extract_openai_content(message: &OpenAiMessage) -> (String, Vec<Value>) {
    let mut text = String::new();
    let mut images = Vec::new();

    match &message.content {
        Value::String(raw) => {
            text.push_str(raw);
        }
        Value::Array(parts) => {
            for part in parts {
                if let Some(kind) = part.get("type").and_then(|v| v.as_str()) {
                    match kind {
                        "text" => {
                            if let Some(content) = part.get("text").and_then(|v| v.as_str()) {
                                text.push_str(content);
                            }
                        }
                        "image_url" => {
                            if let Some(url) = part
                                .get("image_url")
                                .and_then(|v| v.get("url"))
                                .and_then(|v| v.as_str())
                            {
                                if let Some(image) = parse_data_url_image(url) {
                                    images.push(image);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {
            text.push_str(&message.content.to_string());
        }
    }

    (text, images)
}

fn convert_openai_tools(request: &OpenAiChatRequest, disable_tools: bool) -> Vec<Value> {
    if disable_tools {
        return Vec::new();
    }

    request
        .tools
        .as_ref()
        .map(|tools| {
            tools
                .iter()
                .filter(|tool| tool.tool_type == "function")
                .map(|tool| {
                    json!({
                        "toolSpecification": {
                            "name": tool.function.name,
                            "description": tool.function.description.clone().unwrap_or_default(),
                            "inputSchema": { "json": tool.function.parameters.clone() }
                        }
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn sanitize_history(mut history: Vec<Value>) -> Vec<Value> {
    if history.is_empty() {
        return history;
    }

    if history
        .first()
        .and_then(|item| item.get("userInputMessage"))
        .is_none()
    {
        history.insert(
            0,
            json!({
                "userInputMessage": {
                    "content": "Hello",
                    "origin": "AI_EDITOR"
                }
            }),
        );
    }

    history
}

pub fn openai_to_kiro(
    request: &OpenAiChatRequest,
    profile_arn: Option<String>,
    disable_tools: bool,
) -> Value {
    let model_id = map_model_id(&request.model);
    let origin = "AI_EDITOR";

    let mut history: Vec<Value> = Vec::new();
    let mut tool_results_for_current: Vec<Value> = Vec::new();
    let mut current_content = String::new();
    let mut current_images: Vec<Value> = Vec::new();

    let mut system_prompt = String::new();
    for message in &request.messages {
        if message.role == "system" {
            let (text, _) = extract_openai_content(message);
            if !text.trim().is_empty() {
                if !system_prompt.is_empty() {
                    system_prompt.push('\n');
                }
                system_prompt.push_str(text.trim());
            }
        }
    }

    for (idx, message) in request
        .messages
        .iter()
        .filter(|msg| msg.role != "system")
        .enumerate()
    {
        let is_last = idx + 1
            == request
                .messages
                .iter()
                .filter(|msg| msg.role != "system")
                .count();

        match message.role.as_str() {
            "user" => {
                let (text, images) = extract_openai_content(message);
                let merged = if current_content.is_empty() {
                    text
                } else {
                    format!("{}\n{}", current_content, text)
                };

                if is_last {
                    current_content = merged;
                    current_images = images;
                } else {
                    history.push(json!({
                        "userInputMessage": {
                            "content": if merged.trim().is_empty() { "Continue" } else { merged.trim() },
                            "modelId": model_id,
                            "origin": origin,
                            "images": if images.is_empty() { Value::Null } else { Value::Array(images) }
                        }
                    }));
                    current_content.clear();
                }
            }
            "assistant" => {
                let (text, _) = extract_openai_content(message);
                let tool_uses = message
                    .tool_calls
                    .as_ref()
                    .map(|calls| {
                        calls
                            .iter()
                            .map(|call| {
                                let parsed_input: Value = serde_json::from_str(&call.function.arguments)
                                    .unwrap_or_else(|_| {
                                        json!({
                                            "_raw": call.function.arguments,
                                        })
                                    });
                                json!({
                                    "toolUseId": call.id,
                                    "name": call.function.name,
                                    "input": parsed_input,
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                history.push(json!({
                    "assistantResponseMessage": {
                        "content": if text.trim().is_empty() { "I understand." } else { text.trim() },
                        "toolUses": if tool_uses.is_empty() { Value::Null } else { Value::Array(tool_uses) }
                    }
                }));
            }
            "tool" => {
                let tool_result_text = match &message.content {
                    Value::String(v) => v.clone(),
                    other => other.to_string(),
                };
                if let Some(tool_use_id) = message.tool_call_id.clone() {
                    tool_results_for_current.push(json!({
                        "toolUseId": tool_use_id,
                        "status": "success",
                        "content": [{ "text": tool_result_text }]
                    }));
                }
                if is_last && current_content.trim().is_empty() {
                    current_content = "Tool results provided.".to_string();
                }
            }
            _ => {}
        }
    }

    let timestamp = chrono::Utc::now().to_rfc3339();
    let mut final_content = current_content.trim().to_string();
    if final_content.is_empty() {
        final_content = "Continue.".to_string();
    }

    if !system_prompt.trim().is_empty() {
        final_content = format!(
            "[Context: Current time is {}]\n\n{}\n\n{}",
            timestamp,
            system_prompt.trim(),
            final_content
        );
    }

    let mut user_context = serde_json::Map::new();
    let tools = convert_openai_tools(request, disable_tools);
    if !tools.is_empty() {
        user_context.insert("tools".to_string(), Value::Array(tools));
    }
    if !tool_results_for_current.is_empty() {
        user_context.insert(
            "toolResults".to_string(),
            Value::Array(tool_results_for_current),
        );
    }

    let mut current_message = serde_json::Map::new();
    current_message.insert("content".to_string(), Value::String(final_content));
    current_message.insert("modelId".to_string(), Value::String(model_id.clone()));
    current_message.insert("origin".to_string(), Value::String(origin.to_string()));
    if !current_images.is_empty() {
        current_message.insert("images".to_string(), Value::Array(current_images));
    }
    if !user_context.is_empty() {
        current_message.insert(
            "userInputMessageContext".to_string(),
            Value::Object(user_context),
        );
    }

    let history = sanitize_history(history);

    json!({
        "profileArn": profile_arn,
        "conversationState": {
            "chatTriggerType": "MANUAL",
            "conversationId": uuid::Uuid::new_v4().to_string(),
            "currentMessage": {
                "userInputMessage": Value::Object(current_message)
            },
            "history": if history.is_empty() { Value::Null } else { Value::Array(history) }
        },
        "inferenceConfig": {
            "maxTokens": request.max_tokens,
            "temperature": request.temperature,
            "topP": request.top_p,
        }
    })
}

fn extract_claude_text_and_aux(message: &ClaudeMessage) -> (String, Vec<Value>, Vec<Value>, Vec<Value>) {
    let mut text = String::new();
    let mut images: Vec<Value> = Vec::new();
    let mut tool_results: Vec<Value> = Vec::new();
    let mut tool_uses: Vec<Value> = Vec::new();

    match &message.content {
        Value::String(raw) => text.push_str(raw),
        Value::Array(blocks) => {
            for block in blocks {
                let kind = block.get("type").and_then(|v| v.as_str()).unwrap_or_default();
                match kind {
                    "text" => {
                        if let Some(value) = block.get("text").and_then(|v| v.as_str()) {
                            text.push_str(value);
                        }
                    }
                    "image" => {
                        if let Some(source) = block.get("source") {
                            let media_type = source
                                .get("media_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("image/png");
                            let format = media_type.rsplit('/').next().unwrap_or("png");
                            if let Some(bytes) = source.get("data").and_then(|v| v.as_str()) {
                                images.push(json!({
                                    "format": format,
                                    "source": { "bytes": bytes }
                                }));
                            }
                        }
                    }
                    "tool_result" => {
                        let tool_use_id = block
                            .get("tool_use_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("tool_result_unknown");
                        let content = block.get("content").cloned().unwrap_or(Value::String(String::new()));
                        let content_text = match content {
                            Value::String(raw) => raw,
                            Value::Array(items) => items
                                .iter()
                                .filter_map(|item| item.get("text").and_then(|v| v.as_str()))
                                .collect::<Vec<_>>()
                                .join("\n"),
                            other => other.to_string(),
                        };

                        tool_results.push(json!({
                            "toolUseId": tool_use_id,
                            "status": "success",
                            "content": [{ "text": content_text }]
                        }));
                    }
                    "tool_use" => {
                        let tool_use_id = block
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("tool_use_unknown");
                        let name = block
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("tool");
                        let input = block.get("input").cloned().unwrap_or_else(|| json!({}));
                        tool_uses.push(json!({
                            "toolUseId": tool_use_id,
                            "name": name,
                            "input": input,
                        }));
                    }
                    "thinking" => {
                        if let Some(value) = block
                            .get("thinking")
                            .or_else(|| block.get("text"))
                            .and_then(|v| v.as_str())
                        {
                            text.push_str(value);
                        }
                    }
                    _ => {}
                }
            }
        }
        other => text.push_str(&other.to_string()),
    }

    (text, images, tool_results, tool_uses)
}

pub fn claude_to_kiro(request: &ClaudeRequest, profile_arn: Option<String>) -> Value {
    let model_id = map_model_id(&request.model);
    let origin = "AI_EDITOR";

    let mut history: Vec<Value> = Vec::new();
    let mut current_content = String::new();
    let mut current_images = Vec::new();
    let mut current_tool_results = Vec::new();

    let mut system_text = String::new();
    if let Some(system) = &request.system {
        match system {
            Value::String(raw) => system_text.push_str(raw),
            Value::Array(blocks) => {
                for block in blocks {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        if !system_text.is_empty() {
                            system_text.push('\n');
                        }
                        system_text.push_str(text);
                    }
                }
            }
            other => system_text.push_str(&other.to_string()),
        }
    }

    for (idx, message) in request.messages.iter().enumerate() {
        let is_last = idx + 1 == request.messages.len();
        let (text, images, tool_results, tool_uses) = extract_claude_text_and_aux(message);

        match message.role.as_str() {
            "user" => {
                if is_last {
                    current_content = text;
                    current_images = images;
                    current_tool_results = tool_results;
                } else {
                    let mut user_message = json!({
                        "content": if text.trim().is_empty() { "Continue" } else { text.trim() },
                        "modelId": model_id,
                        "origin": origin,
                    });
                    if !images.is_empty() {
                        user_message["images"] = Value::Array(images);
                    }
                    if !tool_results.is_empty() {
                        user_message["userInputMessageContext"] = json!({
                            "toolResults": tool_results,
                        });
                    }
                    history.push(json!({ "userInputMessage": user_message }));
                }
            }
            "assistant" => {
                history.push(json!({
                    "assistantResponseMessage": {
                        "content": if text.trim().is_empty() { "I understand." } else { text.trim() },
                        "toolUses": if tool_uses.is_empty() { Value::Null } else { Value::Array(tool_uses) },
                    }
                }));
            }
            _ => {}
        }
    }

    if current_content.trim().is_empty() {
        current_content = if current_tool_results.is_empty() {
            "Continue.".to_string()
        } else {
            "Tool results provided.".to_string()
        };
    }

    if !system_text.trim().is_empty() {
        current_content = format!(
            "[Context: Current time is {}]\n\n{}\n\n{}",
            chrono::Utc::now().to_rfc3339(),
            system_text.trim(),
            current_content,
        );
    }

    let mut context = serde_json::Map::new();
    if let Some(tools) = &request.tools {
        let mapped = tools
            .iter()
            .map(|tool| {
                json!({
                    "toolSpecification": {
                        "name": tool.name,
                        "description": tool.description.clone().unwrap_or_default(),
                        "inputSchema": { "json": tool.input_schema.clone().unwrap_or_else(|| json!({})) }
                    }
                })
            })
            .collect::<Vec<_>>();
        if !mapped.is_empty() {
            context.insert("tools".to_string(), Value::Array(mapped));
        }
    }
    if !current_tool_results.is_empty() {
        context.insert("toolResults".to_string(), Value::Array(current_tool_results));
    }

    let mut current_user_input = json!({
        "content": current_content,
        "modelId": model_id,
        "origin": origin,
    });
    if !current_images.is_empty() {
        current_user_input["images"] = Value::Array(current_images);
    }
    if !context.is_empty() {
        current_user_input["userInputMessageContext"] = Value::Object(context);
    }

    json!({
        "profileArn": profile_arn,
        "conversationState": {
            "chatTriggerType": "MANUAL",
            "conversationId": uuid::Uuid::new_v4().to_string(),
            "currentMessage": {
                "userInputMessage": current_user_input
            },
            "history": if history.is_empty() { Value::Null } else { Value::Array(sanitize_history(history)) }
        },
        "inferenceConfig": {
            "maxTokens": request.max_tokens,
            "temperature": request.temperature,
            "topP": request.top_p,
        }
    })
}

pub fn kiro_to_openai_response(
    content: String,
    tool_uses: Vec<KiroToolUse>,
    usage: KiroStreamUsage,
    model: String,
) -> Value {
    let message = if tool_uses.is_empty() {
        json!({
            "role": "assistant",
            "content": content,
        })
    } else {
        let tool_calls = tool_uses
            .iter()
            .map(|tool| {
                json!({
                    "id": tool.tool_use_id,
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "arguments": tool.input.to_string(),
                    }
                })
            })
            .collect::<Vec<_>>();
        json!({
            "role": "assistant",
            "content": Value::Null,
            "tool_calls": tool_calls,
        })
    };

    json!({
        "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        "object": "chat.completion",
        "created": now_ts(),
        "model": model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": if tool_uses.is_empty() { "stop" } else { "tool_calls" },
        }],
        "usage": {
            "prompt_tokens": usage.input_tokens,
            "completion_tokens": usage.output_tokens,
            "total_tokens": usage.input_tokens + usage.output_tokens,
        }
    })
}

pub fn create_openai_stream_chunk(
    id: &str,
    model: &str,
    delta: Value,
    finish_reason: Option<&str>,
    usage: Option<KiroStreamUsage>,
) -> Value {
    let mut chunk = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": now_ts(),
        "model": model,
        "choices": [{
            "index": 0,
            "delta": delta,
            "finish_reason": finish_reason,
        }]
    });

    if let Some(usage) = usage {
        chunk["usage"] = json!({
            "prompt_tokens": usage.input_tokens,
            "completion_tokens": usage.output_tokens,
            "total_tokens": usage.input_tokens + usage.output_tokens,
            "prompt_tokens_details": {
                "cached_tokens": usage.cache_read_tokens,
            },
            "completion_tokens_details": {
                "reasoning_tokens": usage.reasoning_tokens,
            }
        });
    }

    chunk
}

pub fn kiro_to_claude_response(
    content: String,
    tool_uses: Vec<KiroToolUse>,
    usage: KiroStreamUsage,
    model: String,
) -> Value {
    let mut blocks = Vec::new();
    if !content.trim().is_empty() {
        blocks.push(json!({
            "type": "text",
            "text": content,
        }));
    }

    for tool in tool_uses {
        blocks.push(json!({
            "type": "tool_use",
            "id": tool.tool_use_id,
            "name": tool.name,
            "input": tool.input,
        }));
    }

    json!({
        "id": format!("msg_{}", uuid::Uuid::new_v4()),
        "type": "message",
        "role": "assistant",
        "content": blocks,
        "model": model,
        "stop_reason": if blocks.iter().any(|b| b.get("type").and_then(|v| v.as_str()) == Some("tool_use")) { "tool_use" } else { "end_turn" },
        "stop_sequence": Value::Null,
        "usage": {
            "input_tokens": usage.input_tokens,
            "output_tokens": usage.output_tokens,
        }
    })
}

pub fn create_claude_stream_event(event_type: &str, payload: Value) -> Value {
    let mut obj = match payload {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    obj.insert("type".to_string(), Value::String(event_type.to_string()));
    Value::Object(obj)
}
