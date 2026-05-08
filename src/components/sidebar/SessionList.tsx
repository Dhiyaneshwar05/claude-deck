import { useMemo } from "react";
import { useAppStore } from "../../stores/appStore";
import { StatusDot } from "../shared/StatusDot";
import { formatUptime, formatEntrypoint } from "../../lib/format";
import type { DiscoveredSession, HubSession, SessionGroup } from "../../types";
import { Folder, Terminal, Lightning } from "@phosphor-icons/react";

function groupByProject(sessions: DiscoveredSession[]): SessionGroup[] {
  const map = new Map<string, DiscoveredSession[]>();
  for (const s of sessions) {
    const group = map.get(s.project_name) || [];
    group.push(s);
    map.set(s.project_name, group);
  }
  return Array.from(map.entries())
    .map(([project_name, sessions]) => ({ project_name, sessions }))
    .sort((a, b) => {
      // Groups with alive sessions first
      const aAlive = a.sessions.some((s) => s.is_alive);
      const bAlive = b.sessions.some((s) => s.is_alive);
      if (aAlive !== bAlive) return bAlive ? 1 : -1;
      return a.project_name.localeCompare(b.project_name);
    });
}

function SessionItem({ session }: { session: DiscoveredSession }) {
  const activeSessionId = useAppStore((s) => s.activeSessionId);
  const selectSession = useAppStore((s) => s.selectSession);
  const isActive = activeSessionId === session.session_id;

  return (
    <button
      onClick={() =>
        selectSession(isActive ? null : session.session_id)
      }
      className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-left transition-colors ${
        isActive
          ? "bg-zinc-800 text-zinc-100"
          : "text-zinc-400 hover:bg-zinc-800/50 hover:text-zinc-300"
      }`}
    >
      <StatusDot alive={session.is_alive} />
      <div className="flex-1 min-w-0">
        <div className="text-sm truncate">{session.title}</div>
        <div className="flex items-center gap-2 text-xs text-zinc-500">
          <span>{formatEntrypoint(session.entrypoint)}</span>
          {session.is_alive && (
            <>
              <span>&middot;</span>
              <span>{formatUptime(session.uptime_secs)}</span>
            </>
          )}
        </div>
      </div>
    </button>
  );
}

function HubSessionItem({ hs }: { hs: HubSession }) {
  const activeSessionId = useAppStore((s) => s.activeSessionId);
  const selectSession = useAppStore((s) => s.selectSession);
  const isActive = activeSessionId === hs.session_id;

  const isAlive =
    hs.status === "running" ||
    hs.status === "connecting" ||
    hs.status === "completed";

  const statusText =
    hs.status === "connecting"
      ? "Starting..."
      : hs.status === "running"
        ? "Running"
        : hs.status === "completed"
          ? "Done"
          : hs.status === "failed"
            ? "Failed"
            : "Exited";

  // Use first user message as title
  const title =
    hs.messages.find((m) => m.role === "user")?.content.slice(0, 40) ||
    "New session";

  return (
    <button
      onClick={() => selectSession(isActive ? null : hs.session_id)}
      className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-left transition-colors ${
        isActive
          ? "bg-zinc-800 text-zinc-100"
          : "text-zinc-400 hover:bg-zinc-800/50 hover:text-zinc-300"
      }`}
    >
      <StatusDot alive={isAlive} />
      <div className="flex-1 min-w-0">
        <div className="text-sm truncate">{title}</div>
        <div className="flex items-center gap-2 text-xs text-zinc-500">
          <span>{statusText}</span>
          {hs.messages.length > 0 && (
            <>
              <span>&middot;</span>
              <span>{hs.messages.length} msgs</span>
            </>
          )}
        </div>
      </div>
    </button>
  );
}

export function SessionList() {
  const sessions = useAppStore((s) => s.sessions);
  const hubSessions = useAppStore((s) => s.hubSessions);
  const isLoading = useAppStore((s) => s.isLoading);
  const groups = useMemo(() => groupByProject(sessions), [sessions]);
  const hubList = Object.values(hubSessions);

  if (isLoading && hubList.length === 0) {
    return (
      <div className="px-3 py-6 text-center text-zinc-500 text-sm">
        Scanning sessions...
      </div>
    );
  }

  if (groups.length === 0 && hubList.length === 0) {
    return (
      <div className="px-3 py-6 text-center text-zinc-600 text-sm">
        <Terminal size={24} className="mx-auto mb-2 text-zinc-700" />
        No sessions found
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-1">
      {/* Hub-spawned sessions */}
      {hubList.length > 0 && (
        <div>
          <div className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-zinc-500 uppercase tracking-wider">
            <Lightning size={14} weight="fill" className="text-emerald-500" />
            Hub Sessions
            <span className="text-zinc-700 ml-auto">{hubList.length}</span>
          </div>
          {hubList.map((hs) => (
            <HubSessionItem key={hs.session_id} hs={hs} />
          ))}
        </div>
      )}

      {/* Discovered sessions */}
      {groups.map((group) => (
        <div key={group.project_name}>
          <div className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-zinc-500 uppercase tracking-wider">
            <Folder size={14} weight="fill" className="text-zinc-600" />
            {group.project_name}
            <span className="text-zinc-700 ml-auto">
              {group.sessions.filter((s) => s.is_alive).length}/
              {group.sessions.length}
            </span>
          </div>
          {group.sessions.map((session) => (
            <SessionItem key={session.session_id} session={session} />
          ))}
        </div>
      ))}
    </div>
  );
}
