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
 * Horizontal tab bar for switching between active terminal connections.
 *
 * UX-2: tabs are keyboard-accessible with role="tab", tabIndex, and
 * keyboard handlers for Enter/Space activation and arrow key navigation.
 */
export default function TerminalTabs({
  connections,
  activeId,
  onSelect,
  onClose,
}: TerminalTabsProps) {
  if (connections.length === 0) return null;

  const handleKeyDown = (e: React.KeyboardEvent, connId: string, index: number) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect(connId);
    } else if (e.key === "ArrowRight" || e.key === "ArrowDown") {
      e.preventDefault();
      const next = connections[(index + 1) % connections.length];
      if (next) {
        onSelect(next.id);
        // Focus the next tab element
        const nextEl = (e.currentTarget.parentElement?.children[index + 1] ??
          e.currentTarget.parentElement?.children[0]) as HTMLElement | undefined;
        nextEl?.focus();
      }
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
      e.preventDefault();
      const prev = connections[(index - 1 + connections.length) % connections.length];
      if (prev) {
        onSelect(prev.id);
        const prevEl = (e.currentTarget.parentElement?.children[index - 1] ??
          e.currentTarget.parentElement?.children[connections.length - 1]) as HTMLElement | undefined;
        prevEl?.focus();
      }
    }
  };

  return (
    <div className="terminal-tab-bar" role="tablist">
      {connections.map((conn, i) => (
        <div
          key={conn.id}
          role="tab"
          tabIndex={conn.id === activeId ? 0 : -1}
          aria-selected={conn.id === activeId}
          className={`terminal-tab ${conn.id === activeId ? "active" : ""}`}
          onClick={() => onSelect(conn.id)}
          onKeyDown={(e) => handleKeyDown(e, conn.id, i)}
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
