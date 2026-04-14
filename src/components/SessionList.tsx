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
        const protoBadge = proto === "rdp" ? "RDP" : "SSH";
        const detail =
          proto === "rdp"
            ? `${s.username}@${s.host}:${s.port}`
            : `${s.username}@${s.host}:${s.port}`;
        const tooltip =
          proto === "rdp"
            ? `${s.username}@${s.host}:${s.port} (RDP) — Double-click to connect`
            : `${s.username}@${s.host}:${s.port} — Double-click to connect`;
        const isConnected = connectedSessionIds.has(s.id);
        return (
          <div
            key={s.id}
            className={`host-item ${isConnected ? "connected" : ""}`}
            onDoubleClick={() => { if (!isConnected) onConnect(s); }}
            title={tooltip}
          >
            <div className="host-item-info">
              <span className="host-item-label">
                <span className={`protocol-badge protocol-${proto}`}>{protoBadge}</span>
                {s.label}
              </span>
              <span className="host-item-detail">{detail}</span>
            </div>
            <div className="host-item-actions">
              {/* FE-6: disable connect if already connected to prevent duplicates */}
              <button onClick={() => onConnect(s)} title={isConnected ? "Already connected" : "Connect"} disabled={isConnected}>&#x25B6;</button>
              <button onClick={() => onEdit(s)} title="Edit">&#x270E;</button>
              {/* UX-4: confirm before deleting to prevent accidental data loss */}
              <button className="btn-delete" onClick={() => {
                if (window.confirm(`Delete "${s.label}"? This cannot be undone.`)) {
                  onDelete(s.id);
                }
              }} title="Delete">&#x2715;</button>
            </div>
          </div>
        );
      })}
    </div>
  );
}
