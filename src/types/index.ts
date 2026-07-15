export interface DiscoveredSession {
  pid: number;
  session_id: string;
  cwd: string;
  project_name: string;
  started_at: number;
  entrypoint: string;
  is_alive: boolean;
  uptime_secs: number;
  title: string;
  git_branch: string | null;
  model: string | null;
  message_count: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_read_tokens: number;
  total_cache_creation_tokens: number;
  last_prompt: string | null;
  failed_tool_count: number;
  transcript_entrypoint: string | null;
  cli_version: string | null;
}

export interface SessionGroup {
  project_name: string;
  sessions: DiscoveredSession[];
}

// ── Chat types ──────────────────────────────────────────────

export interface SpawnedSessionInfo {
  session_id: string;
  pid: number;
  cwd: string;
}

export type MessageRole = "user" | "assistant" | "tool" | "system";

export interface ChatMessage {
  id: string;
  role: MessageRole;
  content: string;
  timestamp: number;
  /** For tool messages */
  tool_name?: string;
  tool_input?: string;
  tool_status?: "running" | "completed" | "error";
  tool_id?: string;
}

// ── Normalized events from Rust backend ─────────────────────

export type NormalizedEvent =
  | { type: "session_init"; session_id: string; model: string; tools: string[] }
  | { type: "text_chunk"; text: string }
  | { type: "tool_call"; tool_name: string; tool_id: string }
  | { type: "tool_call_update"; tool_id: string; partial_input: string }
  | { type: "tool_call_complete"; tool_id: string }
  | {
      type: "task_complete";
      cost_usd: number;
      duration_ms: number;
      num_turns: number;
      session_id: string;
    }
  | { type: "error"; message: string }
  | { type: "session_dead"; exit_code: number | null; stderr_tail: string[] }
  | { type: "rate_limit"; status: string; resets_at: number };

export interface SessionEvent {
  session_id: string;
  event: NormalizedEvent;
}

// ── Permission hub ──────────────────────────────────────────

export type PermissionDecision =
  | "allow"
  | "allow-session"
  | "allow-domain"
  | "deny";

export interface PendingPermission {
  request_id: string;
  run_token: string;
  tab_id: string;
  tool_name: string;
  tool_input: unknown;
  session_id: string;
  cwd: string;
  /** Client-side: when we received the request, for elapsed-time display */
  received_at: number;
}

export type HubSessionStatus =
  | "idle"
  | "connecting"
  | "running"
  | "completed"
  | "failed"
  | "dead";

/** State for a hub-spawned session (one we control). */
export interface HubSession {
  session_id: string;
  pid: number;
  cwd: string;
  model: string | null;
  tools: string[];
  status: HubSessionStatus;
  messages: ChatMessage[];
  cost_usd: number;
  duration_ms: number;
}
