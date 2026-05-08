# Agent Hub - MVP Spec

> A desktop app to manage, monitor, and interact with multiple Claude Code sessions from one place.

---

## 1. Problem Statement

When running 5+ Claude Code sessions across different projects (Cursor tabs, terminals, background agents), there is:
- **No single view** of what's running, where, and in what state
- **Permission requests scatter** across terminals — easy to miss, blocks sessions for up to 5 minutes before auto-deny
- **No way to quickly switch** between active conversations without hunting through IDE tabs
- **No cost visibility** across sessions
- **No persistent agent identities** — every session starts blank, no "Zoey the PM" or "Rex the reviewer"

## 2. MVP Scope

### In Scope (ship this)

| Feature | Priority | Description |
|---------|----------|-------------|
| **Session Dashboard** | P0 | Auto-discover all running Claude Code sessions, show live status |
| **Multi-Session Chat** | P0 | Independent chat panel per session, full markdown rendering |
| **Permission Hub** | P0 | Centralized approve/deny for ALL sessions — the killer feature |
| **New Session Launch** | P0 | Start a new session pointed at any project directory |
| **Session Resume** | P1 | Resume any historical session with full conversation context |
| **Agent Profiles** | P1 | Named agent configs (role, system prompt, model, project dir) |
| **Cost & Token Tracker** | P1 | Per-session and aggregate cost from result events |
| **Project Grouping** | P2 | Sessions grouped by project directory in sidebar |
| **Dark/Light Theme** | P2 | System-follow + manual toggle |

### Out of Scope (V2+)

- Mobile companion app (but Tauri 2 chosen specifically to enable this later)
- Todo routing / auto-assignment
- Proactive agent briefings (auto-prompt on tab open)
- Agent-to-agent handoffs
- Scheduled/cron prompts
- Marketplace / plugin management
- Voice input
- Collaborative multi-user editing

## 3. Tech Stack

```
Framework:    Tauri 2 (Rust backend + webview frontend)
Frontend:     React 19 + TypeScript 5.7 + Vite 6
Styling:      Tailwind CSS 4
State:        Zustand 5 (single store, proven pattern from clui-cc)
Markdown:     react-markdown + remark-gfm + rehype-highlight
Icons:        Phosphor Icons (consistent with clui-cc)
Animations:   Framer Motion 12
Layout:       CSS Grid (sidebar + main) — NOT a floating overlay like clui-cc
DB:           SQLite via Tauri SQL plugin (agent profiles, session bookmarks)
```

### Why Tauri 2 over Electron

| Dimension | Electron (clui-cc) | Tauri 2 (Agent Hub) |
|-----------|--------------------|--------------------|
| Bundle size | ~150 MB | ~8 MB |
| Memory/window | ~150 MB | ~30 MB |
| Backend | Node.js | Rust (fast process mgmt, file watching) |
| Mobile path | None | iOS + Android (Tauri 2 native) |
| Process spawn | child_process.spawn | Rust std::process::Command (more reliable) |
| File watching | fs.watch (flaky) | notify-rs (robust, cross-platform) |
| Subprocess NDJSON | StreamParser (Node streams) | tokio::io::BufReader (async, zero-copy) |

### Why NOT the Claude Agent SDK

The SDK (`@anthropic-ai/claude-agent-sdk`) provides programmatic session control, but:
- Requires Node.js runtime (Tauri backend is Rust)
- We'd need a Node sidecar just for SDK — adds complexity
- The CLI subprocess pattern (`claude -p --output-format stream-json`) is proven by clui-cc
- CLI gives us `--resume`, `--settings`, `--allowedTools`, `--agent` — all we need
- We can add SDK integration later via optional Node sidecar for V2 features

**Decision:** CLI subprocess for MVP, SDK sidecar as V2 option.

## 4. Architecture

