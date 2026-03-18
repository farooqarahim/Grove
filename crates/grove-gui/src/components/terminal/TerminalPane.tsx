import { useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { IDisposable } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

import { SessionRegistry } from "./SessionRegistry";
import type { PtyOutputPayload, PtyExitPayload, PtyOpenResult } from "./types";

interface TerminalPaneProps {
  ptyId: string;
  cwd?: string;
  visible: boolean;
  onStatusChange: (status: "running" | "exited", exitCode?: number) => void;
}

export function TerminalPane({ ptyId, cwd, visible, onStatusChange }: TerminalPaneProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const generationRef = useRef(0);
  const connectedRef = useRef(false);
  const resizeTimerRef = useRef<number>(0);
  const resizeObserverRef = useRef<ResizeObserver | null>(null);
  const windowResizeRef = useRef<(() => void) | null>(null);
  const termDisposablesRef = useRef<IDisposable[]>([]);
  const unlistenOutputRef = useRef<UnlistenFn | null>(null);
  const unlistenExitRef = useRef<UnlistenFn | null>(null);

  const syncPtySize = useCallback(() => {
    const fitAddon = SessionRegistry.getFitAddon(ptyId);
    if (!fitAddon) return;
    const dims = fitAddon.proposeDimensions();
    if (dims && dims.cols > 0 && dims.rows > 0) {
      fitAddon.fit();
      invoke("pty_resize_new", { ptyId, cols: dims.cols, rows: dims.rows }).catch(() => {});
    }
  }, [ptyId]);

  // Main lifecycle: create terminal, connect PTY, listen for events.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let cancelled = false;

    // Generation counter defeats React StrictMode double-mount: if cleanup
    // runs between two mounts, the first mount's async init sees a stale
    // generation and bails out, preventing duplicate listeners/handlers.
    const gen = ++generationRef.current;
    const isStale = () => cancelled || gen !== generationRef.current;

    const init = async () => {
      if ("fonts" in document) {
        await document.fonts.ready;
      }
      if (isStale()) return;

      const terminal = SessionRegistry.attach(ptyId, container);

      // Let xterm measure against the actual DOM size before spawning the PTY.
      await new Promise<void>((resolve) => {
        requestAnimationFrame(() => {
          if (!isStale()) {
            SessionRegistry.getFitAddon(ptyId)?.fit();
          }
          resolve();
        });
      });
      if (isStale()) return;

      const unlistenOutput = await listen<PtyOutputPayload>(
        `pty:output:${ptyId}`,
        (event) => {
          terminal.write(event.payload.data);
        },
      );
      if (isStale()) { unlistenOutput(); return; }
      unlistenOutputRef.current = unlistenOutput;

      const unlistenExit = await listen<PtyExitPayload>(
        `pty:exit:${ptyId}`,
        (event) => {
          onStatusChange("exited", event.payload.code ?? undefined);
        },
      );
      if (isStale()) { unlistenOutput(); unlistenExit(); return; }
      unlistenExitRef.current = unlistenExit;

      if (!connectedRef.current) {
        const fitAddon = SessionRegistry.getFitAddon(ptyId);
        const dims = fitAddon?.proposeDimensions();
        const cols = terminal.cols > 0 ? terminal.cols : (dims?.cols ?? 120);
        const rows = terminal.rows > 0 ? terminal.rows : (dims?.rows ?? 32);
        try {
          await invoke<PtyOpenResult>("pty_open", {
            ptyId,
            cwd: cwd ?? null,
            cols,
            rows,
          });
          if (isStale()) return;
          connectedRef.current = true;
          onStatusChange("running");
        } catch (err) {
          terminal.writeln(`\r\n\x1b[31mFailed to open terminal: ${err}\x1b[0m\r\n`);
          onStatusChange("exited", 1);
          return;
        }
      }

      if (isStale()) return;

      const dataDisposable = terminal.onData((data) => {
        if (!isStale()) {
          invoke("pty_write_new", { ptyId, data }).catch(() => {});
        }
      });

      const resizeDisposable = terminal.onResize(({ cols, rows }) => {
        if (!isStale()) {
          invoke("pty_resize_new", { ptyId, cols, rows }).catch(() => {});
        }
      });

      termDisposablesRef.current = [dataDisposable, resizeDisposable];

      const scheduleFit = () => {
        cancelAnimationFrame(resizeTimerRef.current);
        resizeTimerRef.current = requestAnimationFrame(() => {
          if (!isStale()) syncPtySize();
        });
      };

      const observer = new ResizeObserver(() => scheduleFit());
      observer.observe(container);
      resizeObserverRef.current = observer;
      windowResizeRef.current = scheduleFit;
      window.addEventListener("resize", scheduleFit);

      requestAnimationFrame(() => {
        if (!isStale()) syncPtySize();
      });
    };

    void init();

    return () => {
      cancelled = true;
      generationRef.current += 1;
      // generationRef is incremented on next mount, making isStale() true
      // for any in-flight async init from this mount.
      cancelAnimationFrame(resizeTimerRef.current);
      resizeObserverRef.current?.disconnect();
      if (windowResizeRef.current) {
        window.removeEventListener("resize", windowResizeRef.current);
        windowResizeRef.current = null;
      }
      for (const d of termDisposablesRef.current) d.dispose();
      termDisposablesRef.current = [];
      unlistenOutputRef.current?.();
      unlistenOutputRef.current = null;
      unlistenExitRef.current?.();
      unlistenExitRef.current = null;
      SessionRegistry.detach(ptyId);
    };
  }, [ptyId]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    if (visible) {
      SessionRegistry.reattach(ptyId, container);
      requestAnimationFrame(() => syncPtySize());
    } else {
      SessionRegistry.detach(ptyId);
    }
  }, [visible, ptyId, syncPtySize]);

  return (
    <div
      ref={containerRef}
      style={{
        flex: 1,
        minHeight: 0,
        padding: "6px 4px",
        boxSizing: "border-box",
        background: "#15171E",
        display: visible ? "block" : "none",
      }}
    />
  );
}
