import { useEffect, useRef, useState } from "react";
import RFB from "@novnc/novnc/lib/rfb";

interface VncPaneProps {
  /** Unique connection identifier from the backend. */
  connectionId: string;
  /** Local WebSocket port the proxy is listening on. */
  wsPort: number;
  /** VNC password for the RFB handshake (optional). */
  password?: string;
  /** Called when the user explicitly closes the tab. */
  onDisconnected: () => void;
  /** Called to re-establish the connection (reconnect). */
  onReconnect: () => void;
}

/**
 * Renders a VNC remote desktop session using noVNC.
 *
 * On connection failure or unclean disconnect, shows an error overlay
 * with a Reconnect button instead of closing the tab.
 */
export default function VncPane({
  connectionId,
  wsPort,
  password,
  onDisconnected,
  onReconnect,
}: VncPaneProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const rfbRef = useRef<RFB | null>(null);
  const passwordRef = useRef(password);
  const onDisconnectedRef = useRef(onDisconnected);
  const [error, setError] = useState<string | null>(null);

  // Keep refs in sync with latest props
  passwordRef.current = password;
  onDisconnectedRef.current = onDisconnected;

  useEffect(() => {
    if (!containerRef.current) return;
    setError(null);

    const url = `ws://127.0.0.1:${wsPort}`;

    const rfb = new RFB(containerRef.current, url, {
      credentials: passwordRef.current ? { password: passwordRef.current } : undefined,
    });
    // Scale the remote desktop to fit the container — keeps VNC inside its
    // tab pane instead of overflowing to full screen.
    rfb.scaleViewport = true;
    // Do NOT resize the remote session to match the container — that causes
    // the server to resize and the canvas to grow unbounded.
    rfb.resizeSession = false;
    rfbRef.current = rfb;

    rfb.addEventListener("disconnect", (e: CustomEvent) => {
      const clean = e.detail?.clean ?? false;
      if (!clean) {
        // Show error in the pane instead of closing the tab
        setError("VNC connection lost unexpectedly.");
      } else {
        // Clean disconnect (user-initiated) — close the tab
        onDisconnectedRef.current();
      }
    });

    rfb.addEventListener("credentialsrequired", () => {
      if (passwordRef.current) {
        rfb.sendCredentials({ password: passwordRef.current });
      } else {
        setError("VNC server requires a password but none was provided.");
      }
    });

    return () => {
      if (rfbRef.current) {
        rfbRef.current.disconnect();
        rfbRef.current = null;
      }
    };
  }, [connectionId, wsPort]);

  if (error) {
    return (
      <div className="pane-error">
        <div className="pane-error-icon">&#x26A0;</div>
        <div className="pane-error-title">Connection Failed</div>
        <div className="pane-error-message">{error}</div>
        <div className="pane-error-actions">
          <button className="btn-primary" onClick={onReconnect}>
            Reconnect
          </button>
          <button className="btn-secondary" onClick={onDisconnected}>
            Close Tab
          </button>
        </div>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="vnc-container"
      style={{ width: "100%", height: "100%" }}
    />
  );
}
