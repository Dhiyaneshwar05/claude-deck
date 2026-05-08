import { useEffect, useRef } from "react";
import { useAppStore } from "../stores/appStore";

const POLL_INTERVAL_MS = 3000;

export function useHealthPoll() {
  const refreshSessions = useAppStore((s) => s.refreshSessions);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    // Initial fetch
    refreshSessions();

    // Poll every 3s
    intervalRef.current = setInterval(refreshSessions, POLL_INTERVAL_MS);

    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [refreshSessions]);
}
