import { Terminal, type ITerminalOptions } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { C } from "@/lib/theme";

/** xterm.js terminal options — Grove dark theme. */
const TERMINAL_OPTIONS: ITerminalOptions = {
  cursorBlink: true,
  fontFamily: C.mono,
  fontSize: 13,
  lineHeight: 1.4,
  letterSpacing: 0,
  scrollback: 50_000,
  convertEol: true,
  allowTransparency: false,
  theme: {
    background: "#15171E",
    foreground: "#DDE0E7",
    cursor: "#31B97B",
    cursorAccent: "#15171E",
    selectionBackground: "rgba(49,185,123,0.22)",
    black: "#24272F",
    red: "#EF4444",
    green: "#31B97B",
    yellow: "#F59E0B",
    blue: "#3B82F6",
    magenta: "#818CF8",
    cyan: "#67E8F9",
    white: "#A1A6AE",
    brightBlack: "#52575F",
    brightRed: "#F87171",
    brightGreen: "#31B97B",
    brightYellow: "#FBBF24",
    brightBlue: "#7DD3FC",
    brightMagenta: "#A5B4FC",
    brightCyan: "#A5F3FC",
    brightWhite: "#DDE0E7",
  },
};

interface TerminalSession {
  terminal: Terminal;
  fitAddon: FitAddon;
  webLinksAddon: WebLinksAddon;
  attached: boolean;
}

class SessionRegistryImpl {
  private sessions = new Map<string, TerminalSession>();

  attach(ptyId: string, container: HTMLDivElement): Terminal {
    const existing = this.sessions.get(ptyId);
    if (existing) {
      if (!existing.attached) {
        this._moveToContainer(existing, container);
      }
      return existing.terminal;
    }

    const terminal = new Terminal(TERMINAL_OPTIONS);
    const fitAddon = new FitAddon();
    const webLinksAddon = new WebLinksAddon();

    terminal.loadAddon(fitAddon);
    terminal.loadAddon(webLinksAddon);
    terminal.open(container);

    requestAnimationFrame(() => fitAddon.fit());

    this.sessions.set(ptyId, {
      terminal,
      fitAddon,
      webLinksAddon,
      attached: true,
    });

    return terminal;
  }

  detach(ptyId: string): void {
    const session = this.sessions.get(ptyId);
    if (session && session.attached) {
      const el = session.terminal.element;
      if (el?.parentElement) {
        el.parentElement.removeChild(el);
      }
      session.attached = false;
    }
  }

  reattach(ptyId: string, container: HTMLDivElement): void {
    const session = this.sessions.get(ptyId);
    if (session && !session.attached) {
      this._moveToContainer(session, container);
    }
  }

  /** Move a detached terminal's DOM into a new container (never calls open() twice). */
  private _moveToContainer(session: TerminalSession, container: HTMLDivElement): void {
    const el = session.terminal.element;
    if (el) {
      container.appendChild(el);
    }
    session.attached = true;
    requestAnimationFrame(() => {
      session.fitAddon.fit();
      session.terminal.refresh(0, session.terminal.rows - 1);
    });
  }

  getFitAddon(ptyId: string): FitAddon | undefined {
    return this.sessions.get(ptyId)?.fitAddon;
  }

  get(ptyId: string): Terminal | undefined {
    return this.sessions.get(ptyId)?.terminal;
  }

  dispose(ptyId: string): void {
    const session = this.sessions.get(ptyId);
    if (session) {
      session.terminal.dispose();
      this.sessions.delete(ptyId);
    }
  }

  disposeAll(): void {
    for (const [, session] of this.sessions) {
      session.terminal.dispose();
    }
    this.sessions.clear();
  }
}

export const SessionRegistry = new SessionRegistryImpl();
