# Contributing to Claude Deck

Thanks for taking a look. This is an actively developed project and contributions —
bug fixes, features, docs, or just sharp design feedback on an issue — are genuinely
welcome. This guide covers how to get set up, how work is organized, and the
conventions a pull request is expected to follow.

---

## Getting set up

### Prerequisites

- **macOS** (Apple Silicon or Intel) — the app is macOS-only for now.
- [**Rust stable**](https://rustup.rs/) — `rustup default stable`
- **Node 20+** and npm
- **Xcode Command Line Tools** — `xcode-select --install`
- **Claude Code CLI** — `npm install -g @anthropic-ai/claude-code`, or your channel of choice. You'll want at least one real Claude Code session to exercise the Permission Hub end to end.

### Run it locally

```bash
npm install

# Dev mode with hot reload (frontend + Rust)
npm run tauri dev

# Rust-only check — much faster than a full build while iterating on the backend
cargo check --manifest-path src-tauri/Cargo.toml

# Full production build (.app + .dmg under src-tauri/target/release/bundle/)
npm run tauri build
```

> **Note on names:** the working tree and crate on disk are still `agent-hub`. The
> shipping product is **Claude Deck**. Don't rename the directory — it invalidates
> `target/` for no benefit.

A good first sanity check: launch the app, confirm it discovers your existing Claude
Code sessions in the sidebar, then trigger a permission (e.g. ask a session to run a
shell command) and confirm it surfaces in the Permission Center.

---

## Finding something to work on

- **New here?** Start with a [**`good first issue`**](../../issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22).
  These are scoped to have clear boundaries and not require deep context on the whole
  codebase.
- **Want the bigger picture?** [`docs/PHASES.md`](docs/PHASES.md) is the phase tracker,
  and [`docs/shipped-notes/`](docs/shipped-notes/) has plain-language write-ups of the
  major pieces already built (start there to understand *why* things are shaped the
  way they are).
- Browse [all open issues](../../issues). Labels tell you what kind of work each is:

| Label | Meaning |
|---|---|
| `good first issue` | Scoped and newcomer-friendly |
| `bug` | Something isn't working |
| `enhancement` | New feature or improvement |
| `tech-debt` | Cleanup, dead code, refactor |
| `security` | Security-relevant work or hardening |
| `documentation` | Docs improvements |
| `help wanted` | Extra attention wanted |

### Claiming an issue

Comment on the issue to say you're picking it up before you start — that avoids two
people doing the same work. If you want to propose something that isn't filed yet,
**open an issue first** so the approach can be discussed before you write code. For
anything non-trivial, agreeing on the shape in the issue saves a painful review later.

---

## Making changes

### Branches

Branch off `main`. Use a short, descriptive, kebab-case name prefixed by type:

```
feat/bulk-permission-actions
fix/stale-permission-ghost-card
docs/contributing-guide
chore/retire-tcp-server
```

### Commits

- Use [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): summary`,
  e.g. `fix(permissions): drop expired requests from the queue`.
- Keep commits focused — one logical change each. It makes review and `git bisect` sane.
- Write imperative, present-tense summaries ("add", not "added").

### Code style

- **Rust** — standard Tauri 2 / tokio patterns; commands return `Result<T, String>`;
  serialize with serde. Run `cargo fmt` and make sure `cargo check` is clean.
- **Frontend** — functional components, Zustand with selectors, Tailwind utility
  classes. TypeScript, no untyped `any` where it can be avoided.
- Match the surrounding code — its naming, comment density, and idioms — over any
  personal preference.

### Before you open a PR

- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` passes.
- [ ] `npm run tauri dev` runs and the feature/fix works in the real UI.
- [ ] You've manually verified the change end-to-end (see below) and noted what you
      checked in the PR description.

---

## Pull requests

Open the PR against `main` and link the issue it closes (`Closes #NN`). In the
description, include:

1. **What & why** — a short summary of the change and the problem it solves.
2. **How you verified it** — this project leans on manual, in-the-real-UI verification
   rather than an automated suite. Spell out the steps you actually ran and what you
   observed (e.g. "spawned 3 sessions, fired a Bash permission from each, confirmed all
   three queued and Deny/Allow resolved the right one"). A change to permission routing
   or the hook bridge especially needs this — describe the end-to-end path you exercised.
3. **Screenshots / recordings** for any UI change.

Reviewers may be invited to look over PRs. Expect a round or two of feedback — it's
about the code, not you. Keep the branch up to date with `main` and squash noise where
it helps readability.

---

## A note on the public repo

This is a public repository. Please don't create throwaway or test artifacts (dummy
issues, test PRs, scratch branches pushed to origin) against it — use your fork or a
local branch for experiments.

---

## License

By contributing, you agree that your contributions are licensed under the
[Apache License 2.0](LICENSE), the same as the project.
