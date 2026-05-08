import { useAppStore } from "../../stores/appStore";
import { SessionList } from "./SessionList";
import {
  ChatsCircle,
  Robot,
  ClockCounterClockwise,
} from "@phosphor-icons/react";

const NAV_ITEMS = [
  { id: "sessions" as const, label: "Sessions", icon: ChatsCircle },
  { id: "agents" as const, label: "Agents", icon: Robot },
  { id: "history" as const, label: "History", icon: ClockCounterClockwise },
];

export function Sidebar() {
  const section = useAppStore((s) => s.sidebarSection);
  const setSection = useAppStore((s) => s.setSection);
  const sessions = useAppStore((s) => s.sessions);
  const aliveCount = sessions.filter((s) => s.is_alive).length;

  return (
    <aside className="flex flex-col h-full bg-zinc-950 border-r border-zinc-800/50">
      {/* Drag region + title */}
      <div
        data-tauri-drag-region
        className="flex items-center gap-2 px-4 pt-8 pb-3"
      >
        <h1 className="text-sm font-semibold text-zinc-300 tracking-tight">
          AGENT HUB
        </h1>
        {aliveCount > 0 && (
          <span className="text-[10px] font-medium bg-emerald-500/15 text-emerald-400 px-1.5 py-0.5 rounded-full">
            {aliveCount} live
          </span>
        )}
      </div>

      {/* Navigation tabs */}
      <div className="flex gap-1 px-3 pb-2">
        {NAV_ITEMS.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => setSection(id)}
            className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded-md text-xs font-medium transition-colors ${
              section === id
                ? "bg-zinc-800 text-zinc-200"
                : "text-zinc-500 hover:text-zinc-400 hover:bg-zinc-900"
            }`}
          >
            <Icon size={14} weight={section === id ? "fill" : "regular"} />
            {label}
          </button>
        ))}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-1 py-1">
        {section === "sessions" && <SessionList />}
        {section === "agents" && (
          <div className="px-3 py-6 text-center text-zinc-600 text-sm">
            <Robot size={24} className="mx-auto mb-2 text-zinc-700" />
            Agent profiles coming in Phase 2
          </div>
        )}
        {section === "history" && (
          <div className="px-3 py-6 text-center text-zinc-600 text-sm">
            <ClockCounterClockwise
              size={24}
              className="mx-auto mb-2 text-zinc-700"
            />
            Session history coming in Phase 2
          </div>
        )}
      </div>

      {/* Footer stats */}
      <div className="px-4 py-3 border-t border-zinc-800/50 text-xs text-zinc-600">
        {sessions.length} session{sessions.length !== 1 ? "s" : ""} discovered
      </div>
    </aside>
  );
}
