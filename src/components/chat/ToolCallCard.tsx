import type { ChatMessage } from "../../types";
import { Wrench, CircleNotch, CheckCircle } from "@phosphor-icons/react";

export function ToolCallCard({ message }: { message: ChatMessage }) {
  const isRunning = message.tool_status === "running";

  return (
    <div className="mx-6 my-1.5 rounded-lg border border-zinc-800/60 bg-zinc-900/40 overflow-hidden">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 bg-zinc-900/60">
        {isRunning ? (
          <CircleNotch
            size={14}
            className="text-amber-400 animate-spin"
          />
        ) : (
          <CheckCircle size={14} className="text-emerald-400" />
        )}
        <Wrench size={12} className="text-zinc-500" />
        <span className="text-xs font-medium text-zinc-300">
          {message.tool_name}
        </span>
        <span className="text-[10px] text-zinc-600 ml-auto">
          {isRunning ? "running..." : "done"}
        </span>
      </div>

      {/* Tool input (collapsed preview) */}
      {message.tool_input && (
        <div className="px-3 py-2 border-t border-zinc-800/40">
          <pre className="text-[11px] text-zinc-500 font-mono whitespace-pre-wrap break-words max-h-24 overflow-y-auto">
            {message.tool_input.length > 500
              ? message.tool_input.slice(0, 500) + "..."
              : message.tool_input}
          </pre>
        </div>
      )}
    </div>
  );
}
