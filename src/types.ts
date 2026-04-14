/** Shared TypeScript types mirroring the Rust data model. */

/** Connection protocol for a session. */
export type Protocol = "ssh" | "rdp";

/** Authentication method for an SSH session. */
export type AuthMethod = "password" | "key";

/** A saved session configuration (SSH or RDP). */
export interface SshSession {
  /** Unique identifier (UUID v4). */
  id: string;
  /** Human-readable label. */
  label: string;
  /** Connection protocol (defaults to "ssh"). */
  protocol: Protocol;
  /** Remote hostname or IP address. */
  host: string;
  /** Port (default 22 for SSH, 3389 for RDP). */
  port: number;
  /** Username for authentication (SSH and RDP). */
  username: string;
  /** Authentication method (SSH only). */
  auth_method: AuthMethod;
  /** Password (SSH password auth or RDP authentication). */
  password?: string;
  /** Path to the private key file (SSH key auth only). */
  private_key_path?: string;
  /** Optional notes / description. */
  notes?: string;
  /** Windows domain for RDP authentication (RDP only). */
  domain?: string;
}

/** Shape of the new-session form state (before an id is assigned). */
export type SshSessionDraft = Omit<SshSession, "id">;

/** An active terminal or RDP connection. */
export interface Connection {
  /** Unique connection identifier (UUID). */
  id: string;
  /** The session ID this connection was created from. */
  sessionId: string;
  /** Human-readable label (copied from session). */
  label: string;
  /** Connection protocol. */
  protocol: Protocol;
  /** RDP desktop dimensions (RDP only). */
  rdpWidth?: number;
  rdpHeight?: number;
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

/** Payload of an RDP frame update event from the backend. */
export interface RdpFramePayload {
  connection_id: string;
  full_width: number;
  full_height: number;
  x: number;
  y: number;
  width: number;
  height: number;
  /** Base64-encoded RGBA pixel data for the dirty rectangle. */
  data: string;
}

/** Payload of the rdp-disconnected event. */
export interface RdpDisconnectedPayload {
  connection_id: string;
  reason: string;
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

/** Create an empty RDP draft with sensible defaults. */
export function emptyRdpDraft(): SshSessionDraft {
  return {
    label: "",
    protocol: "rdp",
    host: "",
    port: 3389,
    username: "",
    auth_method: "password",
    password: "",
    domain: "",
    notes: "",
  };
}
