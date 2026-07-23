import { invoke } from "@tauri-apps/api/core";
import type {
  DiscoveredSession,
  SpawnedSessionInfo,
  PermissionDecision,
} from "../types";

export async function listSessions(): Promise<DiscoveredSession[]> {
  return invoke<DiscoveredSession[]>("list_sessions");
}

export async function checkSessionHealth(pid: number): Promise<boolean> {
  return invoke<boolean>("check_session_health", { pid });
}

export async function createSession(
  cwd: string,
  prompt: string,
  model?: string,
  resumeSessionId?: string,
): Promise<SpawnedSessionInfo> {
  return invoke<SpawnedSessionInfo>("create_session", {
    cwd,
    prompt,
    model: model ?? null,
    resumeSessionId: resumeSessionId ?? null,
  });
}

export async function sendPrompt(
  sessionId: string,
  prompt: string,
): Promise<void> {
  return invoke("send_prompt", { sessionId, prompt });
}

export async function cancelSession(sessionId: string): Promise<void> {
  return invoke("cancel_session", { sessionId });
}

export async function resolvePermission(
  requestId: string,
  runToken: string,
  decision: PermissionDecision,
): Promise<void> {
  return invoke("resolve_permission", { requestId, runToken, decision });
}

export async function getPermissionServerInfo(): Promise<{
  port: number;
  app_secret: string;
} | null> {
  return invoke("get_permission_server_info");
}

export async function listScopedAllows(): Promise<string[]> {
  return invoke<string[]>("list_scoped_allows");
}

export async function clearScopedAllows(key?: string): Promise<void> {
  return invoke("clear_scoped_allows", { key: key ?? null });
}
