# claude-deck

> Mission-control desktop app for managing **multiple Claude Code sessions** in one pane of glass. Spawn, monitor, and (soon) centrally approve permissions across every session you have running — Cursor, VS Code, terminal, or hub-native.

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8DB.svg)](https://tauri.app)
[![React 19](https://img.shields.io/badge/React-19-61dafb.svg)](package.json)
[![Rust](https://img.shields.io/badge/Rust-stable-dea584.svg)](src-tauri/Cargo.toml)
[![Status: WIP](https://img.shields.io/badge/status-WIP_Phase_2-f59e0b.svg)](#current-status)

> **Heads up:** this is a work-in-progress personal project. Phase 1 (session discovery + dashboard) works end-to-end; Phase 2 (hub-spawned chat engine) is ~80% built but currently blocked on a macOS GUI env/stdout issue documented below. Reading honest engineering trade-offs is kind of the point — skip to [Current Status](#current-status) for the full picture.

---

## Demo

<!-- Capture screenshots / GIF per docs/CAPTURE_CHECKLIST.md and drop them in docs/images/ -->

<p align="center">
  <img src="docs/images/sidebar.png" alt="claude-deck sidebar showing multiple Claude sessions" width="720"/>
</p>

<p align="center">
  <img src="docs/images/chat-view.png" alt="claude-deck chat view rendering NDJSON events" width="720"/>
</p>

<!-- End-to-end screen recording: docs/demo.gif -->

---

## Why I built this

I run five-plus Claude Code sessions in parallel — in Cursor, VS Code terminals, raw zsh, sometimes all at once on the same refactor. The operational glue (remembering which session owns which branch, approving permissions twice per session, spotting when one hit a rate limit) was eating more time than the actual work.

**claude-deck** started as a weekend answer: one window that knows about every Claude Code session on the machine, streams their output in real time, and (next phase) lets me approve permissions centrally via Claude's file-watched `settings.json` hook system. Along the way it's a showcase of Tauri 2, tokio-based process management, and a non-trivial NDJSON streaming UI.

---

## Features

### ✅ Working today (Phase 1)

- **Session discovery** — scans `~/.claude/sessions/*.json` and `~/.claude/projects/*/*.jsonl` on disk; shows every session regardless of which client spawned it.
- **Live sidebar** with project grouping, health polling every 3 s, token/uptime formatting, and animated status dots.
- **Metadata enrichment** — surfaces model, cost, token counts, last activity from the raw session files.
- **Cross-pane IPC** wired end-to-end: Rust commands return `Result<T, String>`, events stream back to the React layer via `app.emit`.

### 🚧 Built but currently blocked (Phase 2 — ~80%)

- **Process spawning** of `claude -p --input-format stream-json --output-format stream-json --verbose --include-partial-messages` with a bespoke `build_spawn_env()` that merges parent env + login-shell env (`/bin/zsh -lc "env -0"`) + extra PATH entries — needed because macOS GUI apps don't inherit `.zshrc` vars.
- **NDJSON normalizer** in `src-tauri/src/process/events.rs` that decodes `system`, `stream_event`, `result`, and `rate_limit_event` payloads into a typed `NormalizedEvent` enum.
- **Chat UI** — `MessageBubble`, `ToolCallCard`, `InputBar`, `EmptyState`; RAF-batched text streaming via Zustand selectors.
- **Blocker:** spawned sessions return a PID but no stdout ever flows. See [Current Status](#current-status) for the leading hypotheses.

### 📋 Planned

- **Phase 3 — Permission Hub** (most ambitious): a local HTTP server + a hook injected into `~/.claude/settings.json`. Because Claude Code file-watches that config, the hook applies to *every* session on the machine — hub-spawned or not. One central approval UI for permissions across Cursor, VS Code, and terminal Claude.
- **Phase 4 — Agent Profiles + History** (SQLite, agent CRUD, searchable run log).
- **Phase 5 — Polish** (cost dashboard, themes, keyboard shortcuts, signed `.dmg`).

Full tracker: [docs/PHASES.md](docs/PHASES.md).

---

## Tech stack

| Layer | Tools |
|---|---|
| Desktop shell | Tauri 2 (Rust binary + WebView2 / WKWebView) |
| Backend | Rust stable, tokio (`process`, `io-util`, `sync`, `rt`), serde, dirs, uuid |
| Frontend | React 19, TypeScript 5.7, Tailwind CSS 4, Zustand 5, Framer Motion |
| Build | Vite 6, `@tauri-apps/cli` 2, `@tailwindcss/vite` |
| Streaming | NDJSON over child stdin/stdout, normalized into typed Rust enums, relayed to the UI via Tauri events |
| Icons | @phosphor-icons/react |

---

## Architecture

```
┌────────────────────────────────────────────────────────────────────────┐
│                           claude-deck (Tauri 2)                         │
│                                                                         │
│  ┌───────────────────────────┐        ┌────────────────────────────┐   │
│  │   Rust backend            │◄──────►│   React 19 frontend        │   │
│  │                           │        │                            │   │
│  │  lib.rs   (commands)      │  IPC   │  Zustand store             │   │
│  │  commands.rs              │◄──────►│  ├─ sessions                │   │
│  │  ├─ list_sessions          │ (cmds +│  ├─ hubSessions             │   │
│  │  ├─ create_session         │ events)│  └─ accumulated messages   │   │
│  │  ├─ send_prompt            │        │                            │   │
│  │  └─ cancel_session         │        │  Components                │   │
│  │                           │        │  ├─ Sidebar / SessionList   │   │
│  │  process/pool.rs          │        │  ├─ ChatView                │   │
│  │  ├─ ProcessPool            │        │  ├─ MessageBubble           │   │
│  │  ├─ build_spawn_env        │        │  ├─ ToolCallCard            │   │
│  │  └─ stdin/stdout/stderr    │        │  └─ InputBar                │   │
│  │                           │        │                            │   │
│  │  process/events.rs        │        │  Hooks                     │   │
│  │  └─ NormalizedEvent enum  │        │  ├─ useHealthPoll (3s)      │   │
│  │                           │        │  └─ useSessionEvents (RAF)  │   │
│  │  session/discovery.rs     │        └────────────────────────────┘   │
│  │  └─ scans ~/.claude/*      │                                         │
│  └───────────────────────────┘                                         │
│                │                                                        │
│                ▼                                                        │
│   spawns: `claude -p --input-format stream-json --output-format        │
│             stream-json --verbose --include-partial-messages`          │
└────────────────────────────────────────────────────────────────────────┘
```

Deeper technical notes: [CLAUDE.md](CLAUDE.md) (verbose context for future AI sessions) and [docs/MVP-SPEC.md](docs/MVP-SPEC.md).

---

## Current Status

Phase 1 ships. Phase 2 is the interesting part — and where honesty matters for a portfolio repo.

### What works

- Session discovery, sidebar rendering, health polling, metadata enrichment.
- Spawning a Claude process succeeds (valid PID, binary resolves via augmented PATH).
- NDJSON parser is unit-complete for every event shape the protocol emits.
- UI surfaces messages, tool calls, and system events correctly in dev fixtures.

### What's blocked

**Symptom:** hub-spawned Claude sessions return a PID but never stream stdout. UI sits at "Connecting…"; stderr is empty; no rate-limit event; no result.

**Root cause under investigation.** Leading hypotheses (in order of likelihood):

1. **`stdin.flush()` missing on the initial prompt** — follow-ups call `flush()`; the first write doesn't. Claude may be buffering waiting for a newline-flushed input.
2. **`--allowedTools` flag format** — currently passed as a single comma-separated arg. Upstream Claude Code may want space-separated or repeated flags.
3. **Env capture from the Tauri GUI context** — `/bin/zsh -lc "env -0"` works from a terminal; when run from Tauri's spawn context it may return a truncated env. A debug command that dumps what we actually captured is the next test.
4. **Stderr races** — auth errors could arrive on stderr *before* any stdout, never surfacing because the UI only subscribes to stdout-derived events. Mitigation is straightforward once confirmed.

Reference: [clui-cc](https://github.com/) (Electron Claude Desktop) uses a much simpler approach — capture only `PATH` via `/bin/zsh -lc "echo $PATH"`, keep parent `process.env` intact, no `env_clear()`. Worth porting if hypothesis 3 confirms.

### Why ship this as-is

Hiding known issues on a portfolio project is worse than documenting them. The architecture and tooling decisions — Tauri 2, tokio async, RAF-batched Zustand updates, NDJSON normalization — are sound and reviewable. The remaining work is a debugging exercise, not a rewrite.

---

## Setup

### Prerequisites

- macOS (Apple Silicon or Intel)
- [Rust stable](https://rustup.rs/) (`rustup default stable`)
- Node 20+ and npm
- Xcode Command Line Tools (`xcode-select --install`)
- Claude Code CLI installed (`npm install -g @anthropic-ai/claude-code` or whichever channel you use)

### Install & run

```bash
npm install

# Dev mode with hot reload
npm run tauri dev

# Production build (.app + .dmg in src-tauri/target/release/bundle/)
npm run tauri build

# Rust-only check (faster than a full build)
cargo check --manifest-path src-tauri/Cargo.toml
```

> The local folder is still named `agent-hub/` on disk — renaming the working tree mid-stream would invalidate `target/` without benefit. The shipping product, bundle identifier, and crate are all `claude-deck`.

---

## Project layout

```
claude-deck/
├── src/                         # React 19 frontend
│   ├── stores/appStore.ts       # Zustand — sessions, messages, event handling
│   ├── hooks/
│   │   ├── useHealthPoll.ts     # 3s session discovery refresh
│   │   └── useSessionEvents.ts  # Tauri event listener, RAF-batched chunks
│   ├── components/
│   │   ├── sidebar/             # Nav, live session count, project grouping
│   │   ├── chat/                # ChatView + MessageBubble + ToolCallCard
│   │   ├── input/               # InputBar with CWD picker
│   │   └── shared/              # StatusDot, etc.
│   └── lib/                     # tauri.ts invoke wrappers, formatters
│
├── src-tauri/                   # Rust backend
│   └── src/
│       ├── lib.rs               # App init, command registration
│       ├── commands.rs          # list/create/send/cancel/debug_info
│       ├── process/
│       │   ├── pool.rs          # ProcessPool, build_spawn_env()
│       │   └── events.rs        # NormalizedEvent + NDJSON parser
│       └── session/
│           ├── discovery.rs     # ~/.claude/* scanner
│           └── types.rs         # Session / Metadata structs
│
├── docs/
│   ├── MVP-SPEC.md              # Original MVP spec
│   ├── PHASES.md                # Phase tracker with checklists
│   ├── CAPTURE_CHECKLIST.md     # Screenshot/demo capture guide
│   └── images/                  # Screenshots referenced in README
│
├── CLAUDE.md                    # Verbose context for Claude Code sessions
├── README.md
├── LICENSE                      # Apache 2.0
└── package.json
```

---

## Contributing

This is a personal project, but bug reports and design-level feedback are welcome via issues. If you've wrestled with the same macOS GUI / login-shell env-capture problem for Tauri, I'd love to compare notes.

---

## License

Apache License 2.0 — see [LICENSE](LICENSE).
