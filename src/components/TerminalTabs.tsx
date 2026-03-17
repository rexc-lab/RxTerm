import type { Connection } from "../types";

interface TerminalTabsProps {
  /** All active connections. */
  connections: Connection[];
  /** Currently displayed connection ID. */
  activeId: string | null;
  /** Called when the user selects a tab. */
  onSelect: (id: string) => void;
  /** Called when the user closes a tab. */
  onClose: (id: string) => void;
}

/**
 * Horizontal tab bar for switching between active SSH terminal connections.
 */
export default function TerminalTabs({
  connections,
  activeId,
  onSelect,
  onClose,
}: TerminalTabsProps) {
  if (connections.length === 0) return null;

  return (
    <div className="terminal-tab-bar">
      {connections.map((conn) => (
        <div
          key={conn.id}
          className={`terminal-tab ${conn.id === activeId ? "active" : ""}`}
          onClick={() => onSelect(conn.id)}
        >
          <span className="terminal-tab-label">{conn.label}</span>
          <button
            className="terminal-tab-close"
            onClick={(e) => {
              e.stopPropagation();
              onClose(conn.id);
            }}
            title="Disconnect"
          >
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
