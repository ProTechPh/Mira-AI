use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::Value;

use super::types::{KiroStreamMessage, KiroStreamUsage, KiroToolUse};

#[derive(Debug, Clone)]
struct ToolUseState {
    tool_use_id: String,
    name: String,
    input_buffer: String,
}

fn read_u32_be(slice: &[u8]) -> Option<u32> {
    if slice.len() < 4 {
        return None;
    }
    Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn extract_event_type(headers: &[u8]) -> String {
    let mut offset = 0usize;
    while offset < headers.len() {
        let name_len = *headers.get(offset).unwrap_or(&0) as usize;
        offset += 1;
        if offset + name_len > headers.len() {
            break;
        }

        let name = std::str::from_utf8(&headers[offset..offset + name_len]).unwrap_or("");
        offset += name_len;

        let value_type = *headers.get(offset).unwrap_or(&0);
        offset += 1;

        if value_type == 7 {
            if offset + 2 > headers.len() {
                break;
            }
            let value_len = u16::from_be_bytes([headers[offset], headers[offset + 1]]) as usize;
            offset += 2;
            if offset + value_len > headers.len() {
                break;
            }
            let value = std::str::from_utf8(&headers[offset..offset + value_len]).unwrap_or("");
            offset += value_len;
            if name == ":event-type" {
                return value.to_string();
            }
            continue;
        }

        let skip = match value_type {
            0 | 1 => 0,
            2 => 1,
            3 => 2,
            4 => 4,
            5 => 8,
            8 => 8,
            9 => 16,
            6 => {
                if offset + 2 > headers.len() {
                    break;
                }
                let len = u16::from_be_bytes([headers[offset], headers[offset + 1]]) as usize;
                offset += 2;
                len
            }
            _ => break,
        };
        offset += skip;
    }

    String::new()
}

fn as_u64(value: Option<&Value>) -> Option<u64> {
    value
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_i64().map(|n| n.max(0) as u64))
                .or_else(|| v.as_f64().map(|f| f.max(0.0).round() as u64))
                .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
        })
}

fn as_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(|v| {
        v.as_f64()
            .or_else(|| v.as_i64().map(|n| n as f64))
            .or_else(|| v.as_u64().map(|n| n as f64))
            .or_else(|| v.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
    })
}

fn flush_tool_use(state: ToolUseState) -> Option<KiroToolUse> {
    let input = if state.input_buffer.trim().is_empty() {
        Value::Object(serde_json::Map::new())
    } else {
        serde_json::from_str::<Value>(&state.input_buffer).unwrap_or_else(|_| {
            serde_json::json!({
                "_error": "tool input parse failed",
                "_partialInput": state.input_buffer,
            })
        })
    };

    Some(KiroToolUse {
        tool_use_id: state.tool_use_id,
        name: state.name,
        input,
    })
}

