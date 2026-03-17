/**
 * Thin wrapper around Tauri `invoke()` for all session-related IPC calls.
 *
 * Each function maps 1:1 to a `#[tauri::command]` on the Rust side.
 */
import { invoke } from "@tauri-apps/api/core";
import type { SshSession } from "./types";

/** Retrieve all saved SSH sessions from the backend store. */
export async function getSessions(): Promise<SshSession[]> {
  return invoke<SshSession[]>("get_sessions");
}

/** Save (create or update) an SSH session. Returns the full updated list. */
export async function saveSession(session: SshSession): Promise<SshSession[]> {
  return invoke<SshSession[]>("save_session", { session });
}

/** Delete a session by id. Returns the full updated list. */
export async function deleteSession(id: string): Promise<SshSession[]> {
  return invoke<SshSession[]>("delete_session", { id });
}

/** Export all sessions as a pretty-printed JSON string. */
export async function exportSessions(): Promise<string> {
  return invoke<string>("export_sessions");
}

/** Import sessions from a JSON string, merging with existing data. */
export async function importSessions(json: string): Promise<SshSession[]> {
  return invoke<SshSession[]>("import_sessions", { json });
}

// ─── SSH connection commands ────────────────────────────────────────────────

/** Initiate an SSH connection to the given session. */
export async function sshConnect(
  sessionId: string,
  password?: string,
): Promise<{ connection_id: string }> {
  return invoke<{ connection_id: string }>("ssh_connect", {
    sessionId,
    password: password ?? null,
  });
}

/** Accept a server host key and persist it for future connections. */
export async function sshAcceptHostKey(
  host: string,
  port: number,
  keyData: string,
  algorithm: string,
): Promise<void> {
  return invoke("ssh_accept_host_key", { host, port, keyData, algorithm });
}

/** Send raw bytes (user keystrokes) to an active SSH connection. */
export async function sshSend(
  connectionId: string,
  data: number[],
): Promise<void> {
  return invoke("ssh_send", { connectionId, data });
}

/** Notify the remote server of a terminal resize. */
export async function sshResize(
  connectionId: string,
  cols: number,
  rows: number,
): Promise<void> {
  return invoke("ssh_resize", { connectionId, cols, rows });
}

/** Disconnect an active SSH connection. */
export async function sshDisconnect(connectionId: string): Promise<void> {
  return invoke("ssh_disconnect", { connectionId });
}

// ─── VNC connection commands ────────────────────────────────────────────────

/** Start a VNC WebSocket proxy for the given session. */
export async function vncConnect(
  sessionId: string,
  password?: string,
): Promise<{ connection_id: string; ws_port: number }> {
  return invoke<{ connection_id: string; ws_port: number }>("vnc_connect", {
    sessionId,
    password: password ?? null,
  });
}

/** Stop a VNC proxy connection. */
export async function vncDisconnect(connectionId: string): Promise<void> {
  return invoke("vnc_disconnect", { connectionId });
}
