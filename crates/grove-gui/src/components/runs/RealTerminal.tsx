import { useEffect, useRef, useCallback } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { C } from "@/lib/theme";

interface PtyOutputPayload {
  data: string;
}

interface PtyOpenResult {
  pty_id: string;
  is_new: boolean;
}

interface SshTerminalConfig {
  target: string;
  port?: number | null;
  remotePath?: string | null;
}

interface RealTerminalProps {
  /** Identifies the PTY session — one per conversation, persists for app lifetime. */
  conversationId: string;
  /** Working directory when spawning a fresh shell. Defaults to $HOME. */
  cwd?: string;
  /** Optional SSH launch config for remote project terminals. */
  ssh?: SshTerminalConfig | null;
}

export function RealTerminal({ conversationId, cwd, ssh }: RealTerminalProps) {
  const ptyId = `${conversationId}:0`;
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const unlistenRef = useRef<UnlistenFn | null>(null);
  const unloadingRef = useRef(false);

  const closePty = useCallback(() => {
    invoke("pty_close_new", { ptyId }).catch(() => {});
  }, [ptyId]);

  const connectPty = useCallback(
    async (term: Terminal, fitAddon: FitAddon) => {
      if (unloadingRef.current) return;

      // Subscribe before spawning/resuming so initial PTY output is not lost.
      const unlisten = await listen<PtyOutputPayload>(
        `pty:output:${ptyId}`,
        (event) => {
          term.write(event.payload.data);
        },
      );
      unlistenRef.current = unlisten;

      const dims = fitAddon.proposeDimensions();
      const cols = dims?.cols ?? 80;
      const rows = dims?.rows ?? 24;

      await invoke<PtyOpenResult>("pty_open", {
        ptyId,
        cwd: cwd ?? null,
        sshTarget: ssh?.target ?? null,
        sshPort: ssh?.port ?? null,
        sshRemotePath: ssh?.remotePath ?? null,
        cols,
        rows,
      });
      if (unloadingRef.current) return;

      // Forward keystrokes -> PTY stdin.
      term.onData((data) => {
        if (!unloadingRef.current) {
          invoke("pty_write_new", { ptyId, data }).catch(() => {});
        }
      });

      // Propagate xterm resize -> PTY.
      term.onResize(({ cols: c, rows: r }) => {
        if (!unloadingRef.current) {
          invoke("pty_resize_new", { ptyId, cols: c, rows: r }).catch(
            () => {},
          );
        }
      });
    },
    [ptyId, cwd, ssh?.port, ssh?.remotePath, ssh?.target],
  );

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    unloadingRef.current = false;
    let disposed = false;
    let resizeFrame = 0;
    let resizeObserver: ResizeObserver | null = null;
    let handleWindowResize: (() => void) | null = null;
    const markUnloading = () => {
      unloadingRef.current = true;
      closePty();
    };

    const syncPtySize = (term: Terminal) => {
      if (!unloadingRef.current) {
        invoke("pty_resize_new", {
          ptyId,
          cols: term.cols,
          rows: term.rows,
        }).catch(() => {});
      }
    };

    const init = async () => {
      if ("fonts" in document) {
        await document.fonts.ready;
      }
      if (disposed) return;

      const term = new Terminal({
        cursorBlink: true,
        fontFamily: C.mono,
        fontSize: 12,
        lineHeight: 1.45,
        letterSpacing: 0,
        scrollback: 5000,
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
      });

      const fitAddon = new FitAddon();
      term.loadAddon(fitAddon);
      term.open(container);

      const fitAndSync = () => {
        fitAddon.fit();
        if (term.cols > 0 && term.rows > 0) {
          syncPtySize(term);
        }
      };

      requestAnimationFrame(() => {
        if (!disposed) {
          fitAndSync();
        }
      });

      termRef.current = term;
      fitAddonRef.current = fitAddon;

      connectPty(term, fitAddon).catch((e: unknown) => {
        term.writeln(
          `\r\n\x1b[31mFailed to connect terminal: ${e}\x1b[0m\r\n`,
        );
      });

      const scheduleFit = () => {
        cancelAnimationFrame(resizeFrame);
        resizeFrame = requestAnimationFrame(() => {
          if (!disposed) {
            fitAndSync();
          }
        });
      };

      resizeObserver = new ResizeObserver(() => {
        scheduleFit();
      });
      resizeObserver.observe(container);
      handleWindowResize = scheduleFit;
      window.addEventListener("resize", handleWindowResize);
    };

    void init();
    window.addEventListener("beforeunload", markUnloading);
    window.addEventListener("pagehide", markUnloading);

    return () => {
      disposed = true;
      cancelAnimationFrame(resizeFrame);
      resizeObserver?.disconnect();
      if (handleWindowResize) {
        window.removeEventListener("resize", handleWindowResize);
      }
      window.removeEventListener("beforeunload", markUnloading);
      window.removeEventListener("pagehide", markUnloading);
      unlistenRef.current?.();
      termRef.current?.dispose();
      termRef.current = null;
      fitAddonRef.current = null;
    };
  }, [ptyId, closePty, connectPty]);

  return (
    <div
      ref={containerRef}
      style={{
        flex: 1,
        minHeight: 0,
        padding: "6px 4px",
        boxSizing: "border-box",
        background: "#15171E",
      }}
    />
  );
}
