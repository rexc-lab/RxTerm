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
 */
export default function HostKeyDialog({
  host,
  port,
  fingerprint,
  onAccept,
  onReject,
}: HostKeyDialogProps) {
  return (
    <div className="dialog-overlay">
      <div className="dialog-box">
        <h3>Unknown Host Key</h3>
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
