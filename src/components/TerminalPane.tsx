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

    // Listen for SSH output from backend
    let unlistenOutput: UnlistenFn | undefined;
    let unlistenClosed: UnlistenFn | undefined;

    const setupListeners = async () => {
      unlistenOutput = await listen<{ data: number[] }>(
        `ssh-output-${connectionId}`,
        (event) => {
          const bytes = new Uint8Array(event.payload.data);
          term.write(bytes);
        },
      );

      unlistenClosed = await listen<{ reason: string }>(
        `ssh-closed-${connectionId}`,
        (event) => {
          term.writeln(`\r\n\x1b[31m[Disconnected: ${event.payload.reason}]\x1b[0m`);
          onDisconnected();
        },
      );
    };

    setupListeners();

    // Handle window resize
    const resizeObserver = new ResizeObserver(() => {
      fitAddon.fit();
      sshResize(connectionId, term.cols, term.rows).catch(() => {});
    });
    resizeObserver.observe(containerRef.current);

    return () => {
      dataDisposable.dispose();
      resizeObserver.disconnect();
      unlistenOutput?.();
      unlistenClosed?.();
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
