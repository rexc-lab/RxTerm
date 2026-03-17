/** Minimal type declarations for @novnc/novnc RFB client. */
declare module "@novnc/novnc/lib/rfb" {
  interface RFBCredentials {
    password?: string;
    username?: string;
    target?: string;
  }

  interface RFBOptions {
    shared?: boolean;
    credentials?: RFBCredentials;
    repeaterID?: string;
    wsProtocols?: string[];
  }

  class RFB {
    constructor(target: HTMLElement, urlOrChannel: string | WebSocket, options?: RFBOptions);

    /** Scale the remote session to fit the container. */
    scaleViewport: boolean;
    /** Request the remote server to resize to the container dimensions. */
    resizeSession: boolean;
    /** Make the session view-only (no input events sent). */
    viewOnly: boolean;
    /** Show a dot cursor instead of the remote cursor. */
    showDotCursor: boolean;
    /** Background color of the viewport. */
    background: string;
    /** Quality level for JPEG encoding (0–9). */
    qualityLevel: number;
    /** Compression level (0–9). */
    compressionLevel: number;

    /** Capabilities reported by the server. */
    readonly capabilities: { power: boolean };

    /** Disconnect from the VNC server. */
    disconnect(): void;
    /** Send credentials (e.g. VNC password). */
    sendCredentials(credentials: RFBCredentials): void;
    /** Send a Ctrl-Alt-Del key sequence. */
    sendCtrlAltDel(): void;
    /** Send a clipboard string. */
    clipboardPasteFrom(text: string): void;
    /** Request a power action (shutdown / reboot). */
    machineShutdown(): void;
    machineReboot(): void;
    machineReset(): void;

    /** Register an event listener. */
    addEventListener(type: string, listener: (e: CustomEvent) => void): void;
    /** Remove an event listener. */
    removeEventListener(type: string, listener: (e: CustomEvent) => void): void;
  }

  export default RFB;
}
