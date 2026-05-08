# Claude Deck — Project Context for Claude Code

> This file is an internal context document that Claude Code reads when assisting on this repo.
> It's verbose on purpose (architecture, known issues, tried-and-failed approaches) so future
> sessions can pick up quickly. Start with [README.md](README.md) for a public-facing overview.

## What This Is
A Tauri 2 desktop app (macOS) that manages multiple Claude Code sessions from one place.
The user (Dhiyanesh) runs 5+ Claude sessions simultaneously across Cursor, VS Code, and terminals.
Claude Deck gives a single dashboard to see all sessions, spawn new ones, and (eventually) approve permissions centrally.

## Tech Stack
- **Backend:** Rust (Tauri 2), tokio async runtime
- **Frontend:** React 19 + TypeScript + Vite, Tailwind CSS 4, Zustand 5
- **IPC:** Tauri commands (Rust -> TS) + Tauri events (Rust -> TS, real-time)
- **Icons:** @phosphor-icons/react
- **Markdown:** react-markdown + remark-gfm (installed, not yet wired)

## Architecture

### Rust Backend (`src-tauri/src/`)
```
lib.rs              — Tauri app init, registers commands + ProcessPool state
commands.rs         — IPC command handlers (list_sessions, create_session, send_prompt, cancel_session, debug_info)
process/pool.rs     — ProcessPool: spawns `claude -p --stream-json`, manages stdin/stdout/stderr per session
process/events.rs   — NormalizedEvent enum + normalize_event() parser for Claude NDJSON protocol
session/discovery.rs — Scans ~/.claude/sessions/*.json + ~/.claude/projects/*/*.jsonl for metadata
session/types.rs    — DiscoveredSession, SessionMetadata, AiTitleRecord structs
```

### React Frontend (`src/`)
```
App.tsx                          — Two-column layout (sidebar 280px + main panel)
stores/appStore.ts               — Zustand store: sessions, hubSessions, event handling, message accumulation
hooks/useHealthPoll.ts           — 3-second polling for session discovery refresh
hooks/useSessionEvents.ts        — Tauri event listener with RAF text chunk batching
components/sidebar/Sidebar.tsx   — Nav tabs (Sessions/Agents/History) + live session count
components/sidebar/SessionList.tsx — Hub sessions + discovered sessions grouped by project
components/chat/ChatView.tsx     — Routes: EmptyState / DiscoveredSessionView / HubSessionChat
components/chat/MessageBubble.tsx — User (blue), Assistant (green), System (amber) messages
components/chat/ToolCallCard.tsx  — Tool invocation display with status + input preview
components/input/InputBar.tsx     — Prompt input with CWD picker for new sessions
components/shared/StatusDot.tsx   — Green/gray animated status indicator
lib/tauri.ts                     — Typed Tauri invoke wrappers
lib/format.ts                    — formatUptime, formatTokens, formatModel helpers
types/index.ts                   — All TypeScript interfaces
```

## Claude Code NDJSON Protocol
Sessions are spawned with:
```
claude -p --input-format stream-json --output-format stream-json --verbose --include-partial-messages
```

**Stdin (sending prompts):** NDJSON lines:
```json
{"type":"user","message":{"role":"user","content":[{"type":"text","text":"..."}]}}
```

**Stdout (receiving events):** NDJSON lines with types:
- `system` — init (session_id, model, cwd), hook_started, hook_response
- `stream_event` — content_block_start/delta/stop, message_start/delta/stop
- `result` — task completion with cost/tokens
- `rate_limit_event` — rate limiting info

These are normalized into `NormalizedEvent` variants in `process/events.rs`.

## Phase Status (see docs/PHASES.md for full tracker)
- **Phase 1 (Foundation):** DONE — session discovery, sidebar, health polling, metadata enrichment
- **Phase 2 (Chat Engine):** ~80% — process spawning, NDJSON parsing, event streaming, chat UI all built. BLOCKED: sessions stuck at "Connecting..." (see below)
- **Phase 3 (Permission Hub):** Not started — HTTP permission server, hook injection into ~/.claude/settings.json
- **Phase 4 (Agent Profiles):** Not started — SQLite, agent CRUD, session history browser
- **Phase 5 (Polish):** Not started — cost tracking, themes, keyboard shortcuts, .dmg

