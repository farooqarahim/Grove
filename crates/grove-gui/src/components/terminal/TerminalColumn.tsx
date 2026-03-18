import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

import { TerminalTabBar } from "./TerminalTabBar";
import { TerminalPane } from "./TerminalPane";
import { TerminalStatusBar } from "./TerminalStatusBar";
import { SessionRegistry } from "./SessionRegistry";
import type { TerminalTab } from "./types";

interface TerminalColumnProps {
  conversationId: string;
  cwd?: string;
  visible?: boolean;
}

export function TerminalColumn({ conversationId, cwd, visible = true }: TerminalColumnProps) {
  const [tabs, setTabs] = useState<TerminalTab[]>(() => [
    {
      id: `${conversationId}:0`,
      tabIndex: 0,
      label: "Agent",
      kind: "agent",
      config: { cwd: cwd ?? "" },
      status: "starting",
    },
  ]);

  const [activeTabIndex, setActiveTabIndex] = useState(0);
  const [shellCounter, setShellCounter] = useState(1);

  const handleStatusChange = useCallback(
    (tabIndex: number) => (status: "running" | "exited", exitCode?: number) => {
      setTabs((prev) =>
        prev.map((tab) =>
          tab.tabIndex === tabIndex
            ? { ...tab, status, exitCode }
            : tab,
        ),
      );
    },
    [],
  );

  const handleNewTab = useCallback(() => {
    const newTabIndex = shellCounter;
    const newTab: TerminalTab = {
      id: `${conversationId}:${newTabIndex}`,
      tabIndex: newTabIndex,
      label: `Shell ${newTabIndex}`,
      kind: "shell",
      config: { cwd: cwd ?? "" },
      status: "starting",
    };
    setTabs((prev) => {
      const next = [...prev, newTab];
      setActiveTabIndex(next.length - 1);
      return next;
    });
    setShellCounter((c) => c + 1);
  }, [conversationId, cwd, shellCounter]);

  const handleCloseTab = useCallback(
    (displayIndex: number) => {
      const tab = tabs[displayIndex];
      if (!tab || tab.kind === "agent") return;

      invoke("pty_close_new", { ptyId: tab.id }).catch(() => {});
      SessionRegistry.dispose(tab.id);

      setTabs((prev) => prev.filter((_, i) => i !== displayIndex));
      setActiveTabIndex((prev) => {
        if (displayIndex < prev) return prev - 1;
        if (displayIndex === prev) return Math.max(0, prev - 1);
        return prev;
      });
    },
    [tabs],
  );

  const handleRestart = useCallback(() => {
    const agentTab = tabs.find((t) => t.kind === "agent");
    if (!agentTab) return;

    invoke("pty_close_new", { ptyId: agentTab.id }).catch(() => {});
    SessionRegistry.dispose(agentTab.id);

    setTabs((prev) =>
      prev.map((tab) =>
        tab.kind === "agent"
          ? { ...tab, status: "starting" as const, exitCode: undefined }
          : tab,
      ),
    );
  }, [tabs]);

  const activeTab = tabs[activeTabIndex];
  const agentTab = tabs.find((t) => t.kind === "agent");

  return (
    <div
      className="flex-1 flex flex-col min-h-0"
      style={{
        background: "#15171E",
        display: visible ? "flex" : "none",
      }}
    >
      <TerminalTabBar
        tabs={tabs}
        activeTabIndex={activeTabIndex}
        onSelectTab={setActiveTabIndex}
        onCloseTab={handleCloseTab}
        onNewTab={handleNewTab}
      />

      <div className="flex-1 flex flex-col min-h-0 relative">
        {tabs.map((tab, index) => (
          <TerminalPane
            key={tab.id}
            ptyId={tab.id}
            cwd={tab.config.cwd}
            visible={visible && index === activeTabIndex}
            onStatusChange={handleStatusChange(tab.tabIndex)}
          />
        ))}
      </div>

      {agentTab && activeTab?.kind === "agent" && (
        <TerminalStatusBar
          status={agentTab.status}
          exitCode={agentTab.exitCode}
          onRestart={handleRestart}
        />
      )}
    </div>
  );
}
