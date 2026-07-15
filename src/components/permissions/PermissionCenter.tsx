import { useEffect, useRef, useState } from "react";
import {
  ShieldCheck,
  ShieldWarning,
  Check,
  X,
  Lightning,
  Folder,
} from "@phosphor-icons/react";
import { useAppStore } from "../../stores/appStore";
import type { PendingPermission } from "../../types";

function formatToolInput(toolName: string, input: unknown): string {
  if (typeof input !== "object" || input === null) return String(input);
  const obj = input as Record<string, unknown>;

  if (toolName === "Bash" && typeof obj.command === "string") {
    return obj.command as string;
  }
  if (
    (toolName === "Edit" || toolName === "Write" || toolName === "MultiEdit") &&
    typeof obj.file_path === "string"
  ) {
    return obj.file_path as string;
  }
  if (toolName === "WebFetch" && typeof obj.url === "string") {
    return obj.url as string;
  }
  return JSON.stringify(obj).slice(0, 300);
}

function toolColor(toolName: string): string {
  if (toolName === "Bash") return "text-amber-400";
  if (toolName.startsWith("mcp__")) return "text-violet-400";
  if (toolName === "WebFetch") return "text-sky-400";
  return "text-rose-400";
}

function shortCwd(cwd: string): string {
  const parts = cwd.split("/").filter(Boolean);
  if (parts.length <= 2) return cwd;
  return `…/${parts.slice(-2).join("/")}`;
}

function elapsed(receivedAt: number, now: number): string {
  const secs = Math.max(0, Math.floor((now - receivedAt) / 1000));
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  return `${mins}m ago`;
}

function PermissionRow({
  perm,
  now,
}: {
  perm: PendingPermission;
  now: number;
}) {
  const decide = useAppStore((s) => s.decidePermission);
  const preview = formatToolInput(perm.tool_name, perm.tool_input);
  const allowSessionAvailable = perm.tool_name !== "Bash";

  return (
    <div className="border-b border-zinc-800/60 last:border-b-0 p-4">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2 min-w-0">
          <span
            className={`text-sm font-semibold ${toolColor(perm.tool_name)}`}
          >
            {perm.tool_name}
          </span>
          <span className="text-[10px] text-zinc-600">
            {elapsed(perm.received_at, now)}
          </span>
        </div>
      </div>

      <div className="flex items-center gap-1 text-[11px] text-zinc-500 font-mono mb-2 min-w-0">
        <Folder size={11} className="shrink-0" />
        <span className="truncate" title={perm.cwd}>
          {shortCwd(perm.cwd)}
        </span>
      </div>

      <div className="bg-zinc-950/60 border border-zinc-800 rounded-md p-2.5 mb-3 max-h-32 overflow-y-auto">
        <pre className="text-xs text-zinc-300 font-mono whitespace-pre-wrap break-all">
          {preview}
        </pre>
      </div>

      <div className="flex items-center gap-2">
        <button
          onClick={() => decide(perm.request_id, "deny")}
          className="flex items-center justify-center gap-1.5 px-2.5 py-1.5 rounded-md bg-rose-600/90 hover:bg-rose-600 text-white text-[11px] font-medium transition-colors"
        >
          <X size={11} weight="bold" />
          Deny
        </button>
        <div className="flex-1" />
        {allowSessionAvailable && (
          <button
            onClick={() => decide(perm.request_id, "allow-session")}
            className="flex items-center justify-center gap-1.5 px-2.5 py-1.5 rounded-md bg-zinc-800 hover:bg-zinc-700 text-zinc-200 text-[11px] font-medium transition-colors"
            title="Allow this tool for the rest of the session"
          >
            <Lightning size={11} weight="fill" />
            Session
          </button>
        )}
        <button
          onClick={() => decide(perm.request_id, "allow")}
          className="flex items-center justify-center gap-1.5 px-2.5 py-1.5 rounded-md bg-emerald-600 hover:bg-emerald-500 text-white text-[11px] font-medium transition-colors"
        >
          <Check size={11} weight="bold" />
          Allow
        </button>
      </div>
    </div>
  );
}

function scopedAllowLabel(key: string): string {
  // key shape: "session:<id>:tool:<Tool>"
  const m = key.match(/^session:(.*):tool:(.*)$/);
  if (!m) return key;
  const sid = m[1].length > 8 ? `${m[1].slice(0, 8)}…` : m[1];
  return `${m[2]} · ${sid}`;
}

