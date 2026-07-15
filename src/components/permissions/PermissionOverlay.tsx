import { useAppStore } from "../../stores/appStore";
import type { PendingPermission } from "../../types";
import {
  ShieldWarning,
  Check,
  X,
  Lightning,
} from "@phosphor-icons/react";

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

function PermissionCard({ perm }: { perm: PendingPermission }) {
  const decide = useAppStore((s) => s.decidePermission);
  const preview = formatToolInput(perm.tool_name, perm.tool_input);
  const allowSessionAvailable = perm.tool_name !== "Bash";

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded-2xl shadow-2xl shadow-black/50 w-[440px] p-5">
      <div className="flex items-center gap-2 mb-3">
        <ShieldWarning
          size={18}
          weight="fill"
          className="text-amber-400 shrink-0"
        />
        <div className="text-xs font-medium uppercase tracking-wider text-zinc-400">
          Permission required
        </div>
      </div>

      <div className="mb-3">
        <div className={`text-sm font-semibold ${toolColor(perm.tool_name)}`}>
          {perm.tool_name}
        </div>
        <div className="text-xs text-zinc-500 truncate font-mono mt-0.5">
          {perm.cwd}
        </div>
      </div>

      <div className="bg-zinc-950/60 border border-zinc-800 rounded-lg p-3 mb-4 max-h-48 overflow-y-auto">
        <pre className="text-xs text-zinc-300 font-mono whitespace-pre-wrap break-all">
          {preview}
        </pre>
      </div>

      <div className="flex items-center gap-2">
        <button
          onClick={() => decide(perm.request_id, "deny")}
          className="flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg bg-rose-600/90 hover:bg-rose-600 text-white text-xs font-medium transition-colors"
        >
          <X size={12} weight="bold" />
          Deny
        </button>
        <div className="flex-1" />
        {allowSessionAvailable && (
          <button
            onClick={() => decide(perm.request_id, "allow-session")}
            className="flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-200 text-xs font-medium transition-colors"
            title="Allow this tool for the rest of the session"
          >
            <Lightning size={12} weight="fill" />
            Allow session
          </button>
        )}
        <button
          onClick={() => decide(perm.request_id, "allow")}
          className="flex items-center justify-center gap-1.5 px-3 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-medium transition-colors"
        >
          <Check size={12} weight="bold" />
          Allow
        </button>
      </div>
    </div>
  );
}

export function PermissionOverlay() {
  const pending = useAppStore((s) => s.pendingPermissions);
  const list = Object.values(pending).sort(
    (a, b) => a.received_at - b.received_at,
  );
  if (list.length === 0) return null;

  const top = list[0];
  const extraCount = list.length - 1;

  return (
    <div className="fixed bottom-6 right-6 z-50 flex flex-col items-end gap-2">
      {extraCount > 0 && (
        <div className="text-xs text-zinc-500 bg-zinc-900/80 border border-zinc-800 rounded-full px-3 py-1">
          +{extraCount} more pending
        </div>
      )}
      <PermissionCard perm={top} />
    </div>
  );
}
