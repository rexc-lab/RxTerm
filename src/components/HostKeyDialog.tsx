import { useEffect, useRef, useCallback } from "react";

interface HostKeyDialogProps {
  /** Remote host that presented the key. */
  host: string;
  /** Port number. */
  port: number;
  /** Key fingerprint to display to the user. */
  fingerprint: string;
  /** Called when the user accepts the key. */
  onAccept: () => void;
  /** Called when the user rejects the key. */
  onReject: () => void;
}

/**
 * Modal dialog prompting the user to accept or reject an unknown SSH host key.
 *
 * UX-3: Implements focus trapping, Escape key dismissal, role="dialog",
 * and aria-modal for proper accessibility.
 */
export default function HostKeyDialog({
  host,
  port,
  fingerprint,
  onAccept,
  onReject,
}: HostKeyDialogProps) {
  const dialogRef = useRef<HTMLDivElement>(null);

  // Focus the dialog when it mounts
  useEffect(() => {
    dialogRef.current?.focus();
  }, []);

  // Trap focus within the dialog and handle Escape key
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Escape") {
        onReject();
        return;
      }
      if (e.key === "Tab") {
        const dialog = dialogRef.current;
        if (!dialog) return;
        const focusable = dialog.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
        );
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (e.shiftKey) {
          if (document.activeElement === first) {
            e.preventDefault();
            last.focus();
          }
        } else {
          if (document.activeElement === last) {
            e.preventDefault();
            first.focus();
          }
        }
      }
    },
    [onReject],
  );

  return (
    <div className="dialog-overlay" onClick={onReject}>
      <div
        ref={dialogRef}
        className="dialog-box"
        role="dialog"
        aria-modal="true"
        aria-labelledby="hk-dialog-title"
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        <h3 id="hk-dialog-title">Unknown Host Key</h3>
        <p>
          The server at <strong>{host}:{port}</strong> presented an unrecognized
          host key. This is normal for first-time connections, but could indicate
          a man-in-the-middle attack if you&apos;ve connected before.
        </p>
        <div className="dialog-fingerprint">
          <code>{fingerprint}</code>
        </div>
        <p>Do you want to trust this key and continue connecting?</p>
        <div className="dialog-actions">
          <button className="btn-primary" onClick={onAccept}>
            Accept &amp; Connect
          </button>
          <button className="btn-secondary" onClick={onReject}>
            Reject
          </button>
        </div>
      </div>
    </div>
  );
}
