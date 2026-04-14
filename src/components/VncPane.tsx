import { useEffect, useRef, useCallback, useState } from "react";
import type { MouseEvent, WheelEvent, KeyboardEvent } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type { VncFramePayload, VncDisconnectedPayload, VncClipboardPayload } from "../types";
import { vncMouseEvent, vncKeyEvent, vncSendClipboard } from "../api";

interface VncPaneProps {
  /** Unique connection identifier from the backend. */
  connectionId: string;
  /** Called when the user explicitly closes the tab. */
  onDisconnected: () => void;
  /** Called to re-establish the connection (reconnect). */
  onReconnect: () => void;
}

/**
 * X11 keysym map for browser KeyboardEvent.code values.
 *
 * VNC uses X11 keysyms (not scancodes like RDP). This map covers
 * the most common keys. Printable characters use their Unicode
 * code point directly via KeyboardEvent.key.
 */
const KEYSYM_MAP: Record<string, number> = {
  Backspace: 0xff08,
  Tab: 0xff09,
  Enter: 0xff0d,
  Escape: 0xff1b,
  Delete: 0xffff,
  Home: 0xff50,
  End: 0xff57,
  PageUp: 0xff55,
  PageDown: 0xff56,
  ArrowLeft: 0xff51,
  ArrowUp: 0xff52,
  ArrowRight: 0xff53,
  ArrowDown: 0xff54,
  Insert: 0xff63,
  F1: 0xffbe, F2: 0xffbf, F3: 0xffc0, F4: 0xffc1,
  F5: 0xffc2, F6: 0xffc3, F7: 0xffc4, F8: 0xffc5,
  F9: 0xffc6, F10: 0xffc7, F11: 0xffc8, F12: 0xffc9,
  ShiftLeft: 0xffe1, ShiftRight: 0xffe2,
  ControlLeft: 0xffe3, ControlRight: 0xffe4,
  AltLeft: 0xffe9, AltRight: 0xffea,
  MetaLeft: 0xffeb, MetaRight: 0xffec,
  CapsLock: 0xffe5,
  NumLock: 0xff7f,
  ScrollLock: 0xff14,
  // Numpad
  NumpadEnter: 0xff8d,
  NumpadMultiply: 0xffaa,
  NumpadAdd: 0xffab,
  NumpadSubtract: 0xffad,
  NumpadDecimal: 0xffae,
  NumpadDivide: 0xffaf,
  Numpad0: 0xffb0, Numpad1: 0xffb1, Numpad2: 0xffb2, Numpad3: 0xffb3,
  Numpad4: 0xffb4, Numpad5: 0xffb5, Numpad6: 0xffb6, Numpad7: 0xffb7,
  Numpad8: 0xffb8, Numpad9: 0xffb9,
  Space: 0x0020,
};

/**
 * Convert a browser KeyboardEvent to an X11 keysym.
 *
 * For printable characters, use the Unicode code point.
 * For special keys, use the KEYSYM_MAP lookup.
 */
function keyEventToKeysym(e: KeyboardEvent<HTMLDivElement>): number | null {
  // Check special keys first
  const mapped = KEYSYM_MAP[e.code];
  if (mapped !== undefined) return mapped;

  // For printable characters, use the key value's char code
  if (e.key.length === 1) {
    return e.key.charCodeAt(0);
  }

  return null;
}

/**
 * Renders a VNC remote-desktop session on an HTML5 canvas.
 *
 * Data flow:
 * - Rust backend emits `vnc-frame` events with RGBA pixel data for dirty rects
 * - This component paints each dirty rect onto the canvas using `putImageData`
 * - Keyboard / mouse events are forwarded to the backend via Tauri commands
 * - Clipboard events from the server are written to the local clipboard
 */
