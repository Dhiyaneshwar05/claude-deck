import { create } from "zustand";
import type {
  DiscoveredSession,
  HubSession,
  ChatMessage,
  SessionEvent,
  PendingPermission,
  PermissionDecision,
} from "../types";
import {
  listSessions,
  createSession,
  sendPrompt,
  cancelSession as cancelSessionIpc,
  resolvePermission as resolvePermissionIpc,
  listScopedAllows,
  clearScopedAllows,
} from "../lib/tauri";

let msgCounter = 0;
function nextMsgId(): string {
  return `msg-${++msgCounter}-${Date.now()}`;
}

interface AppState {
  // Discovery
  sessions: DiscoveredSession[];
  isLoading: boolean;
  sidebarSection: "sessions" | "agents" | "history";

  // Active session (can be discovered or hub-spawned)
  activeSessionId: string | null;

  // Hub-spawned sessions (keyed by session_id)
  hubSessions: Record<string, HubSession>;

  // Pending permission requests (keyed by request_id)
  pendingPermissions: Record<string, PendingPermission>;

  // Active session-scoped allows (e.g. "session:<id>:tool:<Tool>")
  scopedAllows: string[];

  // Actions — discovery
  refreshSessions: () => Promise<void>;
  selectSession: (sessionId: string | null) => void;
  setSection: (section: AppState["sidebarSection"]) => void;

  // Actions — chat
  startSession: (cwd: string, prompt: string, model?: string) => Promise<void>;
  sendMessage: (prompt: string) => Promise<void>;
  cancelActiveSession: () => Promise<void>;
  handleSessionEvent: (payload: SessionEvent) => void;

  // Actions — permissions
  addPermission: (perm: PendingPermission) => void;
  decidePermission: (
    requestId: string,
    decision: PermissionDecision,
  ) => Promise<void>;
  refreshScopedAllows: () => Promise<void>;
  revokeScopedAllow: (key?: string) => Promise<void>;
}

