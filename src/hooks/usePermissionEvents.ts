import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useAppStore } from "../stores/appStore";
import type { PendingPermission } from "../types";

/**
 * Listens for `permission-request` events from the Rust permission server
 * and pushes them into the app store's pending-permissions queue.
 */
export function usePermissionEvents() {
  const addPermission = useAppStore((s) => s.addPermission);

  useEffect(() => {
    console.log("[permissions] listener attached");
    const unlisten = listen<Omit<PendingPermission, "received_at">>(
      "permission-request",
      (e) => {
        console.log("[permissions] received event:", e.payload);
        addPermission({ ...e.payload, received_at: Date.now() });
      },
    );

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [addPermission]);
}