```
+------------------------------------------------------------------+
|                      AGENT HUB (Tauri 2)                          |
+------------------------------------------------------------------+
|                                                                    |
|  FRONTEND (Webview — React 19 + Zustand 5)                       |
|                                                                    |
|  +-------------------+  +--------------------------------------+  |
|  | Sidebar            |  | Main Panel                          |  |
|  |                    |  |                                      |  |
|  | [Sessions]         |  | +----------------------------------+|  |
|  |  > Prep (2)        |  | | Chat View                        ||  |
|  |    - Session A  🟢 |  | | - Markdown messages               ||  |
|  |    - Session B  🟡 |  | | - Tool call cards                 ||  |
|  |  > CPD Agent (1)   |  | | - Code blocks + syntax highlight  ||  |
|  |    - Session C  🟢 |  | | - Streaming text                  ||  |
|  |                    |  | +----------------------------------+|  |
|  | [Agents]           |  |                                      |  |
|  |  > Zoey (PM)       |  | +----------------------------------+|  |
|  |  > Rex (Reviewer)  |  | | Input Bar                        ||  |
|  |  > Nova (Ops)      |  | | - Prompt input                   ||  |
|  |                    |  | | - Model selector                  ||  |
|  | [Permission Queue] |  | | - Project dir badge               ||  |
|  |  ! Bash (Session A)|  | +----------------------------------+|  |
|  |  ! Edit (Session C)|  |                                      |  |
|  |                    |  | +----------------------------------+|  |
|  | [Stats]            |  | | Status Bar                       ||  |
|  |  $0.42 today       |  | | Session ID | Model | Cost | Time ||  |
|  |  5 sessions        |  | +----------------------------------+|  |
|  +-------------------+  +--------------------------------------+  |
|                                                                    |
+------------------------------------------------------------------+
|                                                                    |
|  TAURI BACKEND (Rust)                                             |
|                                                                    |
|  +------------------------------------------------------------+  |
|  | SessionManager                                              |  |
|  |                                                              |  |
|  | - Discovery: watch ~/.claude/sessions/ (notify-rs)          |  |
|  | - Process monitor: check PID liveness every 2s              |  |
|  | - Session registry: HashMap<SessionId, SessionState>        |  |
|  +------------------------------------------------------------+  |
|                                                                    |
|  +------------------------------------------------------------+  |
|  | ProcessPool                                                  |  |
|  |                                                              |  |
|  | - Spawn: claude -p --output-format stream-json              |  |
|  | - Parse: NDJSON via tokio BufReader line-by-line            |  |
|  | - Route: events to frontend via Tauri event system          |  |
|  | - Stdin: write prompts + permission responses               |  |
|  | - Cancel: SIGINT → 5s → SIGKILL                            |  |
|  +------------------------------------------------------------+  |
|                                                                    |
|  +------------------------------------------------------------+  |
|  | PermissionServer                                             |  |
|  |                                                              |  |
|  | - HTTP server on 127.0.0.1:19837 (hyper/axum)              |  |
|  | - PreToolUse hook interception                               |  |
|  | - Per-launch secret + per-run token auth                    |  |
|  | - Routes to correct session via token registry              |  |
|  | - 5-min auto-deny timeout                                   |  |
|  | - Smart Bash: auto-approve read-only commands               |  |
|  +------------------------------------------------------------+  |
|                                                                    |
|  +------------------------------------------------------------+  |
|  | HistoryReader                                                |  |
|  |                                                              |  |
|  | - Parse ~/.claude/projects/{path}/{uuid}.jsonl              |  |
|  | - Parse ~/.claude/history.jsonl (global search)             |  |
|  | - Read ~/.claude/sessions/ for session metadata             |  |
|  +------------------------------------------------------------+  |
|                                                                    |
|  +------------------------------------------------------------+  |
|  | AgentStore (SQLite)                                          |  |
|  |                                                              |  |
|  | - Agent profiles (name, role, system_prompt, model, dir)    |  |
|  | - Session bookmarks (pinned sessions)                       |  |
|  | - Cost aggregation (per-session, per-day)                   |  |
|  | - User preferences (theme, layout, default model)           |  |
|  +------------------------------------------------------------+  |
|                                                                    |
+------------------------------------------------------------------+
         |                    |                    |
    Rust Command         File watcher         HTTP hooks
    (claude CLI)         (notify-rs)          (hyper)
         |                    |                    |
         v                    v                    v
   Claude Code          ~/.claude/           Claude Code
   Processes            sessions/            hook POSTs
```

## 5. Feature Deep Dives

### 5.1 Session Discovery & Monitoring

