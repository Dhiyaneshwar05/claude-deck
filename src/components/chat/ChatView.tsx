import { useEffect, useRef } from "react";
import { useAppStore } from "../../stores/appStore";
import { StatusDot } from "../shared/StatusDot";
import { MessageBubble } from "./MessageBubble";
import { ToolCallCard } from "./ToolCallCard";
import { InputBar } from "../input/InputBar";
import {
  formatUptime,
  formatEntrypoint,
  formatTokens,
  formatModel,
} from "../../lib/format";
import {
  Terminal,
  Folder,
  Clock,
  Cpu,
  IdentificationBadge,
  GitBranch,
  Brain,
  ChatCircleDots,
  Lightning,
  X,
} from "@phosphor-icons/react";

// ── Empty state (no session selected) ─────────────────────

function EmptyState() {
  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 flex flex-col items-center justify-center text-zinc-600 gap-3">
        <div className="w-16 h-16 rounded-2xl bg-zinc-900 flex items-center justify-center">
          <Terminal size={32} weight="duotone" className="text-zinc-700" />
        </div>
        <div className="text-center">
          <p className="text-sm font-medium text-zinc-500">
            No session selected
          </p>
          <p className="text-xs mt-1">
            Select a session from the sidebar, or start a new one below
          </p>
        </div>
      </div>
      <InputBar />
    </div>
  );
}

// ── Metadata helpers (for discovered sessions) ────────────

function MetaRow({
  icon: Icon,
  label,
  value,
}: {
  icon: React.ComponentType<{ size: number; className?: string }>;
  label: string;
  value: string;
}) {
  return (
    <div className="flex items-center gap-3 py-2">
      <Icon size={16} className="text-zinc-600 shrink-0" />
      <span className="text-xs text-zinc-500 w-24">{label}</span>
      <span className="text-sm text-zinc-300 font-mono">{value}</span>
    </div>
  );
}

function TokenStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="text-center">
      <div className="text-lg font-semibold text-zinc-200 font-mono">
        {value}
      </div>
      <div className="text-xs text-zinc-500 mt-0.5">{label}</div>
    </div>
  );
}

// ── Discovered session details panel ──────────────────────

