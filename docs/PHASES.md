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
**Status: Code-complete, pending live verification**

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
- [x] Stdin flush on initial prompt (was missing — likely "Connecting..." culprit)
- [x] Session title extraction updated to first-user-message (matches clui-cc; `ai-title` records no longer emitted)
- [x] Sidebar enrichment: last-prompt preview, failed-tool badge, entrypoint icon, cache-creation tokens, cli_version
- [x] Markdown rendering in assistant messages (react-markdown + remark-gfm + Tailwind-styled components)

- [x] Code block syntax highlighting (rehype-highlight + highlight.js github-dark theme)
- [x] Follow-up prompts to completed sessions (already wired via InputBar.canSendFollowUp)
- [x] Cancel button wired to SIGINT (Stop button now calls cancelActiveSession → SIGINT + 5s SIGKILL fallback)

### Remaining
- [ ] Debug/verify Claude process spawning works end-to-end (needs live run after flush fix)

## Phase 3: Permission Hub
**Status: MVP code-complete, scoped to hub-spawned sessions**

**Decision:** Started with per-run temp settings files (clui-cc approach), not global `~/.claude/settings.json` injection. Global injection is a future toggle; per-run is safer on crash and can't interfere with the user's own hooks.

### Architecture (implemented)
- Rust axum server on `127.0.0.1:19837` (auto-increments to 19900 on port conflict)
- Two-layer URL auth: `appSecret` (per app launch) + `runToken` (per spawned session)
- Per-run temp settings file at `$TMPDIR/claude-deck-hook-config/claude-deck-hook-<token>.json`, passed to `claude --settings <path>`
- PreToolUse matcher: `^(Bash|Edit|Write|MultiEdit|mcp__.*)$` (other tools bypass via `--allowedTools`)
- 5-minute fail-closed timeout matching claude's `timeout: 300`
- Sensitive field masking before emitting to the UI (token/password/secret/auth/credential/apikey)
- Safe-bash whitelist: ls, pwd, cat, grep, find, git {status,log,diff,show,branch -l}, etc. Auto-approved without UI.

### Done
- [x] Rust: PermissionServer (axum on 127.0.0.1:19837, auto-incrementing port)
- [x] Rust: Per-run hook settings file generator + cleanup on exit
- [x] Rust: Permission request routing via runToken (no session_id lookup needed)
- [x] Rust: Safe bash command whitelist (auto-approve read-only)
- [x] Rust: 5-minute auto-deny timeout
- [x] Rust: Sensitive field masking in tool_input before UI emission
- [x] Rust: Commands `resolve_permission`, `get_permission_server_info`
- [x] Frontend: PermissionOverlay (bottom-right card, Allow / Allow-session / Deny)
- [x] Frontend: usePermissionEvents listener
- [x] Frontend: Store actions `addPermission`, `decidePermission`

### Remaining
- [ ] Frontend: Permission queue badge in sidebar
- [ ] Frontend: Macos notification when app is in background (Tauri notification API)
- [ ] Frontend: Notification sound (configurable)
- [ ] Rust: Scoped allow-domain for WebFetch (parse hostname from tool_input.url)
- [ ] Future: Optional global `~/.claude/settings.json` injection for Cursor/VS Code/terminal coverage

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
