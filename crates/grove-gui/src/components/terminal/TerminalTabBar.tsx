import { Plus, X } from "lucide-react";
import { C } from "@/lib/theme";
import type { TerminalTab } from "./types";

interface TerminalTabBarProps {
  tabs: TerminalTab[];
  activeTabIndex: number;
  onSelectTab: (index: number) => void;
  onCloseTab: (index: number) => void;
  onNewTab: () => void;
}

export function TerminalTabBar({
  tabs,
  activeTabIndex,
  onSelectTab,
  onCloseTab,
  onNewTab,
}: TerminalTabBarProps) {
  return (
    <div
      className="flex items-center shrink-0"
      style={{
        height: 34,
        borderBottom: `1px solid ${C.border}`,
        background: C.surface,
        paddingLeft: 8,
        paddingRight: 4,
        gap: 1,
      }}
    >
      {tabs.map((tab, index) => {
        const isActive = index === activeTabIndex;
        const isAgent = tab.kind === "agent";
        const statusDot =
          tab.status === "running"
            ? C.accent
            : tab.status === "exited" && (tab.exitCode ?? 0) !== 0
              ? C.danger
              : C.surfaceRaised;

        return (
          <button
            key={tab.id}
            onClick={() => onSelectTab(index)}
            className="flex items-center gap-1.5 cursor-pointer"
            style={{
              height: 28,
              padding: "0 10px",
              fontSize: 11,
              fontWeight: isActive ? 600 : 400,
              color: isActive ? C.text1 : C.text4,
              background: isActive ? C.surfaceHover : "transparent",
              border: "none",
              borderRadius: 4,
              transition: "background 80ms",
            }}
          >
            <span
              style={{
                width: 6,
                height: 6,
                borderRadius: "50%",
                background: statusDot,
                flexShrink: 0,
              }}
            />
            <span style={{ fontFamily: C.mono, whiteSpace: "nowrap" }}>{tab.label}</span>
            {!isAgent && (
              <span
                onClick={(e) => {
                  e.stopPropagation();
                  onCloseTab(index);
                }}
                className="flex items-center justify-center cursor-pointer"
                style={{
                  width: 16,
                  height: 16,
                  borderRadius: 3,
                  marginLeft: 2,
                  opacity: 0.5,
                }}
              >
                <X size={10} />
              </span>
            )}
          </button>
        );
      })}

      <button
        onClick={onNewTab}
        className="flex items-center justify-center cursor-pointer"
        title="New shell tab"
        style={{
          width: 24,
          height: 24,
          borderRadius: 4,
          background: "transparent",
          border: "none",
          color: C.text4,
          marginLeft: 2,
        }}
      >
        <Plus size={12} />
      </button>
    </div>
  );
}
