# Permission Hub v2 — Adopting Preloop Patterns

> **Status:** PLAN (not started). Authored 2026-07-15 as a cross-session handoff.
> **Author context:** Written after a deep read of the preloop reference clone
> (`../refs/preloop`, github.com/preloop/preloop) compared against our current
> permission hub. Read this whole file before touching code.
>
> **How to use this doc:** Each Phase below is independently shippable and
> testable. Do them in order (1→5). After each phase: build, run the app, drive
> a real permission through it, then commit. Do NOT batch multiple phases into
> one commit.

---

## 0. Background — why we're doing this

Our permission hub works (proven end-to-end: Claude sessions' tool calls are
intercepted, routed to our local server, surfaced in the Permission Center).
But it has real weaknesses that preloop solved more cleanly. This plan folds
preloop's patterns into Claude Deck, ranked by value.

### Our current architecture (as of this doc)

- **`src-tauri/src/permissions/server.rs`** — `axum` HTTP server on
  `127.0.0.1:19837` (auto-increments to 19900). Holds all state in-memory
  (`run_tokens`, `pending`, `scoped_allows` HashMaps behind tokio Mutexes).
  `handle_pre_tool_use()` is the decision function.
- **`src-tauri/src/permissions/global_settings.rs`** — injects a
  `"type":"http"` PreToolUse hook into `~/.claude/settings.json` on app start
  (`install_global_hook`), strips it on exit (`uninstall_global_hook`).
  Marker: `"claude-deck": true`. URL carries `<app_secret>/<run_token>`.
- **`src-tauri/src/permissions/safe_bash.rs`** — hardcoded read-only-command
  allowlist for the auto-approve fast path (`is_safe_bash_command`).
- **`src-tauri/src/permissions/settings.rs`** — per-run temp settings file
  writer (`write_hook_settings_file`) for hub-spawned sessions (`--settings`).
- **`src-tauri/src/lib.rs`** — startup (`.setup`) spawns the server + installs
  the global hook; `RunEvent::Exit` calls `uninstall_global_hook`.
- **`src-tauri/src/commands.rs`** — `resolve_permission`,
  `get_permission_server_info` IPC handlers.
- **Frontend:** `src/hooks/usePermissionEvents.ts` (listens for
  `permission-request` Tauri events), `src/components/permissions/
  PermissionCenter.tsx` + `PermissionOverlay.tsx`, store in
  `src/stores/appStore.ts` (`addPermission`).

### The three known defects this plan fixes

1. **Dangling-hook bug (ACTIVE PAIN).** The hook is `"type":"http"` pointing at
   our live port. On a clean quit `RunEvent::Exit` removes it, but on **SIGKILL /
   crash** it never fires, leaving a hook in `~/.claude/settings.json` pointing at
   a dead port. Because Claude blocks on that hook for *all* sessions, this hangs
   every other Claude session's Bash/Edit/Write until manually cleaned. We hit
   this repeatedly. `strip_our_hooks` self-heals on next launch, but the gap
   between crash and relaunch is the problem.
2. **Hardcoded fast-path.** `safe_bash.rs` ignores the user's own
   `~/.claude/settings.json` permissions (allow/deny/ask rules, defaultMode).
   It re-invents allow logic instead of honoring what Claude would already do.
3. **Claude-only, in-memory, UI-coupled.** No provider abstraction (can't add
   Cursor/Codex without a rewrite). All state dies with the app. The server's
   lifecycle is welded to the Tauri window.

### Preloop's reference implementation (files to reread while implementing)

- `../refs/preloop/cli/internal/cmd/agents_permission_hook.go` — **the key
  file.** A stateless CLI (`preloop agents permission-hook --source X`) that
  reads a hook event on STDIN, normalizes it to a shared contract, POSTs to a
  backend, writes agent-specific decision JSON to STDOUT, exits. No live port.