pub async fn parse_aws_event_stream(
    response: reqwest::Response,
    input_chars: usize,
    mut on_message: impl FnMut(KiroStreamMessage),
) -> Result<KiroStreamUsage, String> {
    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();
    let mut usage = KiroStreamUsage {
        input_tokens: (input_chars as u64 / 3).max(1),
        ..KiroStreamUsage::default()
    };
    let mut output_chars: usize = 0;
    let mut current_tool: Option<ToolUseState> = None;
    let mut processed_tool_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk: Bytes = chunk_result.map_err(|e| format!("读取上游流失败: {}", e))?;
        buffer.extend_from_slice(&chunk);

        loop {
            if buffer.len() < 16 {
                break;
            }

            let total_length = read_u32_be(&buffer[0..4]).ok_or("解析 EventStream totalLength 失败")?;
            let total_length = total_length as usize;

            if total_length == 0 || total_length > 8 * 1024 * 1024 {
                return Err("上游 EventStream 消息异常".to_string());
            }

            if buffer.len() < total_length {
                break;
            }

            let headers_length =
                read_u32_be(&buffer[4..8]).ok_or("解析 EventStream headersLength 失败")? as usize;
            let headers_start = 12usize;
            let headers_end = headers_start + headers_length;
            if headers_end > total_length {
                return Err("上游 EventStream headers 越界".to_string());
            }

            let payload_start = headers_end;
            let payload_end = total_length.saturating_sub(4);
            let event_type = extract_event_type(&buffer[headers_start..headers_end]);

            if payload_start < payload_end {
                let payload = &buffer[payload_start..payload_end];
                if let Ok(text) = std::str::from_utf8(payload) {
                    if let Ok(event_json) = serde_json::from_str::<Value>(text) {
                        if let Some(message) = event_json
                            .get("assistantResponseEvent")
                            .and_then(|v| v.get("content"))
                            .and_then(|v| v.as_str())
                            .or_else(|| {
                                if event_type == "assistantResponseEvent" {
                                    event_json.get("content").and_then(|v| v.as_str())
                                } else {
                                    None
                                }
                            })
                        {
                            output_chars += message.chars().count();
                            on_message(KiroStreamMessage::Text {
                                text: message.to_string(),
                            });
                        }

                        if let Some(reasoning_text) = event_json
                            .get("reasoningContentEvent")
                            .and_then(|v| v.get("text"))
                            .and_then(|v| v.as_str())
                            .or_else(|| {
                                if event_type == "reasoningContentEvent" {
                                    event_json.get("text").and_then(|v| v.as_str())
                                } else {
                                    None
                                }
                            })
                        {
                            output_chars += reasoning_text.chars().count();
                            usage.reasoning_tokens = usage
                                .reasoning_tokens
                                .saturating_add((reasoning_text.chars().count() as u64 / 3).max(1));
                            on_message(KiroStreamMessage::Thinking {
                                text: reasoning_text.to_string(),
                            });
                        }

                        if event_type == "toolUseEvent" || event_json.get("toolUseEvent").is_some() {
                            let tool_data = event_json
                                .get("toolUseEvent")
                                .cloned()
                                .unwrap_or_else(|| event_json.clone());

                            let tool_use_id = tool_data
                                .get("toolUseId")
                                .and_then(|v| v.as_str())
                                .map(|v| v.to_string());
                            let tool_name = tool_data
                                .get("name")
                                .and_then(|v| v.as_str())
                                .map(|v| v.to_string());
                            let stop = tool_data.get("stop").and_then(|v| v.as_bool()) == Some(true);

                            if let (Some(tool_use_id), Some(tool_name)) =
                                (tool_use_id.clone(), tool_name.clone())
                            {
                                if let Some(active) = &current_tool {
                                    if active.tool_use_id != tool_use_id
                                        && !processed_tool_ids.contains(&active.tool_use_id)
                                    {
                                        if let Some(tool_use) = flush_tool_use(active.clone()) {
                                            processed_tool_ids.insert(tool_use.tool_use_id.clone());
                                            on_message(KiroStreamMessage::ToolUse { tool_use });
                                        }
                                        current_tool = None;
                                    }
                                }

                                if current_tool.is_none() && !processed_tool_ids.contains(&tool_use_id) {
                                    current_tool = Some(ToolUseState {
                                        tool_use_id,
                                        name: tool_name,
                                        input_buffer: String::new(),
                                    });
                                }
                            }

                            if let Some(active) = current_tool.as_mut() {
                                if let Some(input) = tool_data.get("input") {
                                    if let Some(fragment) = input.as_str() {
                                        active.input_buffer.push_str(fragment);
                                    } else if input.is_object() || input.is_array() {
                                        active.input_buffer = input.to_string();
                                    }
                                }
                            }

                            if stop {
                                if let Some(active) = current_tool.take() {
                                    if !processed_tool_ids.contains(&active.tool_use_id) {
                                        if let Some(tool_use) = flush_tool_use(active) {
                                            processed_tool_ids.insert(tool_use.tool_use_id.clone());
                                            on_message(KiroStreamMessage::ToolUse { tool_use });
                                        }
                                    }
                                }
                            }
                        }

                        if event_type == "messageMetadataEvent"
                            || event_type == "metadataEvent"
                            || event_json.get("messageMetadataEvent").is_some()
                            || event_json.get("metadataEvent").is_some()
                        {
                            let metadata = event_json
                                .get("messageMetadataEvent")
                                .or_else(|| event_json.get("metadataEvent"))
                                .unwrap_or(&event_json);

                            if let Some(token_usage) = metadata.get("tokenUsage") {
                                let uncached = as_u64(token_usage.get("uncachedInputTokens")).unwrap_or(0);
                                let cache_read =
                                    as_u64(token_usage.get("cacheReadInputTokens")).unwrap_or(0);
                                let cache_write =
                                    as_u64(token_usage.get("cacheWriteInputTokens")).unwrap_or(0);
                                let input_total = uncached.saturating_add(cache_read).saturating_add(cache_write);
                                if input_total > 0 {
                                    usage.input_tokens = input_total;
                                }
                                if let Some(output) = as_u64(token_usage.get("outputTokens")) {
                                    usage.output_tokens = output;
                                }
                                usage.cache_read_tokens = cache_read;
                                usage.cache_write_tokens = cache_write;
                            }

                            if let Some(input) = as_u64(metadata.get("inputTokens")) {
                                usage.input_tokens = input;
                            }
                            if let Some(output) = as_u64(metadata.get("outputTokens")) {
                                usage.output_tokens = output;
                            }
                        }

                        if event_type == "usageEvent"
                            || event_json.get("usageEvent").is_some()
                            || event_json.get("usage").is_some()
                        {
                            let usage_json = event_json
                                .get("usageEvent")
                                .or_else(|| event_json.get("usage"))
                                .unwrap_or(&event_json);

                            if let Some(input) = as_u64(usage_json.get("inputTokens")) {
                                usage.input_tokens = input;
                            }
                            if let Some(output) = as_u64(usage_json.get("outputTokens")) {
                                usage.output_tokens = output;
                            }
                        }

                        if event_type == "meteringEvent" || event_json.get("meteringEvent").is_some() {
                            let metering = event_json
                                .get("meteringEvent")
                                .unwrap_or(&event_json);
                            if let Some(credits) = as_f64(metering.get("usage")) {
                                usage.credits += credits;
                            }
                        }

                        if event_json.get("_type").is_some() || event_json.get("error").is_some() {
                            let error_message = event_json
                                .get("message")
                                .and_then(|v| v.as_str())
                                .or_else(|| {
                                    event_json
                                        .get("error")
                                        .and_then(|v| v.get("message"))
                                        .and_then(|v| v.as_str())
                                })
                                .unwrap_or("上游流返回错误");
                            return Err(error_message.to_string());
                        }
                    }
                }
            }

            buffer.drain(0..total_length);
        }
    }

    if let Some(active) = current_tool.take() {
        if !processed_tool_ids.contains(&active.tool_use_id) {
            if let Some(tool_use) = flush_tool_use(active) {
                on_message(KiroStreamMessage::ToolUse { tool_use });
            }
        }
    }

    if usage.output_tokens == 0 && output_chars > 0 {
        usage.output_tokens = (output_chars as u64 / 3).max(1);
    }

    Ok(usage)
}