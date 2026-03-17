import { useEffect, useState, useCallback, useRef } from "react";
import SshSessionForm from "./components/SshSessionForm";
import SessionList from "./components/SessionList";
import TerminalPane from "./components/TerminalPane";
import VncPane from "./components/VncPane";
import TerminalTabs from "./components/TerminalTabs";
import HostKeyDialog from "./components/HostKeyDialog";
import type {
  SshSession,
  SshSessionDraft,
  Connection,
  HostKeyPrompt,
  HostKeyInfo,
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
  vncConnect,
  vncDisconnect,
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

  // ─── Sidebar resize ────────────────────────────────────────
  const [sidebarWidth, setSidebarWidth] = useState(260);
  const isResizing = useRef(false);

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

  /** Parse a HOST_KEY_UNKNOWN error message into structured data. */
  const parseHostKeyError = (errMsg: string): HostKeyInfo | null => {
    const prefix = "HOST_KEY_UNKNOWN:";
    const idx = errMsg.indexOf(prefix);
    if (idx === -1) return null;
    try {
      return JSON.parse(errMsg.slice(idx + prefix.length));
    } catch {
      return null;
    }
  };

  /** Attempt to connect to a session (SSH or VNC). */
  const handleConnect = useCallback(
    async (session: SshSession, overridePassword?: string) => {
      const proto = session.protocol ?? "ssh";

      if (proto === "vnc") {
        // VNC connection flow
        try {
          setStatus({ type: "success", text: `Connecting to ${session.label}…` });
          const result = await vncConnect(session.id, overridePassword ?? session.password);

          const conn: Connection = {
            id: result.connection_id,
            sessionId: session.id,
            label: session.label,
            protocol: "vnc",
            wsPort: result.ws_port,
            vncPassword: overridePassword ?? session.password,
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

        const conn: Connection = {
          id: result.connection_id,
          sessionId: session.id,
          label: session.label,
          protocol: "ssh",
        };
        setConnections((prev) => [...prev, conn]);
        setActiveConnectionId(result.connection_id);
        setStatus({ type: "success", text: `Connected to ${session.label}` });
      } catch (err: unknown) {
        const msg = String(err);
        const hostKeyInfo = parseHostKeyError(msg);
        if (hostKeyInfo) {
          setHostKeyPrompt({
            host: session.host,
            port: session.port,
            info: hostKeyInfo,
            sessionId: session.id,
            password: overridePassword ?? session.password,
          });
        } else {
          setStatus({ type: "error", text: msg });
        }
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

  /** Disconnect a connection and remove its tab. */
  const handleDisconnect = useCallback(async (connectionId: string) => {
    // Determine connection type to call the right disconnect API
    const conn = connections.find((c) => c.id === connectionId);
    try {
      if (conn?.protocol === "vnc") {
        await vncDisconnect(connectionId);
      } else {
        await sshDisconnect(connectionId);
      }
    } catch {
      // Already disconnected
    }
    setConnections((prev) => {
      const next = prev.filter((c) => c.id !== connectionId);
      setActiveConnectionId((current) => {
        if (current === connectionId) {
          return next.length > 0 ? next[next.length - 1].id : null;
        }
        return current;
      });
      return next;
    });
  }, [connections]);

  /** Called when a terminal reports the connection was closed remotely. */
  const handleRemoteDisconnect = useCallback((connectionId: string) => {
    setConnections((prev) => {
      const next = prev.filter((c) => c.id !== connectionId);
      setActiveConnectionId((current) => {
        if (current === connectionId) {
          return next.length > 0 ? next[next.length - 1].id : null;
        }
        return current;
      });
      return next;
    });
  }, []);

  return (
    <div
      className={`app-layout${isResizing.current ? ' resizing' : ''}`}
      onMouseMove={(e) => {
        if (!isResizing.current) return;
        const newWidth = Math.max(160, Math.min(e.clientX, 600));
        setSidebarWidth(newWidth);
      }}
      onMouseUp={() => { isResizing.current = false; }}
      onMouseLeave={() => { isResizing.current = false; }}
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
        onMouseDown={() => { isResizing.current = true; }}
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
                  {conn.protocol === "vnc" && conn.wsPort ? (
                    <VncPane
                      connectionId={conn.id}
                      wsPort={conn.wsPort}
                      password={conn.vncPassword}
                      onDisconnected={() => handleRemoteDisconnect(conn.id)}
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