export const useAppStore = create<AppState>((set, get) => ({
  sessions: [],
  isLoading: true,
  sidebarSection: "sessions",
  activeSessionId: null,
  hubSessions: {},
  pendingPermissions: {},
  scopedAllows: [],

  refreshSessions: async () => {
    try {
      const sessions = await listSessions();
      set((state) => {
        const activeStillExists = state.activeSessionId
          ? sessions.some((s) => s.session_id === state.activeSessionId) ||
            state.activeSessionId in state.hubSessions
          : true;
        return {
          sessions,
          isLoading: false,
          activeSessionId: activeStillExists ? state.activeSessionId : null,
        };
      });
    } catch {
      set({ isLoading: false });
    }
  },

  selectSession: (sessionId) => set({ activeSessionId: sessionId }),

  setSection: (section) => set({ sidebarSection: section }),

  startSession: async (cwd, prompt, model) => {
    try {
      const info = await createSession(cwd, prompt, model);

      const userMsg: ChatMessage = {
        id: nextMsgId(),
        role: "user",
        content: prompt,
        timestamp: Date.now(),
      };

      const hubSession: HubSession = {
        session_id: info.session_id,
        pid: info.pid,
        cwd: info.cwd,
        model: null,
        tools: [],
        status: "connecting",
        messages: [userMsg],
        cost_usd: 0,
        duration_ms: 0,
      };

      set((state) => ({
        hubSessions: {
          ...state.hubSessions,
          [info.session_id]: hubSession,
        },
        activeSessionId: info.session_id,
      }));
    } catch (err) {
      console.error("Failed to start session:", err);
    }
  },

  sendMessage: async (prompt) => {
    const { activeSessionId, hubSessions } = get();
    if (!activeSessionId || !(activeSessionId in hubSessions)) return;

    const userMsg: ChatMessage = {
      id: nextMsgId(),
      role: "user",
      content: prompt,
      timestamp: Date.now(),
    };

    // Optimistically add user message
    set((state) => {
      const hs = state.hubSessions[activeSessionId];
      if (!hs) return state;
      return {
        hubSessions: {
          ...state.hubSessions,
          [activeSessionId]: {
            ...hs,
            status: "running",
            messages: [...hs.messages, userMsg],
          },
        },
      };
    });

    try {
      await sendPrompt(activeSessionId, prompt);
    } catch (err) {
      console.error("Failed to send prompt:", err);
    }
  },

  cancelActiveSession: async () => {
    const { activeSessionId, hubSessions } = get();
    if (!activeSessionId || !(activeSessionId in hubSessions)) return;
    try {
      await cancelSessionIpc(activeSessionId);
    } catch (err) {
      console.error("Failed to cancel session:", err);
    }
  },

  addPermission: (perm) =>
    set((state) => ({
      pendingPermissions: {
        ...state.pendingPermissions,
        [perm.request_id]: perm,
      },
    })),

  decidePermission: async (requestId, decision) => {
    const perm = get().pendingPermissions[requestId];
    if (!perm) return;
    // Optimistically drop it from the queue
    set((state) => {
      const next = { ...state.pendingPermissions };
      delete next[requestId];
      return { pendingPermissions: next };
    });
    try {
      await resolvePermissionIpc(requestId, perm.run_token, decision);
      if (decision === "allow-session") {
        await get().refreshScopedAllows();
      }
    } catch (err) {
      console.error("Failed to resolve permission:", err);
      // Put it back so the user can retry
      set((state) => ({
        pendingPermissions: { ...state.pendingPermissions, [requestId]: perm },
      }));
    }
  },

  refreshScopedAllows: async () => {
    try {
      const allows = await listScopedAllows();
      set({ scopedAllows: allows });
    } catch (err) {
      console.error("Failed to list scoped allows:", err);
    }
  },

  revokeScopedAllow: async (key) => {
    try {
      await clearScopedAllows(key);
      await get().refreshScopedAllows();
    } catch (err) {
      console.error("Failed to revoke scoped allow:", err);
    }
  },

  handleSessionEvent: (payload) => {
    const { session_id, event } = payload;

    set((state) => {
      const hs = state.hubSessions[session_id];
      if (!hs) return state;

      const updated = { ...hs, messages: [...hs.messages] };

      switch (event.type) {
        case "session_init":
          updated.status = "running";
          updated.model = event.model;
          updated.tools = event.tools;
          break;

        case "text_chunk": {
          const last = updated.messages[updated.messages.length - 1];
          if (last?.role === "assistant" && !last.tool_name) {
            // Append to existing assistant message
            updated.messages[updated.messages.length - 1] = {
              ...last,
              content: last.content + event.text,
            };
          } else {
            // Start new assistant message
            updated.messages.push({
              id: nextMsgId(),
              role: "assistant",
              content: event.text,
              timestamp: Date.now(),
            });
          }
          updated.status = "running";
          break;
        }

        case "tool_call":
          updated.messages.push({
            id: nextMsgId(),
            role: "tool",
            content: "",
            tool_name: event.tool_name,
            tool_id: event.tool_id,
            tool_input: "",
            tool_status: "running",
            timestamp: Date.now(),
          });
          break;

        case "tool_call_update": {
          // Find the last running tool and append input
          for (let i = updated.messages.length - 1; i >= 0; i--) {
            const msg = updated.messages[i];
            if (msg.role === "tool" && msg.tool_status === "running") {
              updated.messages[i] = {
                ...msg,
                tool_input: (msg.tool_input || "") + event.partial_input,
              };
              break;
            }
          }
          break;
        }

        case "tool_call_complete": {
          // Mark the last running tool as completed
          for (let i = updated.messages.length - 1; i >= 0; i--) {
            const msg = updated.messages[i];
            if (msg.role === "tool" && msg.tool_status === "running") {
              updated.messages[i] = { ...msg, tool_status: "completed" };
              break;
            }
          }
          break;
        }

        case "task_complete":
          updated.status = "completed";
          updated.cost_usd += event.cost_usd;
          updated.duration_ms += event.duration_ms;
          break;

        case "error":
          updated.status = "failed";
          updated.messages.push({
            id: nextMsgId(),
            role: "system",
            content: event.message,
            timestamp: Date.now(),
          });
          break;

        case "session_dead":
          updated.status = "dead";
          if (event.stderr_tail.length > 0) {
            updated.messages.push({
              id: nextMsgId(),
              role: "system",
              content: `Process exited (code ${event.exit_code ?? "unknown"})\n${event.stderr_tail.join("\n")}`,
              timestamp: Date.now(),
            });
          }
          break;

        case "rate_limit":
          updated.messages.push({
            id: nextMsgId(),
            role: "system",
            content: `Rate limited: ${event.status}`,
            timestamp: Date.now(),
          });
          break;
      }

      return {
        hubSessions: { ...state.hubSessions, [session_id]: updated },
      };
    });
  },
}));