export default function VncPane({ connectionId, onDisconnected, onReconnect }: VncPaneProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const connectionIdRef = useRef(connectionId);
  const onDisconnectedRef = useRef(onDisconnected);
  const ctxRef = useRef<CanvasRenderingContext2D | null>(null);
  const [error, setError] = useState<string | null>(null);

  connectionIdRef.current = connectionId;
  onDisconnectedRef.current = onDisconnected;

  // ── Canvas helper: decode base64 RGBA and blit it to the canvas ───────────
  const blitFrame = useCallback((payload: VncFramePayload) => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    if (canvas.width !== payload.full_width || canvas.height !== payload.full_height) {
      canvas.width = payload.full_width;
      canvas.height = payload.full_height;
      ctxRef.current = canvas.getContext("2d");
    }

    if (!ctxRef.current) {
      ctxRef.current = canvas.getContext("2d");
    }
    const ctx = ctxRef.current;
    if (!ctx) return;

    const raw = atob(payload.data);
    const bytes = Uint8Array.from(raw, (c) => c.charCodeAt(0));

    const imageData = new ImageData(
      new Uint8ClampedArray(bytes.buffer),
      payload.width,
      payload.height,
    );
    ctx.putImageData(imageData, payload.x, payload.y);
  }, []);

  // ── Subscribe to backend events ───────────────────────────────────────────
  useEffect(() => {
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];

    listen<VncFramePayload>("vnc-frame", (event) => {
      if (event.payload.connection_id === connectionIdRef.current) {
        blitFrame(event.payload);
      }
    }).then((fn) => {
      if (cancelled) { fn(); } else { unlisteners.push(fn); }
    });

    listen<VncDisconnectedPayload>("vnc-disconnected", (event) => {
      if (event.payload.connection_id === connectionIdRef.current) {
        const reason = event.payload.reason || "Connection closed";
        setError(`VNC session ended: ${reason}`);
      }
    }).then((fn) => {
      if (cancelled) { fn(); } else { unlisteners.push(fn); }
    });

    listen<VncClipboardPayload>("vnc-clipboard", (event) => {
      if (event.payload.connection_id === connectionIdRef.current) {
        navigator.clipboard.writeText(event.payload.text).catch(() => {});
      }
    }).then((fn) => {
      if (cancelled) { fn(); } else { unlisteners.push(fn); }
    });

    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, [blitFrame]);

  // ── Send local clipboard to VNC server on paste ──────────────────────────
  useEffect(() => {
    const handlePaste = (e: ClipboardEvent) => {
      const text = e.clipboardData?.getData("text/plain");
      if (text) {
        vncSendClipboard(connectionIdRef.current, text).catch(() => {});
      }
    };

    const container = containerRef.current;
    if (container) {
      container.addEventListener("paste", handlePaste);
      return () => container.removeEventListener("paste", handlePaste);
    }
  }, []);

  // ── Mouse events ──────────────────────────────────────────────────────────
  const getCanvasCoords = (e: MouseEvent<HTMLCanvasElement>): [number, number] => {
    const canvas = canvasRef.current;
    if (!canvas) return [0, 0];
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    return [
      Math.round((e.clientX - rect.left) * scaleX),
      Math.round((e.clientY - rect.top) * scaleY),
    ];
  };

  const handleMouseMove = useCallback((e: MouseEvent<HTMLCanvasElement>) => {
    const [x, y] = getCanvasCoords(e);
    vncMouseEvent(connectionIdRef.current, x, y, null, false, null).catch(() => {});
  }, []);

  const handleMouseDown = useCallback((e: MouseEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    const [x, y] = getCanvasCoords(e);
    vncMouseEvent(connectionIdRef.current, x, y, e.button, true, null).catch(() => {});
  }, []);

  const handleMouseUp = useCallback((e: MouseEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    const [x, y] = getCanvasCoords(e);
    vncMouseEvent(connectionIdRef.current, x, y, e.button, false, null).catch(() => {});
  }, []);

  const handleWheel = useCallback((e: WheelEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    const x = Math.round((e.clientX - rect.left) * scaleX);
    const y = Math.round((e.clientY - rect.top) * scaleY);
    const delta = e.deltaY < 0 ? 120 : -120;
    vncMouseEvent(connectionIdRef.current, x, y, null, false, delta).catch(() => {});
  }, []);

  const handleContextMenu = useCallback((e: MouseEvent<HTMLCanvasElement>) => {
    e.preventDefault();
  }, []);

  // ── Keyboard events ───────────────────────────────────────────────────────
  const handleKeyDown = useCallback((e: KeyboardEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    const keysym = keyEventToKeysym(e);
    if (keysym !== null) {
      vncKeyEvent(connectionIdRef.current, keysym, true).catch(() => {});
    }
  }, []);

  const handleKeyUp = useCallback((e: KeyboardEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    const keysym = keyEventToKeysym(e);
    if (keysym !== null) {
      vncKeyEvent(connectionIdRef.current, keysym, false).catch(() => {});
    }
  }, []);

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
      tabIndex={0}
      onKeyDown={handleKeyDown}
      onKeyUp={handleKeyUp}
      style={{ width: "100%", height: "100%", outline: "none", overflow: "auto" }}
    >
      <canvas
        ref={canvasRef}
        onMouseMove={handleMouseMove}
        onMouseDown={handleMouseDown}
        onMouseUp={handleMouseUp}
        onWheel={handleWheel}
        onContextMenu={handleContextMenu}
        style={{ display: "block", maxWidth: "100%", cursor: "default" }}
      />
    </div>
  );
}
