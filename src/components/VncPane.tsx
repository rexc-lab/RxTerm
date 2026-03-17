import { useEffect, useRef } from "react";
import RFB from "@novnc/novnc/lib/rfb";

interface VncPaneProps {
  /** Unique connection identifier from the backend. */
  connectionId: string;
  /** Local WebSocket port the proxy is listening on. */
  wsPort: number;
  /** VNC password for the RFB handshake (optional). */
  password?: string;
  /** Called when the VNC connection is closed. */
  onDisconnected: () => void;
}

/**
 * Renders a VNC remote desktop session using noVNC.
 *
 * Data flow:
 * - noVNC connects via WebSocket to a local proxy (127.0.0.1:wsPort)
 * - The Rust backend proxies the WebSocket to the real VNC server
 */
export default function VncPane({
  connectionId,
  wsPort,
  password,
  onDisconnected,
}: VncPaneProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const rfbRef = useRef<RFB | null>(null);
  const passwordRef = useRef(password);
  const onDisconnectedRef = useRef(onDisconnected);

  // Keep refs in sync with latest props
  passwordRef.current = password;
  onDisconnectedRef.current = onDisconnected;

  useEffect(() => {
    if (!containerRef.current) return;

    const url = `ws://127.0.0.1:${wsPort}`;

    const rfb = new RFB(containerRef.current, url, {
      credentials: passwordRef.current ? { password: passwordRef.current } : undefined,
    });
    rfb.scaleViewport = true;
    rfb.resizeSession = true;
    rfbRef.current = rfb;

    rfb.addEventListener("disconnect", (e: CustomEvent) => {
      const clean = e.detail?.clean ?? false;
      if (!clean) {
        console.warn(`[VNC ${connectionId}] unclean disconnect`);
      }
      onDisconnectedRef.current();
    });

    rfb.addEventListener("credentialsrequired", () => {
      if (passwordRef.current) {
        rfb.sendCredentials({ password: passwordRef.current });
      } else {
        rfb.disconnect();
      }
    });

    return () => {
      if (rfbRef.current) {
        rfbRef.current.disconnect();
        rfbRef.current = null;
      }
    };
  }, [connectionId, wsPort]);

  return (
    <div
      ref={containerRef}
      className="vnc-container"
      style={{ width: "100%", height: "100%" }}
    />
  );
}
