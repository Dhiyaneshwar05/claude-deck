# Agent Hub — Phase Tracker

## Phase 1: Foundation
**Status: Done**

- [x] Scaffold Tauri 2 + React 19 + Tailwind 4 + Zustand 5
- [x] Session discovery (read `~/.claude/sessions/*.json`)
- [x] Process liveness checking (`kill(pid, 0)`)
- [x] Sidebar with session list grouped by project
- [x] Session titles from ai-title records in JSONL conversation files
- [x] Enriched session details: git branch, model, message count, token usage
- [x] Health polling (3s interval)
- [x] StatusDot component (green/gray)
- [x] Session deselection (sidebar toggle + X button)

## Phase 2: Chat Engine
**Status: In Progress (~80%)**

### Done
- [x] ProcessPool in Rust (spawn `claude -p --stream-json`, manage stdin/stdout/stderr)
- [x] NDJSON event parsing + normalization (system init, text_chunk, tool_call, result, rate_limit)
- [x] Tauri event emission (backend -> frontend via `session-event`)
- [x] Frontend event listener with RAF text chunk batching (`useSessionEvents`)
- [x] Zustand store: hubSessions state, event handling, message accumulation
- [x] ChatView with message timeline (user/assistant/system/tool messages)
- [x] MessageBubble component (user/assistant/system roles)
- [x] ToolCallCard component (running/completed states, input preview)
- [x] InputBar component (auto-resize, Cmd+Enter, CWD picker for new sessions)
- [x] Hub sessions visible in sidebar with status
- [x] Stderr forwarding to frontend as error events
- [x] Environment propagation for macOS GUI (PATH, HOME, USER, SHELL)

### Remaining
- [ ] Debug/verify Claude process spawning works end-to-end
- [ ] Markdown rendering in assistant messages (react-markdown + remark-gfm)
- [ ] Code block syntax highlighting (rehype-highlight or shiki)
- [ ] Follow-up prompts to completed sessions
- [ ] Cancel button wired to SIGINT

## Phase 3: Permission Hub
**Status: Not Started**

This is the killer feature. All Claude sessions (including Cursor/VS Code/terminal) route permissions through Agent Hub.

**Key discovery:** Claude Code watches `~/.claude/settings.json` with a file watcher. Hooks added there apply to ALL sessions and are picked up immediately without restart.

### Architecture
- Agent Hub starts HTTP PermissionServer on `127.0.0.1:19837`
- On launch: inject PreToolUse HTTP hook into `~/.claude/settings.json`
- All running Claude sessions immediately start routing permission requests to Agent Hub
- On quit: remove the hook from settings.json (cleanup)

### Tasks
- [ ] Rust: PermissionServer (axum/hyper HTTP server on 127.0.0.1:19837)
- [ ] Rust: Hook injection — read/write `~/.claude/settings.json` on app launch/quit
- [ ] Rust: Permission request routing (session identification via token/metadata)
- [ ] Rust: Safe bash command whitelist (auto-approve read-only: git status, ls, cat, etc.)
- [ ] Rust: 5-minute auto-deny timeout with countdown
- [ ] Rust: Sensitive field masking (tokens, passwords, keys in tool inputs)
- [ ] Frontend: PermissionCard component (tool name, input preview, approve/deny/allow-session)
- [ ] Frontend: PermissionQueue in sidebar with badge count
- [ ] Frontend: Permission notification sound (configurable)
- [ ] Frontend: macOS notification when app is in background (Tauri notification API)
- [ ] Scoped allows: "Allow Edit for this session", "Allow Bash for this session"

## Phase 4: Agent Profiles + History
**Status: Not Started**

### Agent Profiles
- [ ] SQLite setup via Tauri SQL plugin
- [ ] Agent table: name, role, avatar_color, system_prompt, model, working_dir
- [ ] Agent CRUD commands (list, create, update, delete)
- [ ] AgentEditor modal (create/edit agent)
- [ ] AgentCard in sidebar (click to launch session with agent config)
- [ ] Default starter agents (CodeBot, Reviewer)
- [ ] Launch session with agent: `--append-system-prompt`, `--model`, working dir

### Session History
- [ ] Parse `~/.claude/projects/*/*.jsonl` for conversation history
- [ ] HistoryBrowser component with search
- [ ] Session resume via `--resume <sessionId>`
- [ ] History entry display: project, first message, timestamp, message count

## Phase 5: Polish
**Status: Not Started**

### Cost Tracking
- [ ] Aggregate cost from task_complete events
- [ ] Per-session cost display
- [ ] Daily/weekly cost summary in sidebar stats
- [ ] SQLite cost_tracking table

### Theme
- [ ] Dark/light theme toggle
- [ ] System theme follow (prefers-color-scheme)

### Keyboard Shortcuts
- [ ] Cmd+N — New session
- [ ] Cmd+T — New session from agent picker
- [ ] Cmd+1-9 — Switch to session 1-9
- [ ] Cmd+W — Close/deselect current session
- [ ] Cmd+K — Focus search / session filter
- [ ] Cmd+Enter — Send prompt (already done)
- [ ] Cmd+Shift+P — Jump to permission queue
- [ ] Cmd+. — Cancel current session
- [ ] Esc — Close modals

### Build & Distribution
- [ ] macOS .dmg build
- [ ] App icon (proper design, not placeholder)
- [ ] Status bar with session metadata
- [ ] Error handling edge cases (process crashes, stale sessions, disk full)
- [ ] Session filter/search in sidebar

## Future (V2+)
- Mobile companion app (iOS/Android via Tauri 2)
- Agent-to-agent handoffs
- Todo routing / auto-assignment
- Scheduled/cron prompts
- ntfy.sh integration
- Multi-window support
- Agent memory persistence
