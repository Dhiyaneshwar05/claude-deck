import type { ChatMessage } from "../../types";
import { User, Robot, Warning } from "@phosphor-icons/react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import "highlight.js/styles/github-dark.css";

const markdownComponents = {
  p: ({ children }: any) => (
    <p className="text-sm text-zinc-300 leading-relaxed mb-3 last:mb-0">
      {children}
    </p>
  ),
  h1: ({ children }: any) => (
    <h1 className="text-base font-semibold text-zinc-100 mt-4 mb-2">{children}</h1>
  ),
  h2: ({ children }: any) => (
    <h2 className="text-sm font-semibold text-zinc-100 mt-4 mb-2">{children}</h2>
  ),
  h3: ({ children }: any) => (
    <h3 className="text-sm font-medium text-zinc-200 mt-3 mb-1.5">{children}</h3>
  ),
  ul: ({ children }: any) => (
    <ul className="list-disc pl-5 space-y-1 mb-3 text-sm text-zinc-300 marker:text-zinc-600">
      {children}
    </ul>
  ),
  ol: ({ children }: any) => (
    <ol className="list-decimal pl-5 space-y-1 mb-3 text-sm text-zinc-300 marker:text-zinc-600">
      {children}
    </ol>
  ),
  li: ({ children }: any) => <li className="leading-relaxed">{children}</li>,
  a: ({ href, children }: any) => (
    <a
      href={href}
      target="_blank"
      rel="noreferrer"
      className="text-emerald-400 hover:text-emerald-300 underline underline-offset-2"
    >
      {children}
    </a>
  ),
  strong: ({ children }: any) => (
    <strong className="font-semibold text-zinc-100">{children}</strong>
  ),
  em: ({ children }: any) => <em className="italic text-zinc-200">{children}</em>,
  blockquote: ({ children }: any) => (
    <blockquote className="border-l-2 border-zinc-700 pl-3 my-3 text-zinc-400 italic">
      {children}
    </blockquote>
  ),
  // Inline code only — fenced code blocks are handled by the `pre` renderer
  // below so rehype-highlight's syntax tokens survive.
  code: ({ inline, children, className, ...props }: any) => {
    if (inline) {
      return (
        <code className="px-1 py-0.5 rounded bg-zinc-800 text-emerald-300 text-[0.85em] font-mono">
          {children}
        </code>
      );
    }
    return (
      <code className={`text-xs font-mono leading-relaxed ${className || ""}`} {...props}>
        {children}
      </code>
    );
  },
  pre: ({ children }: any) => {
    // Try to pull the language off the nested <code> for a header label
    const child = Array.isArray(children) ? children[0] : children;
    const className: string = child?.props?.className || "";
    const lang = /language-(\w+)/.exec(className)?.[1];
    return (
      <pre className="bg-zinc-900/70 border border-zinc-800 rounded-lg p-3 my-3 overflow-x-auto">
        {lang && (
          <div className="text-[10px] uppercase tracking-wider text-zinc-600 mb-1.5">
            {lang}
          </div>
        )}
        {children}
      </pre>
    );
  },
  hr: () => <hr className="border-zinc-800 my-4" />,
  table: ({ children }: any) => (
    <div className="my-3 overflow-x-auto">
      <table className="text-sm text-zinc-300 border-collapse">{children}</table>
    </div>
  ),
  th: ({ children }: any) => (
    <th className="border border-zinc-800 px-2 py-1 text-left font-semibold text-zinc-200 bg-zinc-900/50">
      {children}
    </th>
  ),
  td: ({ children }: any) => (
    <td className="border border-zinc-800 px-2 py-1">{children}</td>
  ),
};

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
  const isStreaming =
    message.content.length > 0 && !message.content.endsWith("\n\n");

  return (
    <div className="flex gap-3 px-6 py-3">
      <div className="w-7 h-7 rounded-lg bg-emerald-600/20 flex items-center justify-center shrink-0 mt-0.5">
        <Robot size={14} className="text-emerald-400" />
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-xs text-zinc-500 mb-1">Claude</div>
        <div className="text-sm text-zinc-300 break-words leading-relaxed">
          <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            rehypePlugins={[[rehypeHighlight, { detect: true, ignoreMissing: true }]]}
            components={markdownComponents}
          >
            {message.content}
          </ReactMarkdown>
          {isStreaming && (
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
