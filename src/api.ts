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

/** ROB-6: typed result from ssh_connect — no more parsing error strings. */
export interface SshConnectResponse {
  status: "connected" | "host_key_unknown";
  connection_id?: string;
  host_key?: {
    fingerprint: string;
    key_data: string;
    algorithm: string;
  };
}

/** Initiate an SSH connection to the given session. */
export async function sshConnect(
  sessionId: string,
  password?: string,
): Promise<SshConnectResponse> {
  return invoke<SshConnectResponse>("ssh_connect", {
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

// ─── RDP connection commands ────────────────────────────────────────────────

/** Start an RDP session for the given session. */
export async function rdpConnect(
  sessionId: string,
  password?: string,
): Promise<{ connection_id: string }> {
  return invoke<{ connection_id: string }>("rdp_connect", {
    sessionId,
    password: password ?? null,
  });
}

/** Disconnect an active RDP session. */
export async function rdpDisconnect(connectionId: string): Promise<void> {
  return invoke("rdp_disconnect", { connectionId });
}

/** Send a mouse event to an active RDP session. */
export async function rdpMouseEvent(
  connectionId: string,
  x: number,
  y: number,
  button: number | null,
  pressed: boolean,
  scrollDelta: number | null,
): Promise<void> {
  return invoke("rdp_mouse_event", {
    connectionId,
    event: { x, y, button, pressed, scroll_delta: scrollDelta },
  });
}

/** Send a keyboard event to an active RDP session. */
export async function rdpKeyEvent(
  connectionId: string,
  scancode: number,
  pressed: boolean,
): Promise<void> {
  return invoke("rdp_key_event", {
    connectionId,
    event: { scancode, pressed },
  });
}

// ─── VNC connection commands ────────────────────────────────────────────────

/** Start a VNC session for the given session. */
export async function vncConnect(
  sessionId: string,
  password?: string,
): Promise<{ connection_id: string }> {
  return invoke<{ connection_id: string }>("vnc_connect", {
    sessionId,
    password: password ?? null,
  });
}

/** Disconnect an active VNC session. */
export async function vncDisconnect(connectionId: string): Promise<void> {
  return invoke("vnc_disconnect", { connectionId });
}

/** Send a mouse event to an active VNC session. */
export async function vncMouseEvent(
  connectionId: string,
  x: number,
  y: number,
  button: number | null,
  pressed: boolean,
  scrollDelta: number | null,
): Promise<void> {
  return invoke("vnc_mouse_event", {
    connectionId,
    event: { x, y, button, pressed, scroll_delta: scrollDelta },
  });
}

/** Send a keyboard event to an active VNC session. */
export async function vncKeyEvent(
  connectionId: string,
  keysym: number,
  pressed: boolean,
): Promise<void> {
  return invoke("vnc_key_event", {
    connectionId,
    event: { keysym, pressed },
  });
}

/** Send clipboard text to an active VNC session. */
export async function vncSendClipboard(
  connectionId: string,
  text: string,
): Promise<void> {
  return invoke("vnc_send_clipboard", { connectionId, text });
}