export function PermissionCenter() {
  const pending = useAppStore((s) => s.pendingPermissions);
  const scopedAllows = useAppStore((s) => s.scopedAllows);
  const refreshScopedAllows = useAppStore((s) => s.refreshScopedAllows);
  const revokeScopedAllow = useAppStore((s) => s.revokeScopedAllow);
  const list = Object.values(pending).sort(
    (a, b) => a.received_at - b.received_at,
  );
  const count = list.length;

  const [open, setOpen] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const containerRef = useRef<HTMLDivElement>(null);

  // Tick "elapsed" labels every second while panel is open
  useEffect(() => {
    if (!open) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [open]);

  // Load active session-allows whenever the panel opens
  useEffect(() => {
    if (open) refreshScopedAllows();
  }, [open, refreshScopedAllows]);

  // Auto-open when a new permission arrives and we currently have none open
  const prevCount = useRef(count);
  useEffect(() => {
    if (count > prevCount.current && !open) {
      setOpen(true);
    }
    prevCount.current = count;
  }, [count, open]);

  // Close on click outside
  useEffect(() => {
    if (!open) return;
    function onMouseDown(e: MouseEvent) {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    window.addEventListener("mousedown", onMouseDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onMouseDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const hasPending = count > 0;

  return (
    <div ref={containerRef} className="fixed top-3 right-4 z-50">
      <button
        onClick={() => setOpen((v) => !v)}
        className={`relative flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border text-xs font-medium transition-colors ${
          hasPending
            ? "bg-amber-500/15 border-amber-500/40 text-amber-300 hover:bg-amber-500/25"
            : "bg-zinc-900/70 border-zinc-800 text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/70"
        }`}
        title={
          hasPending
            ? `${count} pending permission${count === 1 ? "" : "s"}`
            : "No pending permissions"
        }
      >
        {hasPending ? (
          <ShieldWarning size={14} weight="fill" />
        ) : (
          <ShieldCheck size={14} weight="regular" />
        )}
        <span>Permissions</span>
        {hasPending && (
          <span className="ml-0.5 inline-flex items-center justify-center min-w-[18px] h-[18px] px-1 rounded-full bg-amber-400 text-[10px] font-bold text-zinc-950">
            {count}
          </span>
        )}
      </button>

      {open && (
        <div className="absolute right-0 mt-2 w-[440px] max-h-[70vh] flex flex-col bg-zinc-900 border border-zinc-800 rounded-xl shadow-2xl shadow-black/60 overflow-hidden">
          <div className="flex items-center justify-between px-4 py-3 border-b border-zinc-800/80">
            <div className="flex items-center gap-2">
              <ShieldWarning
                size={14}
                weight="fill"
                className="text-amber-400"
              />
              <span className="text-xs font-semibold uppercase tracking-wider text-zinc-300">
                Permission Center
              </span>
            </div>
            <span className="text-[10px] text-zinc-500">
              {count} pending
            </span>
          </div>

          <div className="overflow-y-auto flex-1">
            {list.length === 0 ? (
              <div className="px-6 py-12 text-center">
                <ShieldCheck
                  size={28}
                  className="mx-auto mb-2 text-zinc-700"
                  weight="regular"
                />
                <div className="text-sm text-zinc-500">All clear</div>
                <div className="text-[11px] text-zinc-600 mt-1">
                  Permission requests from any spawned session will appear here.
                </div>
              </div>
            ) : (
              list.map((perm) => (
                <PermissionRow key={perm.request_id} perm={perm} now={now} />
              ))
            )}
          </div>

          {scopedAllows.length > 0 && (
            <div className="border-t border-zinc-800/80 px-4 py-3">
              <div className="flex items-center justify-between mb-2">
                <span className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                  Active session allows ({scopedAllows.length})
                </span>
                <button
                  onClick={() => revokeScopedAllow()}
                  className="text-[10px] text-rose-400 hover:text-rose-300"
                >
                  Revoke all
                </button>
              </div>
              <div className="flex flex-col gap-1 max-h-32 overflow-y-auto">
                {scopedAllows.map((key) => (
                  <div
                    key={key}
                    className="flex items-center justify-between gap-2 text-[11px] text-zinc-400 font-mono"
                  >
                    <span className="truncate" title={key}>
                      {scopedAllowLabel(key)}
                    </span>
                    <button
                      onClick={() => revokeScopedAllow(key)}
                      className="shrink-0 text-zinc-600 hover:text-rose-400"
                      title="Revoke this allow"
                    >
                      <X size={11} weight="bold" />
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
