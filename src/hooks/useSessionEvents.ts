import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { useAppStore } from "../stores/appStore";
import type { SessionEvent } from "../types";

/**
 * Listens to "session-event" Tauri events from the Rust backend
 * and routes them through the store's event handler.
 *
 * Text chunks are batched per animation frame to avoid flooding React.
 */
export function useSessionEvents() {
  const handleSessionEvent = useAppStore((s) => s.handleSessionEvent);
  const chunkBuffer = useRef<Map<string, string>>(new Map());
  const rafId = useRef<number | null>(null);

  useEffect(() => {
    const flush = () => {
      rafId.current = null;
      const buffer = chunkBuffer.current;
      if (buffer.size === 0) return;

      for (const [sessionId, text] of buffer) {
        handleSessionEvent({
          session_id: sessionId,
          event: { type: "text_chunk", text },
        });
      }
      buffer.clear();
    };

    const unlisten = listen<SessionEvent>("session-event", (e) => {
      const payload = e.payload;

      // Batch text_chunk events per RAF for performance
      if (payload.event.type === "text_chunk") {
        const buffer = chunkBuffer.current;
        const existing = buffer.get(payload.session_id) || "";
        buffer.set(payload.session_id, existing + payload.event.text);

        if (rafId.current === null) {
          rafId.current = requestAnimationFrame(flush);
        }
        return;
      }

      // All other events pass through immediately
      handleSessionEvent(payload);
    });

    return () => {
      unlisten.then((fn) => fn());
      if (rafId.current !== null) {
        cancelAnimationFrame(rafId.current);
      }
    };
  }, [handleSessionEvent]);
}
