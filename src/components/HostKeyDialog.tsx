import { useEffect, useRef, useCallback } from "react";

interface HostKeyDialogProps {
  /** Remote host that presented the key. */
  host: string;
  /** Port number. */
  port: number;
  /** Key fingerprint to display to the user. */
  fingerprint: string;
  /** True when a DIFFERENT key was previously accepted (potential MITM). */
  changed: boolean;
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
  changed,
  onAccept,
  onReject,
}: HostKeyDialogProps) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const rejectRef = useRef<HTMLButtonElement>(null);

  // On mount, focus Reject for the changed-key (danger) variant so the
  // safe action is the keyboard default; otherwise focus the dialog
  // container. This effect runs after React's commit-phase autoFocus, so
  // it — not a button autoFocus attribute — decides the initial focus.
  useEffect(() => {
    if (changed && rejectRef.current) {
      rejectRef.current.focus();
    } else {
      dialogRef.current?.focus();
    }
  }, [changed]);

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
        className={changed ? "dialog-box dialog-box-danger" : "dialog-box"}
        role="dialog"
        aria-modal="true"
        aria-labelledby="hk-dialog-title"
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        {changed ? (
          <>
            <h3 id="hk-dialog-title" className="dialog-title-danger">
              ⚠ Host Key Changed
            </h3>
            <p>
              <strong>
                The host key for {host}:{port} is DIFFERENT from the key you
                previously accepted.
              </strong>{" "}
              This can mean the server was reinstalled or its keys were
              rotated — but it can also mean someone is intercepting your
              connection (a man-in-the-middle attack).
            </p>
            <div className="dialog-fingerprint">
              <code>{fingerprint}</code>
            </div>
            <p>
              Do not continue unless you can verify the new key with the
              server administrator. Accepting will permanently replace the
              stored key.
            </p>
          </>
        ) : (
          <>
            <h3 id="hk-dialog-title">Unknown Host Key</h3>
            <p>
              The server at <strong>{host}:{port}</strong> presented an
              unrecognized host key. This is normal for first-time
              connections, but could indicate a man-in-the-middle attack if
              you&apos;ve connected before.
            </p>
            <div className="dialog-fingerprint">
              <code>{fingerprint}</code>
            </div>
            <p>Do you want to trust this key and continue connecting?</p>
          </>
        )}
        <div className="dialog-actions">
          {changed ? (
            <>
              <button
                ref={rejectRef}
                className="btn-secondary"
                onClick={onReject}
              >
                Reject (recommended)
              </button>
              <button className="btn-danger" onClick={onAccept}>
                Replace Key &amp; Connect
              </button>
            </>
          ) : (
            <>
              <button className="btn-primary" onClick={onAccept}>
                Accept &amp; Connect
              </button>
              <button className="btn-secondary" onClick={onReject}>
                Reject
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
