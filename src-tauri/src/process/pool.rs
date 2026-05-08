use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use super::events::{normalize_event, NormalizedEvent, SessionEvent};

/// Info about a spawned session returned to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct SpawnedSessionInfo {
    pub session_id: String,
    pub pid: u32,
    pub cwd: String,
}

/// Handle to a running Claude process.
struct ProcessHandle {
    /// The session ID assigned by us (pre-Claude init).
    session_id: String,
    /// Working directory.
    cwd: String,
    /// Stdin writer for sending prompts and permission responses.
    stdin: tokio::process::ChildStdin,
    /// Stderr tail buffer (last 20 lines).
    stderr_tail: Vec<String>,
    /// PID of the child process.
    pid: u32,
}

/// Manages all spawned Claude Code subprocesses.
pub struct ProcessPool {
    handles: Arc<Mutex<HashMap<String, ProcessHandle>>>,
}

impl ProcessPool {
    pub fn new() -> Self {
        Self {
            handles: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Capture the user's login shell PATH.
    /// macOS GUI apps don't inherit .zshrc/.bashrc PATH entries.
    /// We run `/bin/zsh -lc "echo $PATH"` (login shell, non-interactive) to get it.
    /// Matches the approach used by clui-cc (Electron Claude desktop app).
    fn capture_login_path() -> Option<String> {
        let output = std::process::Command::new("/bin/zsh")
            .args(["-lc", "echo $PATH"])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if path.is_empty() { None } else { Some(path) }
            }
            _ => None,
        }
    }

    /// Build the environment for spawning Claude.
    /// Strategy (matching clui-cc): inherit parent process env, override PATH
    /// with the user's login shell PATH + well-known binary locations.
    /// Also source key env vars from login shell for AWS/Bedrock auth.
    fn build_spawn_env() -> HashMap<String, String> {
        // Start with current process env (includes Tauri internals)
        let mut env: HashMap<String, String> = std::env::vars().collect();

        // Source the full login shell env for auth vars (AWS creds, etc.)
        // Use `env -0` for NUL-delimited output to handle multi-line values safely
        let full_env = std::process::Command::new("/bin/zsh")
            .args(["-lc", "env -0"])
            .output();

        if let Ok(out) = full_env {
            if out.status.success() {
                let raw = String::from_utf8_lossy(&out.stdout);
                // NUL-delimited: each KEY=VALUE is separated by \0
                for entry in raw.split('\0') {
                    if let Some((key, val)) = entry.split_once('=') {
                        // Override with login shell values (catches AWS creds, etc.)
                        env.insert(key.to_string(), val.to_string());
                    }
                }
            }
        }

        // Build PATH: our extras + login shell PATH + existing PATH
        let login_path = Self::capture_login_path().unwrap_or_default();
        let current_path = env.get("PATH").cloned().unwrap_or_default();
        let home = dirs::home_dir().unwrap_or_default();
        let home_str = home.to_string_lossy();

        let combined_path = format!(
            "{}/.claude/local:/opt/homebrew/bin:/usr/local/bin:{}:{}",
            home_str, login_path, current_path
        );
        env.insert("PATH".to_string(), combined_path);

        env
    }

    /// Find the `claude` binary using the shell environment.
    fn find_claude_binary(env: &HashMap<String, String>) -> Result<String, String> {
        let home = dirs::home_dir().ok_or("No home directory")?;

        // Try ~/.claude/local/claude first
        let local_claude = home.join(".claude").join("local").join("claude");
        if local_claude.exists() {
            return Ok(local_claude.to_string_lossy().to_string());
        }

        // Try well-known paths directly
        for path in [
            "/opt/homebrew/bin/claude",
            "/usr/local/bin/claude",
        ] {
            if std::path::Path::new(path).exists() {
                return Ok(path.to_string());
            }
        }

        // Try which with the shell PATH
        if let Some(path_var) = env.get("PATH") {
            if let Ok(path) = which::which_in("claude", Some(path_var), ".") {
                return Ok(path.to_string_lossy().to_string());
            }
        }

        Err("Could not find 'claude' binary. Is Claude Code installed?".to_string())
    }

