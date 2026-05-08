use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::types::{AiTitleRecord, DiscoveredSession, HistoryEntry, SessionFile, SessionMetadata};

/// Get the path to ~/.claude/sessions/
fn sessions_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("sessions"))
}

/// Scan ~/.claude/projects/*/*.jsonl for session metadata:
/// ai-title, git branch, model, message counts, token usage.
/// Falls back to ~/.claude/history.jsonl for titles.
fn load_session_metadata() -> HashMap<String, SessionMetadata> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return HashMap::new(),
    };

    let mut metadata: HashMap<String, SessionMetadata> = HashMap::new();

    // Primary source: JSONL conversation files in project directories
    let projects_dir = home.join(".claude").join("projects");
    if let Ok(project_entries) = fs::read_dir(&projects_dir) {
        for project_entry in project_entries.flatten() {
            let project_path = project_entry.path();
            if !project_path.is_dir() {
                continue;
            }
            if let Ok(jsonl_files) = fs::read_dir(&project_path) {
                for jsonl_entry in jsonl_files.flatten() {
                    let path = jsonl_entry.path();
                    if path.extension().map_or(true, |e| e != "jsonl") {
                        continue;
                    }
                    let session_id = match path.file_stem().and_then(|s| s.to_str()) {
                        Some(s) => s.to_string(),
                        None => continue,
                    };
                    if metadata.contains_key(&session_id) {
                        continue;
                    }
                    if let Some(meta) = extract_metadata_from_jsonl(&path) {
                        metadata.insert(session_id, meta);
                    }
                }
            }
        }
    }

    // Fallback: history.jsonl first prompt for sessions without a title
    let history_path = home.join(".claude").join("history.jsonl");
    if let Ok(file) = fs::File::open(&history_path) {
        let reader = BufReader::new(file);
        for line in reader.lines().map_while(Result::ok) {
            if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
                let meta = metadata.entry(entry.session_id).or_default();
                if meta.title.is_none() {
                    let display = entry.display.trim().to_string();
                    let title = if display.len() > 60 {
                        format!("{}...", &display[..57])
                    } else {
                        display
                    };
                    meta.title = Some(title);
                }
            }
        }
    }

    metadata
}

/// Extract metadata from a single JSONL conversation file
fn extract_metadata_from_jsonl(path: &std::path::Path) -> Option<SessionMetadata> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut meta = SessionMetadata::default();

    for line in reader.lines().map_while(Result::ok) {
        // Fast path: check record type via string contains before parsing
        if line.contains("\"ai-title\"") {
            if let Ok(record) = serde_json::from_str::<AiTitleRecord>(&line) {
                if record.record_type == "ai-title" && !record.ai_title.is_empty() {
                    let title = record.ai_title.trim().to_string();
                    meta.title = Some(if title.len() > 60 {
                        format!("{}...", &title[..57])
                    } else {
                        title
                    });
                }
            }
            continue;
        }

        // Parse user/assistant messages for stats
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
            match val.get("type").and_then(|t| t.as_str()) {
                Some("user") => {
                    meta.message_count += 1;
                    if meta.git_branch.is_none() {
                        if let Some(branch) = val.get("gitBranch").and_then(|b| b.as_str()) {
                            if !branch.is_empty() {
                                meta.git_branch = Some(branch.to_string());
                            }
                        }
                    }
                }
                Some("assistant") => {
                    meta.message_count += 1;
                    if let Some(msg) = val.get("message") {
                        if meta.model.is_none() {
                            if let Some(model) = msg.get("model").and_then(|m| m.as_str()) {
                                meta.model = Some(model.to_string());
                            }
                        }
                        if let Some(usage) = msg.get("usage") {
                            meta.total_input_tokens += usage
                                .get("input_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            meta.total_output_tokens += usage
                                .get("output_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            meta.total_cache_read_tokens += usage
                                .get("cache_read_input_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    Some(meta)
}

/// Check if a process is alive by sending signal 0
fn is_process_alive(pid: u32) -> bool {
    // kill(pid, 0) returns 0 if process exists and we have permission to signal it
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Extract the last path segment as project name
fn project_name_from_cwd(cwd: &str) -> String {
    cwd.split('/')
        .filter(|s| !s.is_empty())
        .last()
        .unwrap_or("unknown")
        .to_string()
}

/// Current timestamp in milliseconds
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Scan ~/.claude/sessions/ and return all discovered sessions
pub fn scan_sessions() -> Vec<DiscoveredSession> {
    let dir = match sessions_dir() {
        Some(d) => d,
        None => return vec![],
    };

    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    let all_metadata = load_session_metadata();
    let now = now_ms();
    let mut sessions = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "json") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let file: SessionFile = match serde_json::from_str(&content) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let alive = is_process_alive(file.pid);
        let uptime_secs = if file.started_at > 0 && now > file.started_at {
            (now - file.started_at) / 1000
        } else {
            0
        };

        let meta = all_metadata.get(&file.session_id);

        let title = meta
            .and_then(|m| m.title.clone())
            .unwrap_or_else(|| project_name_from_cwd(&file.cwd));

        sessions.push(DiscoveredSession {
            pid: file.pid,
            session_id: file.session_id,
            project_name: project_name_from_cwd(&file.cwd),
            cwd: file.cwd,
            started_at: file.started_at,
            entrypoint: file.entrypoint,
            is_alive: alive,
            uptime_secs,
            title,
            git_branch: meta.and_then(|m| m.git_branch.clone()),
            model: meta.and_then(|m| m.model.clone()),
            message_count: meta.map_or(0, |m| m.message_count),
            total_input_tokens: meta.map_or(0, |m| m.total_input_tokens),
            total_output_tokens: meta.map_or(0, |m| m.total_output_tokens),
            total_cache_read_tokens: meta.map_or(0, |m| m.total_cache_read_tokens),
        });
    }

    // Sort: alive first, then by started_at descending
    sessions.sort_by(|a, b| {
        b.is_alive
            .cmp(&a.is_alive)
            .then(b.started_at.cmp(&a.started_at))
    });

    sessions
}