- `../refs/preloop/cli/internal/cmd/claude_permission_policy.go` — reads the
  user's *own* `~/.claude/settings.json` (+ `settings.local.json`) and computes
  allow/deny/**ask** via `evaluateClaudePermissionPolicy`. This is the model for
  our fast-path replacement (defect #2).
- `../refs/preloop/runtime-plugins/openclaw-preloop/src/index.ts` — the OpenClaw
  adapter; shows the `checkToolPermission` seam, fail-open/closed policy, and
  `resolveOpenClawClientDecision` (local policy eval before escalating).
- `../refs/preloop/ARCHITECTURE.md`, `../refs/preloop/openapi.yaml`
  (`/api/v1/agents/permission-check`) — the central control-plane contract.

**Key insight:** preloop's hook is a `"type":"command"` bridge that *exits every
call*. There is no long-lived local port in `settings.json`, so the entire
dangling-hook class of bug is structurally impossible. Their shared
`{source, tool_name, tool_input, session_id, cwd, client_decision}` request
contract + per-source render functions is how they support claude_code, codex_cli,
cursor, and openclaw without 4× the code.

---

## Guiding principles for the implementer

- **Ship one phase at a time.** Each phase leaves the app in a working state.
- **Every phase ends with a real run**, not just `cargo build`. Launch the app,
  trigger a permission (safe: `touch`/`rm` in `/tmp`, or a `git status` on a
  non-allowlisted path), confirm the expected behavior, then commit.
- **After every app run that gets killed, check `~/.claude/settings.json`** for a
  dangling `claude-deck` / `127.0.0.1:198xx` hook and remove it. (Phase 1 makes
  this unnecessary — that's the point.)
- **Never break the working baseline.** If a phase can't be finished, revert to
  the last green commit rather than leaving a half-wired hook mechanism.
- The uncommitted Phase-3-permission-hub work currently lives ONLY in the working
  tree of `git_personal_projs/claude-deck` (HEAD is `23636ee`; these files are
  uncommitted). **Commit the current working state as a baseline BEFORE starting
  Phase 1** so there's a green point to return to.

---

## Phase 0 — Baseline commit + safety net (do first, ~15 min)

**Goal:** lock in the current working permission hub as a committed baseline and
add the one guard that de-risks all later work.

1. In `git_personal_projs/claude-deck`, review `git status` (17 modified + the
   `src-tauri/src/permissions/`, `src/components/permissions/`,
   `src/hooks/usePermissionEvents.ts` new files). Confirm it builds and runs.
2. Commit as: `feat(permissions): Phase 3 permission hub baseline (HTTP hook)`.
3. **Add a hardening safety net for the dangling hook even before Phase 1:**
   In `install_global_hook` (`global_settings.rs`), the hook JSON we write is
   `"type":"http"`. Claude has no built-in "if unreachable, skip" for http hooks,
   so we can't fully fix it here — but we CAN reduce blast radius by making
   `strip_our_hooks` run not just on start but via a lightweight external
   cleanup. **Defer the real fix to Phase 1** (command bridge). For now just
   confirm `uninstall_global_hook` is wired to `RunEvent::Exit` in `lib.rs:66-76`
   (it is) and document the SIGKILL gap in a code comment.

**Test:** launch app, run a mutating command in a Claude session, approve it in
the Permission Center, confirm it proceeds. Kill the app, confirm the hook is
stripped on next launch.

**Ship:** commit.

---

## Phase 1 — Command-bridge hook (kills the dangling-hook bug) ⭐ highest value

**Goal:** replace the `"type":"http"` hook-pointing-at-a-live-port with a
`"type":"command"` hook that runs a short-lived bridge process, exactly like
preloop's `agents permission-hook`. After this, a crash can never leave a hook
that hangs other sessions — the worst case becomes a command that fails fast.

### Design

Claude's `settings.json` gets a **command** hook instead of an **http** hook:

```jsonc
// what we write into ~/.claude/settings.json PreToolUse (Phase 1)
{
  "matcher": "^(Bash|Edit|Write|MultiEdit|mcp__.*)$",
  "hooks": [{
    "type": "command",
    "command": "/path/to/claude-deck-hook --socket /tmp/claude-deck/perm.sock",
    "timeout": 310,
    "claude-deck": true
  }]
}
```

The bridge binary (`claude-deck-hook`):
1. Reads the PreToolUse event JSON from **STDIN** (Claude provides this).
2. Connects to the running app over a **Unix domain socket** (not a TCP port —
   a stale socket path is harmless; connect just fails fast).
3. Forwards the event, blocks for the decision (≤310s).
4. Writes the Claude-format decision to **STDOUT**:
   `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow|deny","permissionDecisionReason":"..."}}`
5. **If the socket doesn't exist / connect fails** → the app isn't running →
   emit the configured fail default (see below) and **exit 0**. No hang, ever.

### Why a Unix socket instead of the existing TCP port

- A dead **TCP port** hook makes Claude's http hook *hang* until timeout (the
  current bug). A dead **socket path** just means `connect()` returns
  ECONNREFUSED/ENOENT instantly → the bridge fails fast and exits.
- The bridge is a process that always exits, so nothing persists in
  `settings.json` that can outlive the app in a harmful way.

### Fail-open vs fail-closed (make it explicit — preloop does)

Add config (Phase 4 formalizes it; hardcode a sane default now):
- **Default = fail-closed for mutating tools** is the safe choice, BUT that would
  block the user's normal work whenever the app is down. Preloop's nuance
  (`clientFallbackDecision`): when the app is unreachable, **honor what the user's
  own Claude config would decide** and only fail-default on genuine "ask" cases.
  Implement that here if Phase 2 lands first; otherwise for Phase 1 alone use
  **fail-open** (allow) when the socket is down, and log loudly, so a dead app
  never bricks the user's other sessions. Revisit once Phase 2 gives us policy eval.

### Implementation steps

1. **New binary target** in `src-tauri/` (or a small sibling crate):
   `src/bin/claude-deck-hook.rs`. Keep it dependency-light (std + serde_json +
   a unix socket). Cargo: add `[[bin]] name = "claude-deck-hook"`.
2. **App side:** in `server.rs`, add a Unix-socket listener alongside (or
   replacing) the axum TCP server. Reuse the same `handle_pre_tool_use` decision
   logic — factor the core into a `decide(req) -> PermissionDecision` fn so both
   transports share it. Socket path: `$TMPDIR/claude-deck/perm.sock`
   (dir 0o700, socket cleaned on start).
3. **`global_settings.rs`:** change `install_global_hook` to write a `command`
   hook invoking the bridge binary with the socket path, keeping the
   `"claude-deck": true` marker and the same matcher. `is_ours` /
   `strip_our_hooks` already match by marker — extend the legacy-detection arm to
   also recognize the old http URL form (already does) AND the new command form
   (match on `command` containing `claude-deck-hook`).
4. **Resolve the bridge binary path:** in dev it's
   `src-tauri/target/debug/claude-deck-hook`; in release it's bundled next to the
   app. Compute at install time from `std::env::current_exe()`.
5. **Keep the per-run temp-settings path (`settings.rs`) in sync** — hub-spawned
   sessions should use the same command hook. Update `write_hook_settings_file`
   to emit the command form too.

### Test (must do all three)

- Approve path: launch app, run `rm /tmp/x` in a Claude session → prompt appears
  in Permission Center → approve → command runs.
- Deny path: same, deny → command blocked with reason.
- **Crash path (the whole point):** `kill -9` the app. Confirm a Claude session's
  Bash call does **not** hang — the bridge fails fast (fail-open/allow with a log,
  or fail per policy). Confirm no manual `settings.json` cleanup is needed for
  sessions to keep working.

**Ship:** commit `feat(permissions): command-bridge hook via unix socket (fixes dangling hook)`.

---

## Phase 2 — Honor the user's own Claude permission policy (fast path) ⭐

**Goal:** replace the hardcoded `safe_bash.rs` allowlist with a real evaluation
of the user's `~/.claude/settings.json` (+ `settings.local.json`) permission
rules, returning allow / deny / **ask**. Only "ask" escalates to the human queue.
Port `claude_permission_policy.go` to Rust.

### What to port (from `../refs/preloop/cli/internal/cmd/claude_permission_policy.go`)

- `loadClaudePermissionPolicy()` — read + merge `settings.json` and
  `settings.local.json` (local wins for `defaultMode`, rules are unioned).
- `evaluateClaudePermissionPolicy(policy, mode, toolName, toolInput)` with the
  exact precedence (comment lines 85-91 in the Go file):
  1. `bypassPermissions` mode → allow
  2. matching **deny** rule → deny
  3. matching **ask** rule → ask (beats allow)
  4. `acceptEdits` mode + edit tool → allow
  5. matching **allow** rule → allow
  6. else → ask
- Rule matching: `matchClaudePermissionRule` — parse `Tool(specifier)`,
  `claudeRuleTarget` (Bash→command, Read/Edit/Write→path, WebFetch→url),
  `globMatch` (simple `*` glob). Port all three.

### Implementation steps

1. New module `src-tauri/src/permissions/claude_policy.rs` mirroring the Go file.
   Add unit tests mirroring the intent (Bash allow/deny/ask, glob specifiers,
   acceptEdits, bypassPermissions).
2. In the decision core (`decide()` from Phase 1), before emitting to the UI:
   - Compute `client_decision = evaluate_claude_permission_policy(...)`.
   - `allow` → return allow immediately (no UI). `deny` → return deny immediately.
   - `ask` → fall through to the emit-to-UI + wait path.
3. **Keep `safe_bash.rs` as a secondary fast-path** OR retire it. Recommendation:
   keep it as an *additional* auto-allow for obviously-read-only commands the
   user didn't explicitly list, but policy eval takes precedence. Decide during
   implementation; document the choice in code.
4. Wire `permission_mode` from the hook event (`HookToolRequest` already has
   `permission_mode`, `server.rs:34`) into the evaluator.

### Test

- Add `"Bash(npm run test:*)"` to your own `~/.claude/settings.json` allow list,
  run `npm run test:unit` in a session → auto-allowed, no prompt.
- Add a `deny` rule → auto-denied.
- A command matching neither → prompt appears (ask).
- Confirm `defaultMode: acceptEdits` auto-allows Edit/Write.

**Ship:** commit `feat(permissions): evaluate user's Claude policy for allow/deny/ask fast path`.

---

## Phase 3 — Normalized multi-source request/response contract ⭐

**Goal:** restructure the request/decision types around a `source` field + a
shared contract, with per-source parse (`build`) and render functions — preloop's
`buildPermissionRequest` / `renderHookDecision` pattern. Even if we only wire
Claude today, this makes Cursor/Codex a small adapter later, not a rewrite.

### What to port (from `agents_permission_hook.go`)

- Shared request: `{ source, tool_name, tool_input, session_id, cwd,
  agent_reasoning, client_decision }` (Go lines 41-49). Our `HookToolRequest`
  (`server.rs:29-41`) is close — add `source` and `client_decision`, and a
  normalized `agent_reasoning`.
- `build_permission_request(source, raw_event)` — a `match source { ClaudeCode
  => …, CodexCli => …, Cursor => … }` that maps each platform's native event
  shape into the shared request (Go lines 200-243). Implement ClaudeCode fully;
  stub Codex/Cursor with `todo!()`-style "not yet wired" that still compiles.
- `render_hook_decision(source, decision)` — maps the decision back to each
  platform's native response (Go lines 360-403):
  - Claude: `hookSpecificOutput.permissionDecision` (allow/deny) + reason.
  - Codex: `hookSpecificOutput.decision.behavior` (+ message on deny).
  - Cursor: `{"permission": "allow"|"deny", ...}` (deny is the only reliably
    enforced verdict — note this).
- `normalize_permission_source` + a `PermissionSource` enum
  (`ClaudeCode | CodexCli | Cursor`).

### Implementation steps

1. New `src-tauri/src/permissions/sources.rs`: `PermissionSource` enum,
   `build_permission_request`, `render_hook_decision`, `normalize_source`.
2. Thread `source` through: the bridge binary (Phase 1) learns a `--source`
   flag (default `claude_code`); the socket protocol carries it; `decide()` and
   the UI payload (`PendingPermission`) carry it so the Center can show which
   platform a request came from.
3. `PendingPermission` (`server.rs:54-63`) + frontend `types.ts` +
   `PermissionCenter.tsx`: add a `source` badge.

### Test

- Claude path unchanged end-to-end (regression check).
- Unit-test `build_permission_request` + `render_hook_decision` for all three
  sources with sample event JSON (copy shapes from the Go tests:
  `agents_permission_hook_test.go`, `claude_permission_policy_test.go`).

**Ship:** commit `feat(permissions): normalized multi-source request/decision contract`.

---

## Phase 4 — Persist the queue + decouple server lifecycle ⭐

**Goal:** stop losing state when the app restarts, and stop welding the decision
server to the Tauri window. Preloop's server is a persistent backend; ours can be
a persistent local daemon/state store without going full client-server.

### Scope (keep it local — we are NOT building a cloud backend)

1. **Persist pending + resolved permissions** to SQLite
   (`~/.claude-deck/permissions.db` or app data dir). On decision, record
   `{request_id, source, session_id, tool, input(masked), decision, reason,
   ts_requested, ts_resolved}`. This gives an **audit trail** (preloop has one)
   and survives restart.
2. **On app start, reload any still-pending requests** from the DB into the
   in-memory `pending` map IF their bridge process is still blocking (the socket
   connection is the source of truth; a request whose bridge already timed out is
   dead — mark it expired). Realistically: pending requests don't survive a full
   restart because the bridge's socket connection dies with the app; so the
   value here is the **audit log + scoped-allow persistence**, not resurrecting
   in-flight prompts. Persist `scoped_allows` so "allow for session" survives.
3. **Explicit fail policy config** (`fail_open: bool`, per-source
   `tool_approval_enabled`) — a small `~/.claude-deck/config.json`, read at start.
   Mirrors preloop's `ControlConfig` (`openclaw-preloop/src/index.ts:7-25`).
4. (Optional, larger) Move the socket server into a **standalone daemon** the
   Tauri app talks to, so permissions keep working even when the UI window is
   closed. Only do this if there's appetite — it's the biggest change. Document
   as a stretch goal; Phases 1-3 already remove the acute pain.

### Test

- Approve some requests, restart app, confirm the audit history is visible in the
  Permission Center (new "History" view or reuse existing history tab).
- "Allow for session" persists across an app restart for the same session id.
- Set `fail_open: false` in config, kill app, confirm bridge denies (fail-closed)
  with a clear reason; set `true`, confirm allow.

**Ship:** commit(s) — split DB persistence and config into separate commits.

---

## Phase 5 — Multi-platform adapters (Cursor, Codex) — the payoff

**Goal:** actually wire the second and third platforms using the Phase 3 seams,
proving the abstraction. Only start once Phases 1-4 are solid.

### Steps

1. **Codex CLI:** its `PermissionRequest` hook only fires for would-prompt calls,
   so no client_decision needed (preloop treats all as "ask" — see
   `agents_permission_hook.go:219-223`). Add the Codex event-shape parse + render
   (`hookSpecificOutput.decision.behavior`). Install into Codex's config
   (research where Codex reads hooks — check
   `../refs/preloop/cli/internal/cmd/agents.go` and `agents_runtime_install.go`
   for how preloop onboards each).
2. **Cursor:** `beforeShellExecution` (bare command, no tool name → map to
   `Shell`) and `beforeMCPExecution` (tool_name + tool_input). Render
   `{"permission":"deny", agent_message, user_message}` — remember deny is the
   only reliably enforced verdict; allow is best-effort (Cursor's own allowlist
   can override). See `agents_permission_hook.go:224-236, 389-399`.
3. Each platform = a bridge `--source` value + a `build`/`render` arm + an
   install routine. No changes to the core `decide()` path.

### Test

- Run Cursor and Codex sessions, trigger a tool call each, confirm both surface
  in the same Permission Center with correct source badges and that deny is
  enforced.

**Ship:** one commit per platform.

---

## Cross-cutting: version compatibility checks (the user asked for this)

Before/at each phase, verify against the versions actually in use:

- **Claude Code hook schema.** Confirm the `PreToolUse` command-hook contract
  (STDIN event fields: `session_id`, `transcript_path`, `cwd`, `permission_mode`,
  `hook_event_name`, `tool_name`, `tool_input`, `tool_use_id`; STDOUT
  `hookSpecificOutput.permissionDecision`) still matches the installed Claude
  Code version. Cross-check against the reference clone at
  `../refs/claude-code` (anthropics/claude-code) — grep its docs/plugins for
  `PreToolUse` and `permissionDecision`. Our current `HookToolRequest`
  (`server.rs:29-41`) encodes today's shape.
- **`type: command` hook support + `timeout` semantics** in the installed Claude
  version — confirm 300s timeout is honored and stdout JSON is parsed.
- **`settings.local.json` precedence** (Phase 2) — confirm Claude still merges it
  the way `claude_permission_policy.go` assumes.
- **Tauri 2** APIs used for the unix socket / bin target — confirm against the
  `@tauri-apps/*` versions in `package.json` (currently `^2.0.0`) and the Rust
  `tauri` crate in `src-tauri/Cargo.toml`.
- **Codex / Cursor hook schemas** (Phase 5) — these change; verify against each
  tool's current docs, not just preloop's snapshot (preloop's clone is a point in
  time; note its `VERSION` file).
- Keep `../refs/preloop` and `../refs/claude-code` **updated** (`git -C <ref>
  pull --ff-only origin main`) before relying on them for a schema check.

---

## File-by-file change map (quick reference)

| File | Phase | Change |
|---|---|---|
| `src-tauri/src/bin/claude-deck-hook.rs` | 1 | NEW: stateless bridge binary |
| `src-tauri/Cargo.toml` | 1 | add `[[bin]]` target |
| `src-tauri/src/permissions/server.rs` | 1,2,3 | factor `decide()`, add unix-socket listener, `source` in payload |
| `src-tauri/src/permissions/global_settings.rs` | 1 | write `command` hook, extend `is_ours` |
| `src-tauri/src/permissions/settings.rs` | 1 | per-run temp file → command hook |
| `src-tauri/src/permissions/claude_policy.rs` | 2 | NEW: port `claude_permission_policy.go` |
| `src-tauri/src/permissions/safe_bash.rs` | 2 | demote to secondary or retire |
| `src-tauri/src/permissions/sources.rs` | 3 | NEW: `PermissionSource`, build/render |
| `src-tauri/src/permissions/mod.rs` | 1-3 | export new modules |
| `src-tauri/src/permissions/store.rs` | 4 | NEW: SQLite persistence + audit |
| `src-tauri/src/permissions/config.rs` | 4 | NEW: fail policy config |
| `src-tauri/src/lib.rs` | 1,4 | socket server startup, DB init |
| `src/types/index.ts` | 3 | `source` on PendingPermission |
| `src/components/permissions/PermissionCenter.tsx` | 3,4 | source badge, history view |

---

## Recommended execution order for the next session(s)

1. **Phase 0** (baseline commit) — 15 min, do immediately.
2. **Phase 1** (command bridge) — biggest bang, fixes the active pain. Ship + live-test the crash path.
3. **Phase 2** (policy eval) — big UX win (fewer prompts), self-contained.
4. **Phase 3** (source contract) — enables everything after.
5. **Phase 4** (persistence/config) — robustness.
6. **Phase 5** (Cursor/Codex) — the multi-platform payoff.

Stop after any phase if context runs low; each leaves a shippable app.

---

## State of the world at handoff time (2026-07-15)

- Working dir: `/Users/devrev/git_personal_projs/claude-deck` (this is the
  canonical copy; `~/claude-deck` is an older duplicate with the SAME uncommitted
  work — safe to delete once this copy is verified building).
- Reference clones: `/Users/devrev/git_personal_projs/refs/{claude-code,
  agent-vault,preloop,sandcastle}` — all pulled to latest main 2026-07-15.
- HEAD = `23636ee`; the entire permission hub is **uncommitted working-tree
  changes** → Phase 0's first job is to commit them.
- Known recurring chore until Phase 1 lands: every time the dev app is killed
  (not cleanly quit), remove the dangling hook from `~/.claude/settings.json`
  (the block tagged `"claude-deck": true` / URL `http://127.0.0.1:198xx/...`).
  A backup was made at `~/.claude/settings.json.bak.claudedeck-20260714`.