## CRITICAL KNOWN ISSUE: Process Spawning Stuck at "Connecting..."

Hub-spawned Claude sessions start but never stream responses. The process spawns successfully (PID exists) but no stdout NDJSON is received.

### What We Know
- User authenticates via **AWS Bedrock** (`CLAUDE_CODE_USE_BEDROCK`, `AWS_BEARER_TOKEN_BEDROCK`, `AWS_REGION`)
- These env vars are in the user's `.zshrc` but macOS GUI apps don't inherit shell profile env vars
- We implemented `build_spawn_env()` in `pool.rs` which:
  1. Starts from `std::env::vars()` (parent process env)
  2. Overlays login shell env via `/bin/zsh -lc "env -0"` (NUL-delimited for multi-line safety)
  3. Builds PATH from: `~/.claude/local` + `/opt/homebrew/bin` + `/usr/local/bin` + login shell PATH + current PATH
- Stderr is forwarded to frontend as error events (but no errors are visible either)
- The claude binary IS found and spawned (PID returned successfully)

### What We Tried (chronologically)
1. Added common paths to PATH for binary resolution -> Binary found, still stuck
2. Added HOME, USER, SHELL, TERM env vars -> Still stuck
3. Full `zsh -ilc env` capture with env_clear() -> Still stuck (and `-i` flag is problematic without TTY)
4. Switched to `zsh -lc "env -0"` (non-interactive, NUL-delimited) + keep parent env -> Latest approach, untested by user

### clui-cc (Electron Claude Desktop) Analysis
The official Electron app uses a simpler approach:
- Captures ONLY PATH from login shell: `/bin/zsh -lc "echo $PATH"`
- Uses Electron's inherited `process.env` with PATH override
- No `env_clear()` — keeps parent process env intact
- stdin written synchronously right after spawn
- No `shell: true`, direct binary spawn

### Likely Remaining Issues to Investigate
1. **stdin flush missing on initial prompt** — We `write_all` but don't `flush()` the initial prompt (we DO flush on follow-ups). Claude may be waiting for input.
2. **`--allowedTools` flag format** — We pass `Read,Glob,Grep,WebSearch,WebFetch,TodoWrite,Agent` as a single comma-separated arg. Need to verify this is the correct format (might need space-separated or multiple --allowedTools flags).
3. **Process might be writing to stderr first** — Auth errors or startup messages could be on stderr before any stdout. Check if stderr events appear in the frontend.
4. **The env capture approach may still not work from Tauri context** — The Tauri process itself may have a minimal env where even `/bin/zsh -lc` doesn't produce the expected output. Could add a debug command that dumps the captured env.

### Key Insight for Phase 3
Claude Code hot-reloads `~/.claude/settings.json` with a file watcher. Hooks added there apply to ALL running sessions immediately. This means the Permission Hub can intercept permissions from ALL Claude sessions (Cursor, VS Code, terminal) — not just hub-spawned ones.

## User Context
- Dhiyanesh G, AI Engineer at DevRev
- Uses Claude Code via AWS Bedrock (not direct Anthropic API)
- Runs macOS, zsh shell
- Prefers concise communication, working code over lengthy explanations
- Building this for personal productivity across multiple simultaneous Claude sessions

## Build Commands
```bash
# Dev mode (hot reload)
npm run tauri dev

# Production build (outputs to src-tauri/target/release/)
npm run tauri build

# Rust-only check (faster iteration)
cargo build --manifest-path src-tauri/Cargo.toml
```

## Conventions
- Rust: standard Tauri 2 patterns, tokio async, serde for serialization
- Frontend: functional components, Zustand with selectors, Tailwind utility classes
- IPC: Tauri commands return Result<T, String>, events via app.emit("session-event", payload)
- No tests yet (MVP velocity phase)
