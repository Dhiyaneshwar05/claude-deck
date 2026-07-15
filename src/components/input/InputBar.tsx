import { useState, useRef, useCallback } from "react";
import { useAppStore } from "../../stores/appStore";
import { PaperPlaneRight, Stop, FolderOpen } from "@phosphor-icons/react";

export function InputBar() {
  const [input, setInput] = useState("");
  const [cwd, setCwd] = useState("");
  const [showNewSession, setShowNewSession] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const activeSessionId = useAppStore((s) => s.activeSessionId);
  const hubSessions = useAppStore((s) => s.hubSessions);
  const startSession = useAppStore((s) => s.startSession);
  const sendMessage = useAppStore((s) => s.sendMessage);
  const cancelActiveSession = useAppStore((s) => s.cancelActiveSession);

  const activeHub = activeSessionId ? hubSessions[activeSessionId] : null;
  const isRunning = activeHub?.status === "running" || activeHub?.status === "connecting";
  const canSendFollowUp =
    activeHub && (activeHub.status === "completed" || activeHub.status === "failed");

  const handleSubmit = useCallback(() => {
    const trimmed = input.trim();
    if (!trimmed) return;

    if (activeHub && (isRunning || canSendFollowUp)) {
      // Send follow-up to existing hub session
      sendMessage(trimmed);
    } else if (!activeHub) {
      // Start new session
      const workDir = cwd.trim() || "/tmp";
      startSession(workDir, trimmed);
      setShowNewSession(false);
    }

    setInput("");
    // Reset textarea height
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  }, [input, activeHub, isRunning, canSendFollowUp, cwd, sendMessage, startSession]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleSubmit();
    }
  };

  const handleInput = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value);
    // Auto-resize
    const el = e.target;
    el.style.height = "auto";
    el.style.height = Math.min(el.scrollHeight, 200) + "px";
  };

  // Determine placeholder and state
  let placeholder = "Start a new session... (Cmd+Enter to send)";
  if (activeHub) {
    if (isRunning) placeholder = "Claude is working...";
    else if (canSendFollowUp)
      placeholder = "Send a follow-up... (Cmd+Enter to send)";
  }

  const canSubmit = input.trim().length > 0 && !isRunning;

  const handleButtonClick = () => {
    if (isRunning) {
      void cancelActiveSession();
    } else {
      handleSubmit();
    }
  };

  return (
    <div className="border-t border-zinc-800/50 bg-zinc-950/80 backdrop-blur-sm px-4 py-3">
      {/* New session CWD picker (shown when no hub session active) */}
      {!activeHub && showNewSession && (
        <div className="flex items-center gap-2 mb-2 px-1">
          <FolderOpen size={14} className="text-zinc-500" />
          <input
            type="text"
            value={cwd}
            onChange={(e) => setCwd(e.target.value)}
            placeholder="Working directory (e.g. ~/projects/my-app)"
            className="flex-1 bg-zinc-900 text-zinc-300 text-xs rounded px-2 py-1 border border-zinc-800 focus:border-zinc-600 focus:outline-none placeholder:text-zinc-600"
          />
        </div>
      )}

      <div className="flex items-end gap-2">
        <textarea
          ref={textareaRef}
          value={input}
          onChange={handleInput}
          onKeyDown={handleKeyDown}
          onFocus={() => !activeHub && setShowNewSession(true)}
          placeholder={placeholder}
          disabled={isRunning}
          rows={1}
          className="flex-1 bg-zinc-900 text-zinc-200 text-sm rounded-lg px-3 py-2.5 border border-zinc-800 focus:border-zinc-600 focus:outline-none resize-none placeholder:text-zinc-600 disabled:opacity-50 disabled:cursor-not-allowed"
        />
        <button
          onClick={handleButtonClick}
          disabled={!isRunning && !canSubmit}
          title={isRunning ? "Stop (SIGINT)" : "Send (Cmd+Enter)"}
          className={`shrink-0 w-9 h-9 rounded-lg flex items-center justify-center transition-colors disabled:opacity-30 disabled:cursor-not-allowed text-white ${
            isRunning
              ? "bg-rose-600 hover:bg-rose-500"
              : "bg-emerald-600 hover:bg-emerald-500"
          }`}
        >
          {isRunning ? (
            <Stop size={16} weight="fill" />
          ) : (
            <PaperPlaneRight size={16} weight="fill" />
          )}
        </button>
      </div>

      {!activeHub && (
        <div className="text-[10px] text-zinc-600 mt-1.5 px-1">
          This will spawn a new Claude Code session
        </div>
      )}
    </div>
  );
}
