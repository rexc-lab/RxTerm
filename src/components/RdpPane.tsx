import { useEffect, useRef, useCallback } from "react";
import type { MouseEvent, WheelEvent, KeyboardEvent } from "react";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type { RdpFramePayload, RdpDisconnectedPayload } from "../types";
import { rdpMouseEvent, rdpKeyEvent } from "../api";

interface RdpPaneProps {
  /** Unique connection identifier from the backend. */
  connectionId: string;
  /** Called when the RDP session is closed (by server or error). */
  onDisconnected: () => void;
}

/** PERF-4 fix: scancode map as a module-level constant. */
const SCANCODE_MAP: Record<string, number> = {
  Escape: 0x01,
  Digit1: 0x02, Digit2: 0x03, Digit3: 0x04, Digit4: 0x05,
  Digit5: 0x06, Digit6: 0x07, Digit7: 0x08, Digit8: 0x09,
  Digit9: 0x0A, Digit0: 0x0B,
  Minus: 0x0C, Equal: 0x0D, Backspace: 0x0E,
  Tab: 0x0F,
  KeyQ: 0x10, KeyW: 0x11, KeyE: 0x12, KeyR: 0x13, KeyT: 0x14,
  KeyY: 0x15, KeyU: 0x16, KeyI: 0x17, KeyO: 0x18, KeyP: 0x19,
  BracketLeft: 0x1A, BracketRight: 0x1B, Enter: 0x1C,
  ControlLeft: 0x1D,
  KeyA: 0x1E, KeyS: 0x1F, KeyD: 0x20, KeyF: 0x21, KeyG: 0x22,
  KeyH: 0x23, KeyJ: 0x24, KeyK: 0x25, KeyL: 0x26,
  Semicolon: 0x27, Quote: 0x28, Backquote: 0x29,
  ShiftLeft: 0x2A, Backslash: 0x2B,
  KeyZ: 0x2C, KeyX: 0x2D, KeyC: 0x2E, KeyV: 0x2F,
  KeyB: 0x30, KeyN: 0x31, KeyM: 0x32,
  Comma: 0x33, Period: 0x34, Slash: 0x35,
  ShiftRight: 0x36,
  NumpadMultiply: 0x37,
  AltLeft: 0x38,
  Space: 0x39,
  CapsLock: 0x3A,
  F1: 0x3B, F2: 0x3C, F3: 0x3D, F4: 0x3E,
  F5: 0x3F, F6: 0x40, F7: 0x41, F8: 0x42,
  F9: 0x43, F10: 0x44,
  NumLock: 0x45, ScrollLock: 0x46,
  Numpad7: 0x47, Numpad8: 0x48, Numpad9: 0x49, NumpadSubtract: 0x4A,
  Numpad4: 0x4B, Numpad5: 0x4C, Numpad6: 0x4D, NumpadAdd: 0x4E,
  Numpad1: 0x4F, Numpad2: 0x50, Numpad3: 0x51, Numpad0: 0x52, NumpadDecimal: 0x53,
  F11: 0x57, F12: 0x58,
  // Extended keys (0xE0 prefix → high bit set in packed u16)
  NumpadEnter:  0xE01C,
  ControlRight: 0xE01D,
  NumpadDivide: 0xE035,
  AltRight:     0xE038,
  Home:         0xE047,
  ArrowUp:      0xE048,
  PageUp:       0xE049,
  ArrowLeft:    0xE04B,
  ArrowRight:   0xE04D,
  End:          0xE04F,
  ArrowDown:    0xE050,
  PageDown:     0xE051,
  Insert:       0xE052,
  Delete:       0xE053,
  MetaLeft:     0xE05B,
  MetaRight:    0xE05C,
  ContextMenu:  0xE05D,
};

/**
 * Renders an RDP remote-desktop session on an HTML5 canvas.
 *
 * Data flow:
 * - Rust backend emits `rdp-frame` events with RGBA pixel data for dirty rects
 * - This component paints each dirty rect onto the canvas using `putImageData`
 * - Keyboard / mouse events are forwarded to the backend via Tauri commands
 */