function DiscoveredSessionView() {
  const activeSessionId = useAppStore((s) => s.activeSessionId);
  const selectSession = useAppStore((s) => s.selectSession);
  const sessions = useAppStore((s) => s.sessions);
  const session = sessions.find((s) => s.session_id === activeSessionId);

  if (!session) return <EmptyState />;

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center gap-3 px-6 py-4 border-b border-zinc-800/50">
        <StatusDot alive={session.is_alive} size="md" />
        <div className="min-w-0 flex-1">
          <h2 className="text-sm font-semibold text-zinc-200 truncate">
            {session.title}
          </h2>
          <span className="text-xs text-zinc-500">
            {session.project_name} &middot;{" "}
            {session.is_alive ? "Active" : "Ended"} &middot; PID {session.pid}
          </span>
        </div>
        <button
          onClick={() => selectSession(null)}
          className="p-1.5 rounded-md text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 transition-colors"
        >
          <X size={14} />
        </button>
      </div>

      {/* Session metadata */}
      <div className="flex-1 overflow-y-auto px-6 py-6">
        <div className="max-w-lg space-y-4">
          <div>
            <h3 className="text-xs font-medium text-zinc-500 uppercase tracking-wider mb-3">
              Session Info
            </h3>
            <div className="bg-zinc-900/50 rounded-xl p-4 border border-zinc-800/50 divide-y divide-zinc-800/50">
              <MetaRow
                icon={IdentificationBadge}
                label="Session ID"
                value={session.session_id.slice(0, 16) + "..."}
              />
              <MetaRow icon={Folder} label="Working Dir" value={session.cwd} />
              <MetaRow
                icon={Cpu}
                label="Entrypoint"
                value={formatEntrypoint(session.entrypoint)}
              />
              {session.git_branch && (
                <MetaRow
                  icon={GitBranch}
                  label="Git Branch"
                  value={session.git_branch}
                />
              )}
              <MetaRow
                icon={Clock}
                label="Uptime"
                value={
                  session.is_alive
                    ? formatUptime(session.uptime_secs)
                    : "Session ended"
                }
              />
            </div>
          </div>

          <div>
            <h3 className="text-xs font-medium text-zinc-500 uppercase tracking-wider mb-3">
              Conversation
            </h3>
            <div className="bg-zinc-900/50 rounded-xl p-4 border border-zinc-800/50 divide-y divide-zinc-800/50">
              <MetaRow
                icon={Brain}
                label="Model"
                value={formatModel(session.model)}
              />
              <MetaRow
                icon={ChatCircleDots}
                label="Messages"
                value={`${session.message_count} turns`}
              />
            </div>
          </div>

          {session.total_input_tokens + session.total_output_tokens > 0 && (
            <div>
              <h3 className="text-xs font-medium text-zinc-500 uppercase tracking-wider mb-3">
                Token Usage
              </h3>
              <div className="bg-zinc-900/50 rounded-xl p-4 border border-zinc-800/50">
                <div className="grid grid-cols-3 gap-4">
                  <TokenStat
                    label="Input"
                    value={formatTokens(session.total_input_tokens)}
                  />
                  <TokenStat
                    label="Output"
                    value={formatTokens(session.total_output_tokens)}
                  />
                  <TokenStat
                    label="Cache Read"
                    value={formatTokens(session.total_cache_read_tokens)}
                  />
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ── Hub session chat view ─────────────────────────────────

function HubSessionChat() {
  const activeSessionId = useAppStore((s) => s.activeSessionId);
  const selectSession = useAppStore((s) => s.selectSession);
  const hubSessions = useAppStore((s) => s.hubSessions);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const hs = activeSessionId ? hubSessions[activeSessionId] : null;

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [hs?.messages.length, hs?.messages[hs.messages.length - 1]?.content]);

  if (!hs) return <EmptyState />;

  const statusColor =
    hs.status === "running" || hs.status === "connecting"
      ? "text-emerald-400"
      : hs.status === "completed"
        ? "text-zinc-400"
        : hs.status === "failed" || hs.status === "dead"
          ? "text-red-400"
          : "text-zinc-500";

  const statusLabel =
    hs.status === "connecting"
      ? "Connecting..."
      : hs.status === "running"
        ? "Running"
        : hs.status === "completed"
          ? "Completed"
          : hs.status === "failed"
            ? "Failed"
            : hs.status === "dead"
              ? "Process exited"
              : "Idle";

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center gap-3 px-6 py-3 border-b border-zinc-800/50">
        <Lightning
          size={18}
          weight="fill"
          className="text-emerald-400 shrink-0"
        />
        <div className="min-w-0 flex-1">
          <h2 className="text-sm font-semibold text-zinc-200 truncate">
            Hub Session
          </h2>
          <div className="flex items-center gap-2 text-xs text-zinc-500">
            <span className={statusColor}>{statusLabel}</span>
            {hs.model && (
              <>
                <span>&middot;</span>
                <span>{formatModel(hs.model)}</span>
              </>
            )}
            {hs.cost_usd > 0 && (
              <>
                <span>&middot;</span>
                <span>${hs.cost_usd.toFixed(4)}</span>
              </>
            )}
            <span>&middot;</span>
            <span className="font-mono text-[10px]">
              {hs.session_id.slice(0, 12)}...
            </span>
          </div>
        </div>
        <button
          onClick={() => selectSession(null)}
          className="p-1.5 rounded-md text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 transition-colors"
        >
          <X size={14} />
        </button>
      </div>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto py-4">
        {hs.messages.map((msg) =>
          msg.role === "tool" ? (
            <ToolCallCard key={msg.id} message={msg} />
          ) : (
            <MessageBubble key={msg.id} message={msg} />
          ),
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* Input */}
      <InputBar />
    </div>
  );
}

// ── Main ChatView router ──────────────────────────────────

export function ChatView() {
  const activeSessionId = useAppStore((s) => s.activeSessionId);
  const hubSessions = useAppStore((s) => s.hubSessions);

  if (!activeSessionId) return <EmptyState />;

  // Is this a hub-spawned session?
  if (activeSessionId in hubSessions) {
    return <HubSessionChat />;
  }

  // Otherwise it's a discovered session — show details panel
  return <DiscoveredSessionView />;
}
