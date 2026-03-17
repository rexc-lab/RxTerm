/** Shared TypeScript types mirroring the Rust data model. */

/** Authentication method for an SSH session. */
export type AuthMethod = "password" | "key";

/** A saved SSH session configuration. */
export interface SshSession {
  /** Unique identifier (UUID v4). */
  id: string;
  /** Human-readable label. */
  label: string;
  /** Remote hostname or IP address. */
  host: string;
  /** SSH port (default 22). */
  port: number;
  /** Username for authentication. */
  username: string;
  /** Authentication method. */
  auth_method: AuthMethod;
  /** Password (only when auth_method === "password"). */
  password?: string;
  /** Path to the private key file (only when auth_method === "key"). */
  private_key_path?: string;
  /** Optional notes / description. */
  notes?: string;
}

/** Shape of the new-session form state (before an id is assigned). */
export type SshSessionDraft = Omit<SshSession, "id">;

/** Create an empty draft with sensible defaults. */
export function emptySshDraft(): SshSessionDraft {
  return {
    label: "",
    host: "",
    port: 22,
    username: "",
    auth_method: "password",
    password: "",
    private_key_path: "",
    notes: "",
  };
}
