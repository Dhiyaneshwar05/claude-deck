import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useAppStore } from "../stores/appStore";
import type { PendingPermission } from "../types";

interface PermissionExpired {
  request_id: string;
  reason: string;
}

/**
 * Listens for permission events from the Rust permission server:
 * - `permission-request` → push into the pending queue.
 * - `permission-expired` → drop the matching card (the backend already
 *   auto-denied it or its session ended, so its request_id is gone and the
 *   Allow/Deny buttons would no-op). This is what keeps ghost cards from
 *   accumulating in the UI.
 */
export function usePermissionEvents() {
  const addPermission = useAppStore((s) => s.addPermission);
  const removePermission = useAppStore((s) => s.removePermission);

  useEffect(() => {
    const unlistenRequest = listen<Omit<PendingPermission, "received_at">>(
      "permission-request",
      (e) => {
        addPermission({ ...e.payload, received_at: Date.now() });
      },
    );

    const unlistenExpired = listen<PermissionExpired>(
      "permission-expired",
      (e) => {
        removePermission(e.payload.request_id);
      },
    );

    return () => {
      unlistenRequest.then((fn) => fn());
      unlistenExpired.then((fn) => fn());
    };
  }, [addPermission, removePermission]);
}
