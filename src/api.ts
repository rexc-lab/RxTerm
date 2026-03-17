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
