import { useEffect, useState, useCallback } from "react";
import SshSessionForm from "./components/SshSessionForm";
import SessionList from "./components/SessionList";
import type { SshSession, SshSessionDraft } from "./types";
import {
  getSessions,
  saveSession,
  deleteSession,
  exportSessions,
  importSessions,
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

  return (
    <div>
      <h1>RxTerm</h1>

      {status && (
        <div
          className={`status-message ${
            status.type === "success" ? "status-success" : "status-error"
          }`}
        >
          {status.text}
        </div>
      )}

      {view === "list" && (
        <>
          <div className="button-row">
            <button className="btn-primary" onClick={openNewForm}>
              + New Session
            </button>
            <button className="btn-secondary" onClick={handleExport}>
              Export
            </button>
            <button className="btn-secondary" onClick={handleImport}>
              Import
            </button>
          </div>
          <SessionList
            sessions={sessions}
            onEdit={openEditForm}
            onDelete={handleDelete}
          />
        </>
      )}

      {view === "form" && (
        <SshSessionForm
          initial={editing}
          onSubmit={handleSubmit}
          onCancel={cancel}
        />
      )}
    </div>
  );
}
