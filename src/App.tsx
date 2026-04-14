import { useEffect, useState, useCallback, useRef } from "react";
import SshSessionForm from "./components/SshSessionForm";
import SessionList from "./components/SessionList";
import TerminalPane from "./components/TerminalPane";
import RdpPane from "./components/RdpPane";
import TerminalTabs from "./components/TerminalTabs";
import HostKeyDialog from "./components/HostKeyDialog";
import type {
  SshSession,
  SshSessionDraft,
  Connection,
  HostKeyPrompt,
} from "./types";
import {
  getSessions,
  saveSession,
  deleteSession,
  exportSessions,
  importSessions,
  sshConnect,
  sshAcceptHostKey,
  sshDisconnect,
  rdpConnect,
  rdpDisconnect,
} from "./api";

type View = "list" | "form";

interface StatusMessage {
  type: "success" | "error";
  text: string;
}

/**
 * Root application component.
 *
 * Manages navigation between the session list and the new/edit form,
 * and coordinates all IPC with the Tauri backend.
 */
export default function App() {
  const [sessions, setSessions] = useState<SshSession[]>([]);
  const [view, setView] = useState<View>("list");
  const [editing, setEditing] = useState<SshSession | undefined>();
  const [status, setStatus] = useState<StatusMessage | null>(null);

  // ─── SSH connection state ──────────────────────────────────────
  const [connections, setConnections] = useState<Connection[]>([]);
  const [activeConnectionId, setActiveConnectionId] = useState<string | null>(
    null,
  );
  const [hostKeyPrompt, setHostKeyPrompt] = useState<HostKeyPrompt | null>(
    null,
  );
  const [passwordPrompt, setPasswordPrompt] = useState<{
    session: SshSession;
    resolve: (pw: string | null) => void;
  } | null>(null);
  const [passwordInput, setPasswordInput] = useState("");

  // ─── Sidebar resize (FE-2: use state not ref for className) ────
  const [sidebarWidth, setSidebarWidth] = useState(260);
  const [isResizing, setIsResizing] = useState(false);

  // FE-3: ref for connections so handleDisconnect never stale-captures
  const connectionsRef = useRef(connections);
  connectionsRef.current = connections;

  // ─── Settings popup ──────────────────────────────────────────
  const [showSettings, setShowSettings] = useState(false);

  // ─── Font size ─────────────────────────────────────────────
  const [fontSize, setFontSize] = useState(() => {
    const saved = localStorage.getItem("rxterm-font-size");
    return saved ? Number(saved) : 16;
  });

  useEffect(() => {
    document.documentElement.style.setProperty("--base-font-size", `${fontSize}px`);
    localStorage.setItem("rxterm-font-size", String(fontSize));
  }, [fontSize]);

  const decreaseFontSize = () => setFontSize((s) => Math.max(10, s - 1));
  const increaseFontSize = () => setFontSize((s) => Math.min(24, s + 1));

  /** Load sessions from the backend on mount. */
  useEffect(() => {
    getSessions()
      .then(setSessions)
      .catch((err: unknown) =>
        setStatus({ type: "error", text: String(err) }),
      );
  }, []);

  /** Clear the status message after a timeout. */
  useEffect(() => {
    if (!status) return;
    const timer = setTimeout(() => setStatus(null), 4000);
    return () => clearTimeout(timer);
  }, [status]);

  /** Handle form submission (create or update). */
  const handleSubmit = useCallback(
    async (draft: SshSessionDraft, existingId?: string) => {
      try {
        const id = existingId ?? crypto.randomUUID();
        const session: SshSession = { ...draft, id };
        const updated = await saveSession(session);
        setSessions(updated);
        setView("list");
        setEditing(undefined);
        setStatus({
          type: "success",
          text: existingId ? "Session updated." : "Session saved.",
        });
      } catch (err: unknown) {
        setStatus({ type: "error", text: String(err) });
      }
    },
    [],
  );

  /** Handle session deletion. */
  const handleDelete = useCallback(async (id: string) => {
    try {
      const updated = await deleteSession(id);
      setSessions(updated);
      setStatus({ type: "success", text: "Session deleted." });
    } catch (err: unknown) {
      setStatus({ type: "error", text: String(err) });
    }
  }, []);

  /** Export sessions to clipboard as JSON. */
  const handleExport = useCallback(async () => {
    try {
      const json = await exportSessions();
      await navigator.clipboard.writeText(json);
      setStatus({ type: "success", text: "Sessions copied to clipboard." });
    } catch (err: unknown) {
      setStatus({ type: "error", text: String(err) });
    }
  }, []);

  /** Import sessions from clipboard JSON. */
  const handleImport = useCallback(async () => {
    try {
      const json = await navigator.clipboard.readText();
      const updated = await importSessions(json);
      setSessions(updated);
      setStatus({ type: "success", text: "Sessions imported." });
    } catch (err: unknown) {
      setStatus({ type: "error", text: String(err) });
    }
  }, []);

  const openNewForm = () => {
    setEditing(undefined);
    setView("form");
  };

  const openEditForm = (session: SshSession) => {
    setEditing(session);
    setView("form");
  };

  const cancel = () => {
    setEditing(undefined);
    setView("list");
  };

  // ─── SSH connection handlers ─────────────────────────────────

  /** Attempt to connect to a session (SSH or RDP). */
  const handleConnect = useCallback(
    async (session: SshSession, overridePassword?: string) => {
      const proto = session.protocol ?? "ssh";

      if (proto === "rdp") {
        // RDP connection flow
        try {
          setStatus({ type: "success", text: `Connecting to ${session.label}…` });
          const result = await rdpConnect(session.id, overridePassword ?? session.password);

          const conn: Connection = {
            id: result.connection_id,
            sessionId: session.id,
            label: session.label,
            protocol: "rdp",
          };
          setConnections((prev) => [...prev, conn]);
          setActiveConnectionId(result.connection_id);
          setStatus({ type: "success", text: `Connected to ${session.label}` });
        } catch (err: unknown) {
          setStatus({ type: "error", text: String(err) });
        }
        return;
      }

      // SSH connection flow
      try {
        const pw =
          overridePassword ?? session.password ?? undefined;

        // If password auth but no password stored, prompt for it
        if (session.auth_method === "password" && !pw) {
          return new Promise<void>((resolve) => {
            setPasswordPrompt({
              session,
              resolve: (inputPw) => {
                setPasswordPrompt(null);
                setPasswordInput("");
                if (inputPw) {
                  handleConnect(session, inputPw).then(resolve);
                } else {
                  resolve();
                }
              },
            });
          });
        }

        setStatus({ type: "success", text: `Connecting to ${session.label}…` });
        const result = await sshConnect(session.id, pw);

        // ROB-6: typed response — check status instead of parsing error strings
        if (result.status === "host_key_unknown" && result.host_key) {
          setHostKeyPrompt({
            host: session.host,
            port: session.port,
            info: result.host_key,
            sessionId: session.id,
            password: overridePassword ?? session.password,
          });
          return;
        }

        if (result.status === "connected" && result.connection_id) {
          const conn: Connection = {
            id: result.connection_id,
            sessionId: session.id,
            label: session.label,
            protocol: "ssh",
          };
          setConnections((prev) => [...prev, conn]);
          setActiveConnectionId(result.connection_id);
          setStatus({ type: "success", text: `Connected to ${session.label}` });
        }
      } catch (err: unknown) {
        setStatus({ type: "error", text: String(err) });
      }
    },
    [],
  );

  /** User accepted the unknown host key — persist and retry. */
  const handleAcceptHostKey = useCallback(async () => {
    if (!hostKeyPrompt) return;
    try {
      await sshAcceptHostKey(
        hostKeyPrompt.host,
        hostKeyPrompt.port,
        hostKeyPrompt.info.key_data,
        hostKeyPrompt.info.algorithm,
      );
      const session = sessions.find((s) => s.id === hostKeyPrompt.sessionId);
      setHostKeyPrompt(null);
      if (session) {
        await handleConnect(session, hostKeyPrompt.password);
      }
    } catch (err: unknown) {
      setStatus({ type: "error", text: String(err) });
      setHostKeyPrompt(null);
    }
  }, [hostKeyPrompt, sessions, handleConnect]);

  /**
   * Remove a connection from state and update the active tab.
   *
   * FE-1: computes both `connections` and `activeConnectionId` together
   * instead of nesting state setters.
   */
  const removeConnection = useCallback((connectionId: string) => {
    setConnections((prev) => prev.filter((c) => c.id !== connectionId));
    setActiveConnectionId((current) => {
      if (current !== connectionId) return current;
      // FE-3: read from ref to avoid stale closure over connections
      const remaining = connectionsRef.current.filter((c) => c.id !== connectionId);
      return remaining.length > 0 ? remaining[remaining.length - 1].id : null;
    });
  }, []);

  /** Disconnect a connection and remove its tab. */
  const handleDisconnect = useCallback(async (connectionId: string) => {
    // FE-3: read from ref so this callback doesn't depend on connections state
    const conn = connectionsRef.current.find((c) => c.id === connectionId);
    try {
      if (conn?.protocol === "rdp") {
        await rdpDisconnect(connectionId);
      } else {
        await sshDisconnect(connectionId);
      }
    } catch {
      // Already disconnected
    }
    removeConnection(connectionId);
  }, [removeConnection]);

  /** Called when a terminal reports the connection was closed remotely. */
  const handleRemoteDisconnect = useCallback((connectionId: string) => {
    removeConnection(connectionId);
  }, [removeConnection]);

  return (
    <div
      className={`app-layout${isResizing ? ' resizing' : ''}`}
      onMouseMove={(e) => {
        if (!isResizing) return;
        const newWidth = Math.max(160, Math.min(e.clientX, 600));
        setSidebarWidth(newWidth);
      }}
      onMouseUp={() => setIsResizing(false)}
      onMouseLeave={() => setIsResizing(false)}
    >
      {/* ─── Left sidebar: host list ─── */}
      <div className="sidebar" style={{ width: sidebarWidth }}>
        <div className="sidebar-header">
          <span>Hosts</span>
          <div className="sidebar-header-actions">
            <button onClick={openNewForm} title="New Host">+</button>
          </div>
        </div>

        {status && (
          <div
            className={`sidebar-status ${
              status.type === "success" ? "status-success" : "status-error"
            }`}
          >
            {status.text}
          </div>
        )}

        {view === "list" && (
          <div className="sidebar-body">
            <SessionList
              sessions={sessions}
              connections={connections}
              onConnect={handleConnect}
              onEdit={openEditForm}
              onDelete={handleDelete}
            />
          </div>
        )}

        {view === "form" && (
          <div className="sidebar-form">
            <SshSessionForm
              initial={editing}
              onSubmit={handleSubmit}
              onCancel={cancel}
            />
          </div>
        )}

        {/* ─── Settings footer ─── */}
        <div className="sidebar-footer">
          <button
            className="sidebar-settings-btn"
            onClick={() => setShowSettings((v) => !v)}
            title="Settings"
          >
            &#x2699; Settings
          </button>

          {showSettings && (
            <div className="settings-overlay" onClick={() => setShowSettings(false)}>
              <div className="settings-popup" onClick={(e) => e.stopPropagation()}>
                <div className="settings-row">
                  <span className="settings-label">Font Size</span>
                  <div className="settings-controls">
                    <button onClick={decreaseFontSize} title="Decrease">A−</button>
                    <span className="settings-value">{fontSize}px</span>
                    <button onClick={increaseFontSize} title="Increase">A+</button>
                  </div>
                </div>
                <div className="settings-divider" />
                <button className="settings-action" onClick={() => { handleExport(); setShowSettings(false); }}>
                  &#x21e7; Export Hosts
                </button>
                <button className="settings-action" onClick={() => { handleImport(); setShowSettings(false); }}>
                  &#x21e9; Import Hosts
                </button>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* ─── Resize handle ─── */}
      <div
        className="resize-handle"
        onMouseDown={() => setIsResizing(true)}
      />

      {/* ─── Right main area: terminals ─── */}
      <div className="main-content">
        {connections.length > 0 ? (
          <div className="terminal-panel">
            <TerminalTabs
              connections={connections}
              activeId={activeConnectionId}
              onSelect={setActiveConnectionId}
              onClose={handleDisconnect}
            />
            <div className="terminal-pane-wrapper">
              {connections.map((conn) => (
                <div
                  key={conn.id}
                  className="terminal-pane-container"
                  style={{
                    display: conn.id === activeConnectionId ? "block" : "none",
                  }}
                >
                  {conn.protocol === "rdp" ? (
                    <RdpPane
                      connectionId={conn.id}
                      onDisconnected={() => handleRemoteDisconnect(conn.id)}
                      onReconnect={() => {
                        const session = sessions.find((s) => s.id === conn.sessionId);
                        removeConnection(conn.id);
                        if (session) handleConnect(session);
                      }}
                    />
                  ) : (
                    <TerminalPane
                      connectionId={conn.id}
                      onDisconnected={() => handleRemoteDisconnect(conn.id)}
                    />
                  )}
                </div>
              ))}
            </div>
          </div>
        ) : (
          <div className="main-empty">
            Select a host and click Connect to open a terminal
          </div>
        )}
      </div>

      {/* ─── Host key verification dialog ─── */}
      {hostKeyPrompt && (
        <HostKeyDialog
          host={hostKeyPrompt.host}
          port={hostKeyPrompt.port}
          fingerprint={hostKeyPrompt.info.fingerprint}
          onAccept={handleAcceptHostKey}
          onReject={() => setHostKeyPrompt(null)}
        />
      )}

      {/* ─── Password prompt dialog ─── */}
      {passwordPrompt && (
        <div className="dialog-overlay">
          <div className="dialog-box">
            <h3>Enter Password</h3>
            <p>
              Password required for{" "}
              <strong>{passwordPrompt.session.label}</strong>
            </p>
            <div className="form-group">
              <input
                type="password"
                value={passwordInput}
                onChange={(e) => setPasswordInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    passwordPrompt.resolve(passwordInput);
                  }
                }}
                autoFocus
                placeholder="SSH password"
              />
            </div>
            <div className="dialog-actions">
              <button
                className="btn-primary"
                onClick={() => passwordPrompt.resolve(passwordInput)}
              >
                Connect
              </button>
              <button
                className="btn-secondary"
                onClick={() => passwordPrompt.resolve(null)}
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
