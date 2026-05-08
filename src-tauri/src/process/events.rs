use serde::{Deserialize, Serialize};

// ── Normalized events emitted to the frontend ──────────────────────────

/// Events sent to the frontend via Tauri event system.
/// These are the canonical events the UI consumes.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum NormalizedEvent {
    #[serde(rename = "session_init")]
    SessionInit {
        session_id: String,
        model: String,
        tools: Vec<String>,
    },
    #[serde(rename = "text_chunk")]
    TextChunk { text: String },
    #[serde(rename = "tool_call")]
    ToolCall {
        tool_name: String,
        tool_id: String,
    },
    #[serde(rename = "tool_call_update")]
    ToolCallUpdate {
        tool_id: String,
        partial_input: String,
    },
    #[serde(rename = "tool_call_complete")]
    ToolCallComplete { tool_id: String },
    #[serde(rename = "task_complete")]
    TaskComplete {
        cost_usd: f64,
        duration_ms: u64,
        num_turns: u64,
        session_id: String,
    },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "session_dead")]
    SessionDead {
        exit_code: Option<i32>,
        stderr_tail: Vec<String>,
    },
    #[serde(rename = "rate_limit")]
    RateLimit {
        status: String,
        resets_at: u64,
    },
}

/// Wrapper sent via Tauri events: includes the session_id for routing.
#[derive(Debug, Clone, Serialize)]
pub struct SessionEvent {
    pub session_id: String,
    pub event: NormalizedEvent,
}

// ── Raw Claude NDJSON event parsing ────────────────────────────────────

/// We use serde_json::Value for initial parsing, then extract what we need.
/// This avoids needing exhaustive struct definitions for every Claude event variant.

/// Normalize a raw NDJSON line into zero or more frontend events.
pub fn normalize_event(raw: &serde_json::Value) -> Vec<NormalizedEvent> {
    let event_type = raw.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match event_type {
        "system" => normalize_system(raw),
        "stream_event" => normalize_stream(raw),
        "result" => normalize_result(raw),
        "rate_limit_event" => normalize_rate_limit(raw),
        _ => vec![],
    }
}

fn normalize_system(raw: &serde_json::Value) -> Vec<NormalizedEvent> {
    let subtype = raw.get("subtype").and_then(|s| s.as_str()).unwrap_or("");
    if subtype != "init" {
        return vec![];
    }

    let session_id = raw
        .get("session_id")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let model = raw
        .get("model")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown")
        .to_string();
    let tools = raw
        .get("tools")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    vec![NormalizedEvent::SessionInit {
        session_id,
        model,
        tools,
    }]
}

fn normalize_stream(raw: &serde_json::Value) -> Vec<NormalizedEvent> {
    let inner = match raw.get("event") {
        Some(e) => e,
        None => return vec![],
    };
    let inner_type = inner.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match inner_type {
        "content_block_start" => {
            let block = match inner.get("content_block") {
                Some(b) => b,
                None => return vec![],
            };
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if block_type == "tool_use" {
                let tool_name = block
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let tool_id = block
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("")
                    .to_string();
                vec![NormalizedEvent::ToolCall { tool_name, tool_id }]
            } else {
                vec![]
            }
        }
        "content_block_delta" => {
            let delta = match inner.get("delta") {
                Some(d) => d,
                None => return vec![],
            };
            let delta_type = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match delta_type {
                "text_delta" => {
                    let text = delta
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    if text.is_empty() {
                        vec![]
                    } else {
                        vec![NormalizedEvent::TextChunk { text }]
                    }
                }
                "input_json_delta" => {
                    let partial = delta
                        .get("partial_json")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_string();
                    // We need the tool_id from the parent — but it's in content_block_start.
                    // For now, use empty string; the frontend tracks by last active tool.
                    vec![NormalizedEvent::ToolCallUpdate {
                        tool_id: String::new(),
                        partial_input: partial,
                    }]
                }
                _ => vec![],
            }
        }
        "content_block_stop" => {
            // The frontend uses this to mark the current tool as complete.
            // We don't have tool_id here; frontend tracks by last active tool.
            vec![NormalizedEvent::ToolCallComplete {
                tool_id: String::new(),
            }]
        }
        _ => vec![],
    }
}

fn normalize_result(raw: &serde_json::Value) -> Vec<NormalizedEvent> {
    let is_error = raw.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false);

    if is_error {
        let message = raw
            .get("result")
            .and_then(|r| r.as_str())
            .unwrap_or("Unknown error")
            .to_string();
        return vec![NormalizedEvent::Error { message }];
    }

    let cost_usd = raw
        .get("total_cost_usd")
        .and_then(|c| c.as_f64())
        .unwrap_or(0.0);
    let duration_ms = raw
        .get("duration_ms")
        .and_then(|d| d.as_u64())
        .unwrap_or(0);
    let num_turns = raw
        .get("num_turns")
        .and_then(|n| n.as_u64())
        .unwrap_or(0);
    let session_id = raw
        .get("session_id")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    vec![NormalizedEvent::TaskComplete {
        cost_usd,
        duration_ms,
        num_turns,
        session_id,
    }]
}

fn normalize_rate_limit(raw: &serde_json::Value) -> Vec<NormalizedEvent> {
    let info = match raw.get("rate_limit_info") {
        Some(i) => i,
        None => return vec![],
    };
    let status = info
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let resets_at = info
        .get("resetsAt")
        .and_then(|r| r.as_u64())
        .unwrap_or(0);

    vec![NormalizedEvent::RateLimit { status, resets_at }]
}
