import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { sshSend, sshResize } from "../api";
import "@xterm/xterm/css/xterm.css";

interface TerminalPaneProps {
  /** Unique connection identifier from the backend. */
  connectionId: string;
  /** Called when the connection is closed (remotely or locally). */
  onDisconnected: () => void;
}

/**
 * Renders an interactive xterm.js terminal bound to a live SSH connection.
 *
 * Data flow:
 * - User input → `sshSend` → backend → SSH server
 * - SSH server → backend Tauri event → `terminal.write()`
 */
export default function TerminalPane({
  connectionId,
  onDisconnected,
}: TerminalPaneProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const onDisconnectedRef = useRef(onDisconnected);

  // Keep ref in sync so the listener closure never stale-captures (FE-4 fix)
  onDisconnectedRef.current = onDisconnected;

  useEffect(() => {
    if (!containerRef.current) return;

    const term = new Terminal({
      cursorBlink: true,
      fontSize: 14,
      fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', monospace",
      theme: {
        background: "#0f172a",
        foreground: "#e2e8f0",
        cursor: "#6366f1",
        selectionBackground: "#334155",
      },
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(containerRef.current);
    fitAddon.fit();
    termRef.current = term;

    // Send initial size to backend
    sshResize(connectionId, term.cols, term.rows).catch(() => {});

    // User types → send to SSH
    const dataDisposable = term.onData((data) => {
      const encoder = new TextEncoder();
      const bytes = Array.from(encoder.encode(data));
      sshSend(connectionId, bytes).catch(() => {});
    });

    // RES-2 fix: track cleanup state and collected unlisten functions.
    // If the component unmounts before listen() resolves, we set cancelled
    // and call the unlisten function as soon as it resolves.
    let cancelled = false;
    const unlisteners: UnlistenFn[] = [];

    listen<{ data: number[] }>(
      `ssh-output-${connectionId}`,
      (event) => {
        const bytes = new Uint8Array(event.payload.data);
        term.write(bytes);
      },
    ).then((fn) => {
      if (cancelled) { fn(); } else { unlisteners.push(fn); }
    });

    listen<{ reason: string }>(
      `ssh-closed-${connectionId}`,
      (event) => {
        term.writeln(`\r\n\x1b[31m[Disconnected: ${event.payload.reason}]\x1b[0m`);
        onDisconnectedRef.current();
      },
    ).then((fn) => {
      if (cancelled) { fn(); } else { unlisteners.push(fn); }
    });

    // Handle window resize with debounce (PERF-3 partial fix)
    let resizeTimeout: ReturnType<typeof setTimeout> | null = null;
    const resizeObserver = new ResizeObserver(() => {
      if (resizeTimeout) clearTimeout(resizeTimeout);
      resizeTimeout = setTimeout(() => {
        fitAddon.fit();
        sshResize(connectionId, term.cols, term.rows).catch(() => {});
      }, 50);
    });
    resizeObserver.observe(containerRef.current);

    return () => {
      cancelled = true;
      dataDisposable.dispose();
      resizeObserver.disconnect();
      if (resizeTimeout) clearTimeout(resizeTimeout);
      unlisteners.forEach((fn) => fn());
      term.dispose();
      termRef.current = null;
    };
    // connectionId is stable for the lifetime of this component
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connectionId]);

  return (
    <div
      ref={containerRef}
      className="terminal-container"
      style={{ width: "100%", height: "100%" }}
    />
  );
}
