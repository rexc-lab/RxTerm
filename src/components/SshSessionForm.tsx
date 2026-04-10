import { useState, useEffect } from "react";
import type { SshSession, SshSessionDraft, AuthMethod, Protocol } from "../types";
import { emptySshDraft, emptyVncDraft, emptyRdpDraft } from "../types";

interface SshSessionFormProps {
  /** If provided, the form pre-fills with this session's data (edit mode). */
  initial?: SshSession;
  /** Called when the user submits the form. */
  onSubmit: (draft: SshSessionDraft, existingId?: string) => void;
  /** Called when the user cancels editing. */
  onCancel: () => void;
}

/**
 * Form for creating or editing a session configuration (SSH or VNC).
 *
 * Fields are conditionally shown based on the selected protocol.
 */
export default function SshSessionForm({
  initial,
  onSubmit,
  onCancel,
}: SshSessionFormProps) {
  const [draft, setDraft] = useState<SshSessionDraft>(
    initial ? { ...initial } : emptySshDraft(),
  );
  const [errors, setErrors] = useState<Record<string, string>>({});

  // FE-5: sync form state when the `initial` prop changes (e.g. user
  // clicks Edit on a different session without unmounting the form).
  useEffect(() => {
    setDraft(initial ? { ...initial } : emptySshDraft());
    setErrors({});
  }, [initial?.id]);

  /** Update a single field in the draft. */
  const set = <K extends keyof SshSessionDraft>(
    key: K,
    value: SshSessionDraft[K],
  ) => {
    setDraft((prev) => ({ ...prev, [key]: value }));
    // Clear field error on change
    if (errors[key]) {
      setErrors((prev) => {
        const next = { ...prev };
        delete next[key];
        return next;
      });
    }
  };

  /** Handle protocol change — reset form to appropriate defaults. */
  const handleProtocolChange = (protocol: Protocol) => {
    if (protocol === draft.protocol) return;
    const base =
      protocol === "vnc"
        ? emptyVncDraft()
        : protocol === "rdp"
          ? emptyRdpDraft()
          : emptySshDraft();
    // Preserve label, host, and notes across protocol switches
    setDraft({
      ...base,
      label: draft.label,
      host: draft.host,
      notes: draft.notes,
    });
    setErrors({});
  };

  /** Basic client-side validation. */
  const validate = (): boolean => {
    const errs: Record<string, string> = {};
    if (!draft.label.trim()) errs.label = "Label is required";
    if (!draft.host.trim()) errs.host = "Host is required";
    if (draft.port < 1 || draft.port > 65535)
      errs.port = "Port must be 1–65535";

    // SSH-specific validation
    if (draft.protocol === "ssh") {
      if (!draft.username.trim()) errs.username = "Username is required";
      if (
        draft.auth_method === "password" &&
        (!draft.password || !draft.password.trim())
      )
        errs.password = "Password is required for password auth";
      if (
        draft.auth_method === "key" &&
        (!draft.private_key_path || !draft.private_key_path.trim())
      )
        errs.private_key_path = "Key path is required for key auth";
    }

    // RDP-specific validation
    if (draft.protocol === "rdp") {
      if (!draft.username.trim()) errs.username = "Username is required";
    }

    setErrors(errs);
    return Object.keys(errs).length === 0;
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;
    onSubmit(draft, initial?.id);
  };

  const isSsh = draft.protocol === "ssh";
  const isRdp = draft.protocol === "rdp";
  // UX-1: prefix IDs with protocol to avoid duplicate HTML id attributes
  const prefix = draft.protocol;

  return (
    <form onSubmit={handleSubmit}>
      <h2>{initial ? "Edit Host" : "New Host"}</h2>

      {/* Protocol */}
      <div className="form-group">
        <label htmlFor="protocol">Protocol</label>
        <select
          id="protocol"
          value={draft.protocol}
          onChange={(e) => handleProtocolChange(e.target.value as Protocol)}
        >
          <option value="ssh">SSH</option>
          <option value="vnc">VNC</option>
          <option value="rdp">RDP</option>
        </select>
      </div>

      {/* Label */}
      <div className="form-group">
        <label htmlFor="label">Label</label>
        <input
          id="label"
          type="text"
          placeholder={isSsh ? "e.g. Production Server" : "e.g. Dev Workstation"}
          value={draft.label}
          onChange={(e) => set("label", e.target.value)}
        />
        {errors.label && <span className="field-error">{errors.label}</span>}
      </div>

      {/* Host & Port */}
      <div className="form-row">
        <div className="form-group">
          <label htmlFor="host">Host</label>
          <input
            id="host"
            type="text"
            placeholder="192.168.1.100 or example.com"
            value={draft.host}
            onChange={(e) => set("host", e.target.value)}
          />
          {errors.host && <span className="field-error">{errors.host}</span>}
        </div>
        <div className="form-group" style={{ maxWidth: "120px" }}>
          <label htmlFor="port">Port</label>
          <input
            id="port"
            type="number"
            min={1}
            max={65535}
            value={draft.port}
            onChange={(e) => set("port", Number(e.target.value))}
          />
          {errors.port && <span className="field-error">{errors.port}</span>}
        </div>
      </div>

      {/* SSH-specific fields */}
      {isSsh && (
        <>
          {/* Username */}
          <div className="form-group">
            <label htmlFor={`${prefix}-username`}>Username</label>
            <input
              id={`${prefix}-username`}
              type="text"
              placeholder="root"
              value={draft.username}
              onChange={(e) => set("username", e.target.value)}
            />
            {errors.username && (
              <span className="field-error">{errors.username}</span>
            )}
          </div>

          {/* Auth Method */}
          <div className="form-group">
            <label htmlFor="auth_method">Authentication</label>
            <select
              id="auth_method"
              value={draft.auth_method}
              onChange={(e) => set("auth_method", e.target.value as AuthMethod)}
            >
              <option value="password">Password</option>
              <option value="key">SSH Key</option>
            </select>
          </div>

          {/* Password (conditional) */}
          {draft.auth_method === "password" && (
            <div className="form-group">
              <label htmlFor={`${prefix}-password`}>Password</label>
              <input
                id={`${prefix}-password`}
                type="password"
                placeholder="••••••••"
                value={draft.password ?? ""}
                onChange={(e) => set("password", e.target.value)}
              />
              {errors.password && (
                <span className="field-error">{errors.password}</span>
              )}
            </div>
          )}

          {/* Key path (conditional) */}
          {draft.auth_method === "key" && (
            <div className="form-group">
              <label htmlFor="private_key_path">Private Key Path</label>
              <input
                id="private_key_path"
                type="text"
                placeholder="C:\\Users\\you\\.ssh\\id_ed25519"
                value={draft.private_key_path ?? ""}
                onChange={(e) => set("private_key_path", e.target.value)}
              />
              {errors.private_key_path && (
                <span className="field-error">{errors.private_key_path}</span>
              )}
            </div>
          )}
        </>
      )}

      {/* VNC password (optional) */}
      {!isSsh && !isRdp && (
        <div className="form-group">
          <label htmlFor={`${prefix}-password`}>VNC Password (optional)</label>
          <input
            id={`${prefix}-password`}
            type="password"
            placeholder="••••••••"
            value={draft.password ?? ""}
            onChange={(e) => set("password", e.target.value)}
          />
        </div>
      )}

      {/* RDP-specific fields */}
      {isRdp && (
        <>
          {/* Username */}
          <div className="form-group">
            <label htmlFor={`${prefix}-username`}>Username</label>
            <input
              id={`${prefix}-username`}
              type="text"
              placeholder="Administrator"
              value={draft.username}
              onChange={(e) => set("username", e.target.value)}
            />
            {errors.username && (
              <span className="field-error">{errors.username}</span>
            )}
          </div>

          {/* Domain */}
          <div className="form-group">
            <label htmlFor="domain">Domain (optional)</label>
            <input
              id="domain"
              type="text"
              placeholder="WORKGROUP or corp.example.com"
              value={draft.domain ?? ""}
              onChange={(e) => set("domain", e.target.value)}
            />
          </div>

          {/* Password */}
          <div className="form-group">
            <label htmlFor={`${prefix}-password`}>Password</label>
            <input
              id={`${prefix}-password`}
              type="password"
              placeholder="••••••••"
              value={draft.password ?? ""}
              onChange={(e) => set("password", e.target.value)}
            />
          </div>
        </>
      )}

      {/* Notes */}
      <div className="form-group">
        <label htmlFor="notes">Notes (optional)</label>
        <textarea
          id="notes"
          rows={3}
          placeholder="Any additional information…"
          value={draft.notes ?? ""}
          onChange={(e) => set("notes", e.target.value)}
        />
      </div>

      {/* Actions */}
      <div className="button-row">
        <button type="submit" className="btn-primary">
          {initial ? "Update Session" : "Save Session"}
        </button>
        <button type="button" className="btn-secondary" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}
