import type { SshSession, Connection } from "../types";

interface SessionListProps {
  /** The list of saved sessions to display. */
  sessions: SshSession[];
  /** Currently active connections. */
  connections: Connection[];
  /** Called when the user clicks "Connect" on a session. */
  onConnect: (session: SshSession) => void;
  /** Called when the user clicks "Edit" on a session. */
  onEdit: (session: SshSession) => void;
  /** Called when the user clicks "Delete" on a session. */
  onDelete: (id: string) => void;
}

/**
 * Compact sidebar host list styled like VS Code's explorer.
 */
export default function SessionList({
  sessions,
  connections,
  onConnect,
  onEdit,
  onDelete,
}: SessionListProps) {
  if (sessions.length === 0) {
    return (
      <div className="sidebar-empty">
        No hosts yet. Click <strong>+</strong> to add one.
      </div>
    );
  }

  const connectedSessionIds = new Set(connections.map((c) => c.sessionId));

  return (
    <div className="session-list">
      {sessions.map((s) => {
        const proto = s.protocol ?? "ssh";
        const protoBadge = proto === "vnc" ? "VNC" : "SSH";
        return (
          <div
            key={s.id}
            className={`host-item ${connectedSessionIds.has(s.id) ? "connected" : ""}`}
            onDoubleClick={() => onConnect(s)}
            title={
              proto === "vnc"
                ? `${s.host}:${s.port} (VNC) — Double-click to connect`
                : `${s.username}@${s.host}:${s.port} — Double-click to connect`
            }
          >
            <div className="host-item-info">
              <span className="host-item-label">
                <span className={`protocol-badge protocol-${proto}`}>{protoBadge}</span>
                {s.label}
              </span>
              <span className="host-item-detail">
                {proto === "vnc"
                  ? `${s.host}:${s.port}`
                  : `${s.username}@${s.host}:${s.port}`}
              </span>
            </div>
            <div className="host-item-actions">
              <button onClick={() => onConnect(s)} title="Connect">&#x25B6;</button>
              <button onClick={() => onEdit(s)} title="Edit">&#x270E;</button>
              <button className="btn-delete" onClick={() => onDelete(s.id)} title="Delete">&#x2715;</button>
            </div>
          </div>
        );
      })}
    </div>
  );
}
