/** Shared TypeScript types mirroring the Rust data model. */

/** Connection protocol for a session. */
export type Protocol = "ssh" | "vnc";

/** Authentication method for an SSH session. */
export type AuthMethod = "password" | "key";

/** A saved session configuration (SSH or VNC). */
export interface SshSession {
  /** Unique identifier (UUID v4). */
  id: string;
  /** Human-readable label. */
  label: string;
  /** Connection protocol (defaults to "ssh"). */
  protocol: Protocol;
  /** Remote hostname or IP address. */
  host: string;
  /** Port (default 22 for SSH, 5900 for VNC). */
  port: number;
  /** Username for authentication (SSH only). */
  username: string;
  /** Authentication method (SSH only). */
  auth_method: AuthMethod;
  /** Password (SSH password auth or VNC authentication). */
  password?: string;
  /** Path to the private key file (SSH key auth only). */
  private_key_path?: string;
  /** Optional notes / description. */
  notes?: string;
}

/** Shape of the new-session form state (before an id is assigned). */
export type SshSessionDraft = Omit<SshSession, "id">;

/** An active terminal or VNC connection. */
export interface Connection {
  /** Unique connection identifier (UUID). */
  id: string;
  /** The session ID this connection was created from. */
  sessionId: string;
  /** Human-readable label (copied from session). */
  label: string;
  /** Connection protocol. */
  protocol: Protocol;
  /** Local WebSocket port for VNC proxy (VNC only). */
  wsPort?: number;
  /** VNC password for the RFB handshake (VNC only). */
  vncPassword?: string;
}

/** Data returned when a host key is unknown and needs user approval. */
export interface HostKeyInfo {
  fingerprint: string;
  key_data: string;
  algorithm: string;
}

/** Prompt state tracked while awaiting user host-key decision. */
export interface HostKeyPrompt {
  host: string;
  port: number;
  info: HostKeyInfo;
  sessionId: string;
  password?: string;
}

/** Create an empty SSH draft with sensible defaults. */
export function emptySshDraft(): SshSessionDraft {
  return {
    label: "",
    protocol: "ssh",
    host: "",
    port: 22,
    username: "",
    auth_method: "password",
    password: "",
    private_key_path: "",
    notes: "",
  };
}

/** Create an empty VNC draft with sensible defaults. */
export function emptyVncDraft(): SshSessionDraft {
  return {
    label: "",
    protocol: "vnc",
    host: "",
    port: 5900,
    username: "",
    auth_method: "password",
    password: "",
    private_key_path: "",
    notes: "",
  };
}
