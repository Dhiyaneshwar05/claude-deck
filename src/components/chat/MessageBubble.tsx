import type { ChatMessage } from "../../types";
import { User, Robot, Warning } from "@phosphor-icons/react";

function UserMessage({ message }: { message: ChatMessage }) {
  return (
    <div className="flex gap-3 px-6 py-3">
      <div className="w-7 h-7 rounded-lg bg-blue-600/20 flex items-center justify-center shrink-0 mt-0.5">
        <User size={14} className="text-blue-400" />
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-xs text-zinc-500 mb-1">You</div>
        <div className="text-sm text-zinc-200 whitespace-pre-wrap break-words">
          {message.content}
        </div>
      </div>
    </div>
  );
}

function AssistantMessage({ message }: { message: ChatMessage }) {
  return (
    <div className="flex gap-3 px-6 py-3">
      <div className="w-7 h-7 rounded-lg bg-emerald-600/20 flex items-center justify-center shrink-0 mt-0.5">
        <Robot size={14} className="text-emerald-400" />
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-xs text-zinc-500 mb-1">Claude</div>
        <div className="text-sm text-zinc-300 whitespace-pre-wrap break-words leading-relaxed">
          {message.content}
          {/* Show a blinking cursor when streaming */}
          {message.content.length > 0 && !message.content.endsWith("\n\n") && (
            <span className="inline-block w-1.5 h-4 bg-emerald-400/60 ml-0.5 animate-pulse align-text-bottom" />
          )}
        </div>
      </div>
    </div>
  );
}

function SystemMessage({ message }: { message: ChatMessage }) {
  return (
    <div className="flex gap-3 px-6 py-2">
      <div className="w-7 h-7 rounded-lg bg-amber-600/20 flex items-center justify-center shrink-0 mt-0.5">
        <Warning size={14} className="text-amber-400" />
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-xs text-amber-400/70 font-mono whitespace-pre-wrap break-words">
          {message.content}
        </div>
      </div>
    </div>
  );
}

export function MessageBubble({ message }: { message: ChatMessage }) {
  switch (message.role) {
    case "user":
      return <UserMessage message={message} />;
    case "assistant":
      return <AssistantMessage message={message} />;
    case "system":
      return <SystemMessage message={message} />;
    default:
      return null; // tool messages handled by ToolCallCard
  }
}
