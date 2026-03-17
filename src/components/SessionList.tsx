import type { SshSession } from "../types";

interface SessionListProps {
  /** The list of saved sessions to display. */
  sessions: SshSession[];
  /** Called when the user clicks "Edit" on a session. */
  onEdit: (session: SshSession) => void;
  /** Called when the user clicks "Delete" on a session. */
  onDelete: (id: string) => void;
}

/**
 * Displays all saved SSH sessions as a vertical list of cards.
 *
 * Each card shows the label, host:port, username, and auth method,
 * with Edit / Delete action buttons.
 */
export default function SessionList({
  sessions,
  onEdit,
  onDelete,
}: SessionListProps) {
  if (sessions.length === 0) {
    return (
      <p style={{ color: "#64748b", marginTop: "1.5rem" }}>
        No saved sessions yet. Click <strong>New Session</strong> to get started.
      </p>
    );
  }

  return (
    <div className="session-list">
      {sessions.map((s) => (
        <div key={s.id} className="session-card">
          <div className="session-card-info">
            <span className="session-label">{s.label}</span>
            <span className="session-detail">
              {s.username}@{s.host}:{s.port} &middot;{" "}
              {s.auth_method === "password" ? "Password" : "SSH Key"}
            </span>
            {s.notes && <span className="session-detail">{s.notes}</span>}
          </div>
          <div className="session-card-actions">
            <button className="btn-secondary" onClick={() => onEdit(s)}>
              Edit
            </button>
            <button className="btn-danger" onClick={() => onDelete(s.id)}>
              Delete
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