    /// Spawn a new Claude Code session.
    pub async fn spawn_session(
        &self,
        app: AppHandle,
        cwd: String,
        prompt: String,
        model: Option<String>,
        resume_session_id: Option<String>,
    ) -> Result<SpawnedSessionInfo, String> {
        // Build spawn environment: inherit process env + login shell PATH & auth vars
        let spawn_env = Self::build_spawn_env();
        let claude_bin = Self::find_claude_binary(&spawn_env)?;

        // Generate a temporary session ID; the real one comes from session_init event.
        let temp_id = uuid::Uuid::new_v4().to_string();

        let mut cmd = Command::new(&claude_bin);
        // Set the merged environment (parent env + login shell overrides)
        cmd.env_clear();
        for (key, val) in &spawn_env {
            cmd.env(key, val);
        }
        cmd.arg("-p")
            .arg("--input-format")
            .arg("stream-json")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose")
            .arg("--include-partial-messages")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .current_dir(&cwd);

        // Safe tools that don't need permission
        cmd.arg("--allowedTools")
            .arg("Read,Glob,Grep,WebSearch,WebFetch,TodoWrite,Agent");

        if let Some(ref m) = model {
            cmd.arg("--model").arg(m);
        }

        if let Some(ref sid) = resume_session_id {
            cmd.arg("--resume").arg(sid);
        }

        let mut child: Child = cmd.spawn().map_err(|e| format!("Failed to spawn claude: {}", e))?;

        let pid = child.id().unwrap_or(0);
        let mut stdin = child.stdin.take().ok_or("Failed to capture stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        // Send the initial prompt via stdin (NDJSON format)
        let user_msg = serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{"type": "text", "text": prompt}]
            }
        });
        let msg_line = format!("{}\n", serde_json::to_string(&user_msg).unwrap());
        stdin
            .write_all(msg_line.as_bytes())
            .await
            .map_err(|e| format!("Failed to write prompt: {}", e))?;

        // Store the handle
        let session_id = resume_session_id.unwrap_or(temp_id);
        {
            let mut handles = self.handles.lock().await;
            handles.insert(
                session_id.clone(),
                ProcessHandle {
                    session_id: session_id.clone(),
                    cwd: cwd.clone(),
                    stdin,
                    stderr_tail: Vec::new(),
                    pid,
                },
            );
        }

        // Spawn stdout reader task
        let handles_clone = self.handles.clone();
        let session_id_clone = session_id.clone();
        let app_clone = app.clone();
        tokio::spawn(async move {
            Self::read_stdout(app_clone, handles_clone, session_id_clone, stdout).await;
        });

        // Spawn stderr reader task (also forwards errors to frontend)
        let handles_clone2 = self.handles.clone();
        let session_id_clone2 = session_id.clone();
        let app_clone_stderr = app.clone();
        tokio::spawn(async move {
            Self::read_stderr(app_clone_stderr, handles_clone2, session_id_clone2, stderr).await;
        });

        // Spawn process exit watcher
        let handles_clone3 = self.handles.clone();
        let session_id_clone3 = session_id.clone();
        let app_clone2 = app.clone();
        tokio::spawn(async move {
            Self::watch_exit(app_clone2, handles_clone3, session_id_clone3, child).await;
        });

        Ok(SpawnedSessionInfo {
            session_id,
            pid,
            cwd,
        })
    }

    /// Read stdout line-by-line, parse NDJSON, normalize, and emit to frontend.
    async fn read_stdout(
        app: AppHandle,
        _handles: Arc<Mutex<HashMap<String, ProcessHandle>>>,
        session_id: String,
        stdout: tokio::process::ChildStdout,
    ) {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Parse the NDJSON line
            let raw: serde_json::Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(_) => continue, // Skip malformed lines
            };

            // Normalize into frontend events
            let events = normalize_event(&raw);
            for event in events {
                let payload = SessionEvent {
                    session_id: session_id.clone(),
                    event,
                };
                let _ = app.emit("session-event", &payload);
            }
        }
    }

    /// Read stderr, keep last 20 lines for diagnostics, and forward errors to frontend.
    async fn read_stderr(
        app: AppHandle,
        handles: Arc<Mutex<HashMap<String, ProcessHandle>>>,
        session_id: String,
        stderr: tokio::process::ChildStderr,
    ) {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            // Forward non-empty stderr lines as error events to help debug
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let payload = SessionEvent {
                    session_id: session_id.clone(),
                    event: NormalizedEvent::Error {
                        message: format!("[stderr] {}", trimmed),
                    },
                };
                let _ = app.emit("session-event", &payload);
            }

            let mut map = handles.lock().await;
            if let Some(handle) = map.get_mut(&session_id) {
                handle.stderr_tail.push(line);
                if handle.stderr_tail.len() > 20 {
                    handle.stderr_tail.remove(0);
                }
            }
        }
    }

    /// Watch for process exit and emit session_dead event.
    async fn watch_exit(
        app: AppHandle,
        handles: Arc<Mutex<HashMap<String, ProcessHandle>>>,
        session_id: String,
        mut child: Child,
    ) {
        let status = child.wait().await;
        let exit_code = status.ok().and_then(|s| s.code());

        // Grab stderr tail before removing handle
        let stderr_tail = {
            let mut map = handles.lock().await;
            let tail = map
                .get(&session_id)
                .map(|h| h.stderr_tail.clone())
                .unwrap_or_default();
            map.remove(&session_id);
            tail
        };

        let payload = SessionEvent {
            session_id,
            event: NormalizedEvent::SessionDead {
                exit_code,
                stderr_tail,
            },
        };
        let _ = app.emit("session-event", &payload);
    }

    /// Send a follow-up prompt to a running session.
    pub async fn send_prompt(&self, session_id: &str, prompt: &str) -> Result<(), String> {
        let mut handles = self.handles.lock().await;
        let handle = handles
            .get_mut(session_id)
            .ok_or_else(|| format!("Session {} not found", session_id))?;

        let user_msg = serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{"type": "text", "text": prompt}]
            }
        });
        let msg_line = format!("{}\n", serde_json::to_string(&user_msg).unwrap());
        handle
            .stdin
            .write_all(msg_line.as_bytes())
            .await
            .map_err(|e| format!("Failed to write: {}", e))?;
        handle
            .stdin
            .flush()
            .await
            .map_err(|e| format!("Failed to flush: {}", e))?;

        Ok(())
    }

    /// Cancel a running session (SIGINT, then SIGKILL after 5s).
    pub async fn cancel_session(&self, session_id: &str) -> Result<(), String> {
        let handles = self.handles.lock().await;
        let handle = handles
            .get(session_id)
            .ok_or_else(|| format!("Session {} not found", session_id))?;

        // Send SIGINT
        unsafe {
            libc::kill(handle.pid as i32, libc::SIGINT);
        }

        // Schedule SIGKILL after 5 seconds
        let pid = handle.pid;
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            // Check if still alive
            let alive = unsafe { libc::kill(pid as i32, 0) == 0 };
            if alive {
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        });

        Ok(())
    }

    /// Check if a session is currently running.
    pub async fn is_running(&self, session_id: &str) -> bool {
        let handles = self.handles.lock().await;
        handles.contains_key(session_id)
    }
}
