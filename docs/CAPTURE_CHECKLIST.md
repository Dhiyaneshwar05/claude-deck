# Screenshot & Demo Checklist

Use this list to grab the visual assets referenced in [../README.md](../README.md). Drop files into `docs/images/` with the exact filenames below so the README renders without edits.

## Screenshots (PNG, 1440×900 display @ 2× is ideal)

- [ ] `sidebar.png` — sidebar with 3–5 Claude sessions discovered (mix of hub-spawned and external).
- [ ] `chat-view.png` — a session's chat pane with a few MessageBubble exchanges and at least one ToolCallCard rendered.
- [ ] `empty-state.png` — first-run empty state prompting the user to create/select a session.
- [ ] `health-polling.png` — close-up of the StatusDot indicator showing green/gray session health.

## Demo video / GIF

- [ ] `demo.gif` — 15–30 s screen recording: launch the app → sidebar populates with discovered sessions → click one → see event stream. Keep under 5 MB for inline GitHub rendering.

### Tooling suggestions

- macOS screen recording: `Shift+Cmd+5` → record selection → convert with `ffmpeg -i demo.mov -vf "fps=15,scale=900:-1:flags=lanczos" -loop 0 demo.gif` (or Gifski).
- Annotation: CleanShot X / Shottr for callouts.

## After capturing

```bash
git add docs/images docs/demo.gif
git commit -m "docs: add demo screenshots and recording"
git push
```