**How it works:**
1. On launch, scan `~/.claude/sessions/*.json` to find all session PIDs
2. For each PID, check if process is alive (`kill(pid, 0)` / `ps`)
3. Read session metadata: `cwd`, `startedAt`, `sessionId`, `entrypoint`
4. Start a `notify-rs` file watcher on `~/.claude/sessions/` for real-time changes
5. Poll process liveness every 2 seconds (like clui-cc's health reconciliation)

**Session state derivation:**
```
Process alive + session file exists     → 🟢 Active
Process alive + permission pending      → 🟡 Waiting (needs attention!)
Session file exists + process dead      → ⚫ Ended
No session file (historical)            → 📁 Archived
```

**Data enrichment per session:**
```rust
struct DiscoveredSession {
    pid: u32,
    session_id: String,
    cwd: String,                    // project directory
    project_name: String,           // last path segment of cwd
    started_at: u64,                // unix timestamp ms
    entrypoint: String,             // "claude-vscode" | "claude-cli"
    is_alive: bool,
    uptime: Duration,
    // Enriched from IDE lock files
    ide_name: Option<String>,       // "Cursor" | "VS Code"
}
```

### 5.2 Multi-Session Chat

**Session types:**
1. **Discovered sessions** — already running externally (Cursor, terminal). Read-only monitoring initially; can "attach" by spawning a new `claude -p --resume <id>` subprocess.
2. **Hub-spawned sessions** — started from the app. Full control: spawn, send prompts, receive events, cancel.

**For hub-spawned sessions (core chat):**

Spawn pattern (adapted from clui-cc's RunManager):
```
claude -p \
  --input-format stream-json \
  --output-format stream-json \
  --verbose \
  --include-partial-messages \
  --permission-mode default \
  --resume <sessionId> \          # if resuming
  --settings <hook-config.json> \ # permission interception
  --allowedTools Read,Glob,Grep,TodoWrite,WebSearch,WebFetch,Agent \
  --append-system-prompt "You are running inside Agent Hub..." \
  --model <preferred-model>
```

**NDJSON event stream (same types as clui-cc):**
```
session_init    → Update session metadata (model, tools, MCP servers)
text_chunk      → Append to current assistant message (streaming)
tool_call       → Show tool card (name, running indicator)
tool_call_update → Append partial JSON input to tool card
tool_call_complete → Mark tool card as done
task_complete   → Record cost, duration, tokens. Status → completed
error           → Show error message, Status → failed
rate_limit      → Show rate limit notice
permission_request → Route to Permission Hub
```

### 5.3 Permission Hub (The Killer Feature)

**The problem it solves:** You have 5 sessions running. Session 3 wants to run `npm install`. Session 1 wants to edit a file. Session 5 wants to run a git command. Currently, you need to find each terminal/tab, review each request, and approve. If you miss one, it auto-denies after 5 minutes.

**How it works:**

1. Agent Hub runs a single HTTP PermissionServer on `127.0.0.1:19837`
2. Every hub-spawned session gets a `--settings` file pointing to this server
3. When any session needs permission, the server receives the POST
4. Routes to the correct session via per-run token
5. Shows in the **sidebar Permission Queue** with a badge count
6. Click → expands the approval card in the main panel
7. User approves/denies → response sent back to the waiting Claude process

**Permission card shows:**
- Which session (name + project)
- Tool name (Bash, Edit, Write, etc.)
- Tool input (command text, file path, code diff)
- Sensitive fields masked (tokens, passwords, keys)
- Options: Allow Once | Allow for Session | Deny
- Bash-specific: auto-approve read-only commands (git status, ls, cat, etc.)

**Cross-session notification:**
- Badge count on sidebar "Permission Queue" section
- macOS notification if app is in background (Tauri notification API)
- Sound alert (configurable)
- Permission auto-deny timer visible (countdown from 5:00)

**Port from clui-cc:** The entire `permission-server.ts` logic (497 lines) translates to Rust cleanly. Key patterns:
- Per-launch app secret (UUID in URL path)
- Per-run token registry (maps token → session)
- Safe bash command whitelist (auto-approve read-only)
- Scoped allows (session-level "allow always" for Edit/Write)
- Sensitive field masking (regex on key names)

### 5.4 Agent Profiles

**Concept:** Pre-configured agent identities that persist across sessions. When you open "Zoey", it starts a Claude session with her system prompt, preferred model, and project directory.

**Schema (SQLite):**
```sql
CREATE TABLE agents (
  id          TEXT PRIMARY KEY,   -- UUID
  name        TEXT NOT NULL,      -- "Zoey"
  role        TEXT NOT NULL,      -- "Session Analytics PM"
  avatar_color TEXT DEFAULT '#22c55e', -- sidebar indicator color
  system_prompt TEXT,             -- appended via --append-system-prompt
  model       TEXT,               -- "claude-sonnet-4-6" (null = default)
  working_dir TEXT,               -- "/Users/devrev/analytics"
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);
```

**How agents launch sessions:**
```
claude -p \
  --output-format stream-json \
  --input-format stream-json \
  --append-system-prompt "<agent.system_prompt>" \
  --model <agent.model> \
  --settings <hook-config.json> \
  --allowedTools Read,Glob,Grep,...
  <agent.working_dir>
```

**Sidebar display:**
```
[Agents]
  🟢 Zoey (PM) — analytics/
  🟢 Rex (Reviewer) — CPD-agent/
  ⚫ Nova (Ops) — idle
```

**Default starter agents (pre-populated):**
```json
[
  {
    "name": "CodeBot",
    "role": "General Coding",
    "system_prompt": null,
    "model": null,
    "working_dir": null
  },
  {
    "name": "Reviewer",
    "role": "Code Reviewer",
    "system_prompt": "You are a senior code reviewer. Focus on bugs, security issues, performance problems, and code quality...",
    "model": "claude-sonnet-4-6",
    "working_dir": null
  }
]
```

### 5.5 Cost & Token Tracking

**Source:** `task_complete` events include:
```json
{
  "costUsd": 0.042,
  "durationMs": 15230,
  "numTurns": 3,
  "usage": {
    "input_tokens": 12500,
    "output_tokens": 3200,
    "cache_read_input_tokens": 8000
  }
}
```

**What we track (SQLite):**
```sql
CREATE TABLE session_costs (
  id          TEXT PRIMARY KEY,
  session_id  TEXT NOT NULL,
  agent_id    TEXT,             -- null if not agent-spawned
  cost_usd    REAL NOT NULL,
  input_tokens INTEGER,
  output_tokens INTEGER,
  cache_tokens INTEGER,
  duration_ms INTEGER,
  num_turns   INTEGER,
  recorded_at INTEGER NOT NULL
);
```

**Sidebar display:**
```
[Stats]
  Today: $1.24 (12 runs)
  This session: $0.08
  Model: Opus 4.6
```

**Status bar per session:**
```
Session: abc123 | Model: claude-opus-4-6 | Cost: $0.08 | Tokens: 15.7K in / 3.2K out | 15.2s
```

### 5.6 Session Resume

**How it works:**
1. Read `~/.claude/history.jsonl` for global prompt history
2. Read `~/.claude/projects/{path}/{uuid}.jsonl` for full conversation
3. Show in a "History" view: session ID, first message preview, project, timestamp
4. Click → opens a new tab with conversation loaded + `--resume <sessionId>`

**History entry:**
```
📁 Prep/                  "NL2WF interview answers..."        2 min ago
📁 CPD-indigo-Agent/      "Fix the deployment pipeline..."    45 min ago
📁 career-ops/            "Update resume with new project..." 2 hours ago
```

## 6. Project Structure

```
agent-hub/
├── src-tauri/                          # Rust backend
│   ├── Cargo.toml
│   ├── tauri.conf.json                 # Tauri config (window, permissions, plugins)
│   ├── src/
│   │   ├── main.rs                     # Entry point, Tauri builder
│   │   ├── commands.rs                 # Tauri IPC commands (frontend → backend)
│   │   ├── session/
│   │   │   ├── mod.rs
│   │   │   ├── discovery.rs            # Watch ~/.claude/sessions/, detect running sessions
│   │   │   ├── manager.rs              # Session lifecycle, state tracking
│   │   │   └── types.rs                # SessionState, DiscoveredSession
│   │   ├── process/
│   │   │   ├── mod.rs
│   │   │   ├── pool.rs                 # Spawn claude -p, manage subprocesses
│   │   │   ├── stream_parser.rs        # NDJSON line parser (tokio BufReader)
│   │   │   └── event_normalizer.rs     # Raw events → canonical types
│   │   ├── permission/
│   │   │   ├── mod.rs
│   │   │   ├── server.rs               # HTTP hook server (hyper/axum)
│   │   │   ├── safe_bash.rs            # Read-only command whitelist
│   │   │   └── types.rs                # PermissionRequest, HookToolRequest
│   │   ├── history/
│   │   │   ├── mod.rs
│   │   │   └── reader.rs              # Parse JSONL files, history.jsonl
│   │   ├── agent/
│   │   │   ├── mod.rs
│   │   │   └── store.rs               # SQLite agent profile CRUD
│   │   ├── cost/
│   │   │   ├── mod.rs
│   │   │   └── tracker.rs             # Aggregate cost from task_complete events
│   │   └── db.rs                       # SQLite connection + migrations
│   └── migrations/
│       └── 001_initial.sql             # agents, session_costs, preferences
│
├── src/                                # React frontend
│   ├── main.tsx                        # React entry
│   ├── App.tsx                         # Root layout (sidebar + main)
│   ├── stores/
│   │   └── appStore.ts                 # Zustand single store
│   ├── components/
│   │   ├── sidebar/
│   │   │   ├── Sidebar.tsx             # Container
│   │   │   ├── SessionList.tsx         # Active sessions grouped by project
│   │   │   ├── AgentList.tsx           # Agent profiles
│   │   │   ├── PermissionBadge.tsx     # Queue count + items
│   │   │   └── StatsPanel.tsx          # Cost aggregation
│   │   ├── chat/
│   │   │   ├── ChatView.tsx            # Message timeline
│   │   │   ├── MessageBubble.tsx       # User/assistant/system messages
│   │   │   ├── ToolCallCard.tsx        # Tool execution display
│   │   │   ├── StreamingText.tsx       # Live text append during generation
│   │   │   └── CodeBlock.tsx           # Syntax-highlighted code
│   │   ├── input/
│   │   │   ├── InputBar.tsx            # Prompt input + controls
│   │   │   └── ModelPicker.tsx         # Dropdown for model selection
│   │   ├── permission/
│   │   │   ├── PermissionCard.tsx      # Approve/deny UI
│   │   │   └── PermissionQueue.tsx     # List of pending approvals
│   │   ├── agents/
│   │   │   ├── AgentEditor.tsx         # Create/edit agent profiles
│   │   │   └── AgentCard.tsx           # Sidebar agent entry
│   │   ├── history/
│   │   │   └── HistoryBrowser.tsx      # Past session browser
│   │   └── shared/
│   │       ├── StatusDot.tsx           # 🟢🟡🔴⚫ indicators
│   │       └── Timer.tsx               # Uptime / countdown display
│   ├── hooks/
│   │   ├── useSessionEvents.ts         # Listen to Tauri events from backend
│   │   ├── useHealthPoll.ts            # Periodic session liveness checks
│   │   └── useKeyboard.ts             # Global shortcuts
│   ├── lib/
│   │   ├── tauri.ts                    # Typed wrappers around invoke/listen
│   │   └── format.ts                   # Cost formatting, time formatting
│   ├── types/
│   │   └── index.ts                    # All shared TypeScript types
│   └── styles/
│       └── index.css                   # Tailwind base + custom tokens
│
├── package.json
├── vite.config.ts
├── tsconfig.json
├── tailwind.config.ts
└── README.md
```

## 7. Data Flow

### 7.1 User sends a prompt

```
InputBar.tsx
  → appStore.sendMessage(prompt)
  → invoke('send_prompt', { sessionId, prompt, model })
  → Rust: commands::send_prompt()
  → ProcessPool::write_stdin(sessionId, userMessage)
  → claude process receives JSON on stdin
  → claude writes NDJSON events to stdout
  → stream_parser reads line-by-line
  → event_normalizer maps to canonical events
  → emit Tauri event: "session-event" { sessionId, event }
  → useSessionEvents.ts receives event
  → appStore.handleEvent(sessionId, event)
  → React re-renders ChatView
```

### 7.2 Permission request arrives

```
Claude process wants to use Bash("npm install")
  → POSTs to http://127.0.0.1:19837/hook/pre-tool-use/<secret>/<runToken>
  → permission::server receives request
  → Checks safe_bash whitelist → NOT safe (npm install mutates)
  → Emits Tauri event: "permission-request" { sessionId, questionId, tool, input }
  → Frontend: PermissionBadge count increments
  → User clicks permission in sidebar → PermissionCard renders
  → User clicks "Allow Once"
  → invoke('respond_permission', { questionId, decision: 'allow' })
  → Rust: permission::server responds to pending HTTP request
  → Claude process receives allow → runs npm install
  → Results stream back as normal events
```

### 7.3 Session discovery

```
App launch
  → Rust: session::discovery::scan_sessions()
  → Read ~/.claude/sessions/*.json
  → For each: check process alive, read metadata
  → Emit Tauri event: "sessions-discovered" [{ pid, sessionId, cwd, alive }]
  → Start notify-rs watcher on ~/.claude/sessions/
  → On file change: re-scan, emit delta
  → Frontend: SessionList updates sidebar
```

## 8. Tauri Commands (IPC Surface)

```rust
// Session management
#[tauri::command] fn list_sessions() -> Vec<DiscoveredSession>;
#[tauri::command] fn create_session(working_dir: String, agent_id: Option<String>) -> SessionInfo;
#[tauri::command] fn resume_session(session_id: String) -> SessionInfo;
#[tauri::command] fn send_prompt(session_id: String, prompt: String, model: Option<String>) -> ();
#[tauri::command] fn cancel_session(session_id: String) -> bool;
#[tauri::command] fn close_session(session_id: String) -> ();

// Permission
#[tauri::command] fn respond_permission(question_id: String, decision: String) -> bool;

// Agent profiles
#[tauri::command] fn list_agents() -> Vec<Agent>;
#[tauri::command] fn create_agent(agent: AgentInput) -> Agent;
#[tauri::command] fn update_agent(id: String, agent: AgentInput) -> Agent;
#[tauri::command] fn delete_agent(id: String) -> ();

// History
#[tauri::command] fn list_history(limit: u32) -> Vec<HistoryEntry>;
#[tauri::command] fn load_session_messages(session_id: String) -> Vec<Message>;

// Cost
#[tauri::command] fn get_cost_summary(period: String) -> CostSummary;

// Preferences
#[tauri::command] fn get_preferences() -> Preferences;
#[tauri::command] fn set_preferences(prefs: Preferences) -> ();
```

## 9. Tauri Events (Backend → Frontend)

```rust
// Emitted per session, per event
"session-event"         → { session_id, event: NormalizedEvent }
"session-status"        → { session_id, status: "running"|"completed"|"failed"|"dead" }
"session-error"         → { session_id, error: EnrichedError }

// Emitted globally
"sessions-discovered"   → Vec<DiscoveredSession>
"permission-request"    → { session_id, question_id, tool_name, tool_input, options }
"permission-timeout"    → { question_id }
```

## 10. Key Design Decisions

### 10.1 Standard window, NOT floating overlay

clui-cc is a floating pill overlay — clever for quick access, but not suited for managing 5+ sessions. Agent Hub is a **full desktop window** with sidebar navigation. You keep it open alongside your IDE.

### 10.2 Sidebar-first navigation

Inspired by the reference screenshot (Zoey/Rex/Nova sidebar). The sidebar is the command center:
- **Sessions section:** Grouped by project, status dots, permission badges
- **Agents section:** Named agent profiles, click to launch
- **Permission Queue:** Badge count, click to review
- **Stats:** Quick cost overview

### 10.3 One PermissionServer for all sessions

Unlike clui-cc (one server per app instance, one app instance), we run one server that handles ALL hub-spawned sessions. The per-run token in the URL path routes each request to the correct session.

### 10.4 Discovered vs. Hub-spawned sessions

- **Discovered sessions** (running in Cursor/terminal): We can see them in the sidebar (name, project, status), but we can NOT intercept their permissions or read their live stream. They're read-only presence indicators.
- **Hub-spawned sessions:** Full control — chat, permissions, events, cost tracking.

This is an honest limitation. Attaching to running processes would require injecting hook configs into already-running sessions, which isn't supported.

### 10.5 History reading is file-based

We parse Claude's own JSONL files — no separate database for conversation history. The SQLite database is only for Agent Hub's own data (agent profiles, cost tracking, preferences).

## 11. Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+N` | New session |
| `Cmd+T` | New session from agent picker |
| `Cmd+1-9` | Switch to session 1-9 |
| `Cmd+W` | Close current session tab |
| `Cmd+K` | Focus search / session filter |
| `Cmd+Enter` | Send prompt |
| `Cmd+Shift+P` | Jump to permission queue |
| `Cmd+.` | Cancel current session |
| `Esc` | Close agent editor / history browser |

## 12. Build & Dev Commands

```bash
# Development
npm run tauri dev          # Start Tauri dev mode (hot reload frontend + Rust backend)

# Production build
npm run tauri build        # Build .dmg for macOS (or .AppImage/.msi for Linux/Windows)

# Frontend only (for UI iteration)
npm run dev                # Vite dev server at localhost:5173
```

## 13. Implementation Phases

### Phase 1: Foundation (Days 1-3)
- [ ] Scaffold Tauri 2 project with React 19 + Tailwind + Zustand
- [ ] Implement session discovery (read `~/.claude/sessions/`, file watcher)
- [ ] Build sidebar layout with session list
- [ ] Implement process liveness checking

### Phase 2: Chat Engine (Days 4-6)
- [ ] Implement ProcessPool (spawn `claude -p`, NDJSON parsing in Rust)
- [ ] Implement event normalizer (port from clui-cc's event-normalizer.ts)
- [ ] Build ChatView with markdown rendering + streaming text
- [ ] Build InputBar with prompt submission
- [ ] Wire Tauri events: backend → frontend for live streaming

### Phase 3: Permission Hub (Days 7-8)
- [ ] Implement PermissionServer in Rust (port from clui-cc's permission-server.ts)
- [ ] Build PermissionCard component with approve/deny/allow-session
- [ ] Wire sidebar badge count + notification sound
- [ ] Implement safe bash command whitelist
- [ ] Add per-session scoped allows

### Phase 4: Agent Profiles + History (Days 9-10)
- [ ] Set up SQLite with Tauri SQL plugin
- [ ] Implement agent CRUD (create, edit, delete, list)
- [ ] Build AgentEditor modal
- [ ] Implement session resume from history
- [ ] Build HistoryBrowser with search

### Phase 5: Polish (Days 11-12)
- [ ] Cost tracking from task_complete events
- [ ] Dark/light theme with system follow
- [ ] Keyboard shortcuts
- [ ] Status bar with session metadata
- [ ] macOS .dmg build
- [ ] Error handling + edge cases (process crashes, stale sessions)

## 14. Patterns to Port from clui-cc

| Pattern | clui-cc Source | Port to |
|---------|---------------|---------|
| NDJSON stream parsing | `run-manager.ts` L234-280 | `process/stream_parser.rs` |
| Event normalization | `event-normalizer.ts` (173 lines) | `process/event_normalizer.rs` |
| Tab state machine | `control-plane.ts` L803-813 | `session/manager.rs` |
| Permission HTTP server | `permission-server.ts` (630 lines) | `permission/server.rs` |
| Safe bash whitelist | `permission-server.ts` L42-137 | `permission/safe_bash.rs` |
| Health polling | `useHealthReconciliation.ts` | `hooks/useHealthPoll.ts` |
| Zustand store shape | `sessionStore.ts` (857 lines) | `stores/appStore.ts` |
| CLI arg construction | `run-manager.ts` L154-204 | `process/pool.rs` |
| Sensitive field masking | `permission-server.ts` L611-629 | `permission/server.rs` |

## 15. Open Questions (Decide During Build)

1. **Port number conflict:** clui-cc uses 19836, we use 19837. But what if user runs both? Should we detect and negotiate?

2. **Attaching to discovered sessions:** Can we read their JSONL files in real-time (tail -f equivalent) to show live status even for non-hub sessions? This would be a nice V1.5 feature.

3. **Multiple windows:** Should Agent Hub support multiple windows (one per project)? Or always single-window?

4. **ntfy.sh integration:** User already has hooks sending to ntfy.sh. Should we surface these as notifications too, or replace that workflow entirely?

5. **Agent memory persistence:** Should agents have their own `MEMORY.md`-like persistence, separate from Claude's built-in memory? Or rely on Claude's native memory?
