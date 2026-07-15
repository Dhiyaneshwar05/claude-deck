use serde::{Deserialize, Serialize};

/// Raw session file from ~/.claude/sessions/{pid}.json
#[derive(Debug, Deserialize)]
pub struct SessionFile {
    pub pid: u32,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub cwd: String,
    #[serde(rename = "startedAt")]
    pub started_at: u64,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub entrypoint: String,
}

/// A history entry from ~/.claude/history.jsonl (fallback title source)
#[derive(Debug, Deserialize)]
pub struct HistoryEntry {
    pub display: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(default)]
    pub timestamp: u64,
}

/// An ai-title record from ~/.claude/projects/*/{session_id}.jsonl
#[derive(Debug, Deserialize)]
pub struct AiTitleRecord {
    #[serde(rename = "type")]
    pub record_type: String,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "aiTitle")]
    pub ai_title: String,
}

/// Enriched session data sent to the frontend
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredSession {
    pub pid: u32,
    pub session_id: String,
    pub cwd: String,
    pub project_name: String,
    pub started_at: u64,
    pub entrypoint: String,
    pub is_alive: bool,
    pub uptime_secs: u64,
    /// First user prompt as a human-readable title
    pub title: String,
    /// Git branch from the session's conversation
    pub git_branch: Option<String>,
    /// Claude model used (e.g. "claude-opus-4-6")
    pub model: Option<String>,
    /// Total number of conversation turns (user + assistant messages)
    pub message_count: u32,
    /// Total input tokens consumed
    pub total_input_tokens: u64,
    /// Total output tokens consumed
    pub total_output_tokens: u64,
    /// Total cache read tokens
    pub total_cache_read_tokens: u64,
    /// Total cache-creation tokens (expensive; separate from cache_read for cost math)
    pub total_cache_creation_tokens: u64,
    /// Most recent user prompt (from `last-prompt` records)
    pub last_prompt: Option<String>,
    /// Count of tool calls that returned `is_error: true`
    pub failed_tool_count: u32,
    /// Launch origin from the transcript (e.g. "claude-vscode", "cli", "claude-code")
    pub transcript_entrypoint: Option<String>,
    /// Claude CLI version recorded in the transcript (e.g. "2.1.138")
    pub cli_version: Option<String>,
}

/// Enriched metadata extracted from a session's JSONL conversation file
#[derive(Debug, Default)]
pub struct SessionMetadata {
    pub title: Option<String>,
    pub git_branch: Option<String>,
    pub model: Option<String>,
    pub message_count: u32,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub last_prompt: Option<String>,
    pub failed_tool_count: u32,
    pub transcript_entrypoint: Option<String>,
    pub cli_version: Option<String>,
}