export default function RdpPane({ connectionId, onDisconnected }: RdpPaneProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const connectionIdRef = useRef(connectionId);
  const onDisconnectedRef = useRef(onDisconnected);
  // PERF-5 fix: cache the 2D context in a ref
  const ctxRef = useRef<CanvasRenderingContext2D | null>(null);

  // Keep refs in sync with latest props so closures never stale-capture them
  connectionIdRef.current = connectionId;
  onDisconnectedRef.current = onDisconnected;

  // ── Canvas helper: decode base64 RGBA and blit it to the canvas ───────────
  const blitFrame = useCallback((payload: RdpFramePayload) => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    // Resize canvas to match the full desktop on first frame
    if (canvas.width !== payload.full_width || canvas.height !== payload.full_height) {
      canvas.width = payload.full_width;
      canvas.height = payload.full_height;
      // Re-acquire context after resize
      ctxRef.current = canvas.getContext("2d");
    }

    if (!ctxRef.current) {
      ctxRef.current = canvas.getContext("2d");
    }
    const ctx = ctxRef.current;
    if (!ctx) return;

    // PERF-1 fix: use Uint8Array.from for efficient base64 → binary conversion
    const raw = atob(payload.data);
    const bytes = Uint8Array.from(raw, (c) => c.charCodeAt(0));

    // Create ImageData and paint the dirty rect
    const imageData = new ImageData(
      new Uint8ClampedArray(bytes.buffer),
      payload.width,
      payload.height,
    );
    ctx.putImageData(imageData, payload.x, payload.y);
  }, []);

  // ── Subscribe to backend events ───────────────────────────────────────────
  useEffect(() => {
    // RES-3 fix: track cleanup state so late-resolving promises are handled.
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];

    listen<RdpFramePayload>("rdp-frame", (event) => {
      if (event.payload.connection_id === connectionIdRef.current) {
        blitFrame(event.payload);
      }
    }).then((fn) => {
      if (cancelled) { fn(); } else { unlisteners.push(fn); }
    });

    listen<RdpDisconnectedPayload>("rdp-disconnected", (event) => {
      if (event.payload.connection_id === connectionIdRef.current) {
        onDisconnectedRef.current();
      }
    }).then((fn) => {
      if (cancelled) { fn(); } else { unlisteners.push(fn); }
    });

    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, [blitFrame]);

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
    rdpMouseEvent(connectionIdRef.current, x, y, null, false, null).catch(() => {});
  }, []);

  const handleMouseDown = useCallback((e: MouseEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    const [x, y] = getCanvasCoords(e);
    // button: 0=left, 1=middle, 2=right  (matches MouseEvent.button)
    rdpMouseEvent(connectionIdRef.current, x, y, e.button, true, null).catch(() => {});
  }, []);

  const handleMouseUp = useCallback((e: MouseEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    const [x, y] = getCanvasCoords(e);
    rdpMouseEvent(connectionIdRef.current, x, y, e.button, false, null).catch(() => {});
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
    // Positive deltaY → scroll down → negative rotation_units
    const delta = e.deltaY < 0 ? 120 : -120;
    rdpMouseEvent(connectionIdRef.current, x, y, null, false, delta).catch(() => {});
  }, []);

  const handleContextMenu = useCallback((e: MouseEvent<HTMLCanvasElement>) => {
    // Prevent the browser's context menu so right-click passes through to RDP
    e.preventDefault();
  }, []);

  // ── Keyboard events ───────────────────────────────────────────────────────
  // We listen on the container div with tabIndex so it can receive focus/key events.
  const handleKeyDown = useCallback((e: KeyboardEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    const scancode = SCANCODE_MAP[e.code] ?? null;
    if (scancode !== null) {
      rdpKeyEvent(connectionIdRef.current, scancode, true).catch(() => {});
    }
  }, []);

  const handleKeyUp = useCallback((e: KeyboardEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    const scancode = SCANCODE_MAP[e.code] ?? null;
    if (scancode !== null) {
      rdpKeyEvent(connectionIdRef.current, scancode, false).catch(() => {});
    }
  }, []);

  return (
    <div
      ref={containerRef}
      className="rdp-container"
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
