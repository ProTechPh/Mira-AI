use serde_json::{json, Value};

use super::types::{OpenAiChatRequest, OpenAiUsage};

fn content_to_text(content: &Value) -> Result<String, String> {
    match content {
        Value::String(text) => Ok(text.clone()),
        Value::Array(parts) => {
            let mut merged = String::new();
            for part in parts {
                let obj = part
                    .as_object()
                    .ok_or_else(|| "message content array item must be object".to_string())?;
                let part_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if part_type != "text" {
                    return Err("Only text content parts are supported in this proxy".to_string());
                }
                let text = obj
                    .get("text")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "text part missing string field `text`".to_string())?;
                merged.push_str(text);
            }
            Ok(merged)
        }
        _ => Err("message content must be string or text-part array".to_string()),
    }
}

fn sanitize_project_id(project_id: Option<&str>) -> Option<String> {
    let raw = project_id?.trim();
    if raw.is_empty() || raw == "projects" || raw == "projects/" {
        return None;
    }
    if raw.starts_with("projects/") && raw.ends_with('/') {
        return None;
    }
    Some(raw.to_string())
}

pub fn to_cloud_code_payload(
    request: &OpenAiChatRequest,
    project_id: Option<&str>,
    session_id: Option<&str>,
) -> Result<Value, String> {
    if request.messages.is_empty() {
        return Err("messages is required".to_string());
    }

    let mut contents: Vec<Value> = Vec::new();
    for message in &request.messages {
        let role = match message.role.as_str() {
            "user" | "system" => "user",
            "assistant" => "model",
            _ => return Err(format!("unsupported message role: {}", message.role)),
        };

        let text = content_to_text(&message.content)?;
        contents.push(json!({
            "role": role,
            "parts": [{ "text": text }]
        }));
    }

    let mut generation_config = json!({});
    if let Some(v) = request.temperature {
        generation_config["temperature"] = json!(v);
    }
    if let Some(v) = request.top_p {
        generation_config["topP"] = json!(v);
    }
    if let Some(v) = request.max_tokens {
        generation_config["maxOutputTokens"] = json!(v);
    }

    let mut body = json!({
        "requestId": format!("req_{}", uuid::Uuid::new_v4().simple()),
        "model": request.model,
        "userAgent": "antigravity",
        "requestType": "agent",
        "request": {
            "contents": contents,
            "generationConfig": generation_config,
            "sessionId": session_id.unwrap_or("desktop-proxy"),
        }
    });

    if let Some(project_id) = sanitize_project_id(project_id) {
        body["project"] = json!(project_id);
    }

    Ok(body)
}

pub fn parse_usage(usage: Option<&Value>) -> OpenAiUsage {
    OpenAiUsage {
        prompt_tokens: usage
            .and_then(|u| u.get("promptTokenCount"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        completion_tokens: usage
            .and_then(|u| u.get("candidatesTokenCount"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        total_tokens: usage
            .and_then(|u| u.get("totalTokenCount"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
    }
}

pub fn parse_content_from_response(payload: &Value) -> String {
    let parts = payload
        .get("response")
        .and_then(|v| v.get("candidates"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("content"))
        .and_then(|v| v.get("parts"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut content = String::new();
    for part in &parts {
        if part.get("thought").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }
        if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
            content.push_str(text);
        }
    }
    content
}

pub fn create_openai_chunk(
    id: &str,
    model: &str,
    delta: Value,
    finish_reason: Option<&str>,
    usage: Option<OpenAiUsage>,
) -> Value {
    let mut chunk = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
        "choices": [{
            "index": 0,
            "delta": delta,
            "finish_reason": finish_reason
        }]
    });

    if let Some(usage) = usage {
        chunk["usage"] = json!(usage);
    }

    chunk
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reject_non_text_array_part() {
        let request = OpenAiChatRequest {
            model: "gemini-2.5-pro".to_string(),
            messages: vec![super::super::types::OpenAiMessage {
                role: "user".to_string(),
                content: json!([{"type":"image_url","image_url":{"url":"x"}}]),
            }],
            temperature: None,
            top_p: None,
            max_tokens: None,
            stream: None,
        };

        let err = to_cloud_code_payload(&request, None, None).unwrap_err();
        assert!(err.contains("Only text"));
    }
}
