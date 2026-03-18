import { useCallback, useEffect, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { Pulse, XIcon } from "@/components/ui/icons";
import { qk } from "@/lib/queryKeys";
import { runEvents } from "@/lib/api";
import { C } from "@/lib/theme";
import { RealTerminal } from "./RealTerminal";

type Tab = "stream" | "terminal";

interface TerminalPaneProps {
  runId: string | null;
  runLabel: string;
  /** Conversation this pane belongs to — used to key the persistent PTY session. */
  conversationId: string;
  /** Controlled: which tab is active — owned by parent. */
  activeTab: Tab;
  onTabChange: (tab: Tab) => void;
  onClose: () => void;
  /** Working directory to open the real terminal in. */
  cwd?: string;
}

const eventTypeColor: Record<string, string> = {
  sys: "#52575F",
  system: "#52575F",
  read: "#67E8F9",
  edit: "#31B97B",
  think: "#F59E0B",
  bash: "#A78BFA",
  error: "#EF4444",
  warn: "#F59E0B",
};

function eventColor(eventType: string): string {
  const lower = eventType.toLowerCase();
  for (const [key, color] of Object.entries(eventTypeColor)) {
    if (lower.includes(key)) return color;
  }
  return C.text4;
}

const TAB_STYLE_BASE: React.CSSProperties = {
  background: "none",
  border: "none",
  fontSize: 10,
  fontWeight: 600,
  letterSpacing: "0.04em",
  cursor: "pointer",
  padding: "4px 10px",
  borderRadius: 4,
  transition: "color 0.12s, background 0.12s",
};

export function TerminalPane({ runId, runLabel, conversationId, activeTab, onTabChange, onClose, cwd }: TerminalPaneProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const isNearBottomRef = useRef(true);
  const prevCountRef = useRef(0);

  const { data: events } = useQuery({
    queryKey: qk.events(runId),
    queryFn: () => runId ? runEvents(runId) : Promise.resolve([]),
    refetchInterval: 2000,
    staleTime: 1000,
  });

  const handleScroll = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    isNearBottomRef.current =
      el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  }, []);

  useEffect(() => {
    const count = events?.length ?? 0;
    const hasNewEvents = count > prevCountRef.current;
    prevCountRef.current = count;
    if (scrollRef.current && isNearBottomRef.current && hasNewEvents) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [events]);

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: 280,
        background: C.base,
        borderTop: `1px solid ${C.border}`,
      }}
    >
      {/* Header: tabs + run label + close */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "0 12px",
          height: 32,
          flexShrink: 0,
          background: C.surfaceHover,
          borderBottom: `1px solid ${C.border}`,
        }}
      >
        {/* Tab bar */}
        <div style={{ display: "flex", alignItems: "center", gap: 2 }}>
          <button
            onClick={() => onTabChange("stream")}
            style={{
              ...TAB_STYLE_BASE,
              color: activeTab === "stream" ? C.accent : C.text4,
              background: activeTab === "stream" ? C.accentMuted : "transparent",
            }}
          >
            Stream
          </button>
          <button
            onClick={() => onTabChange("terminal")}
            style={{
              ...TAB_STYLE_BASE,
              color: activeTab === "terminal" ? C.accent : C.text4,
              background: activeTab === "terminal" ? C.accentMuted : "transparent",
            }}
          >
            Terminal
          </button>
        </div>

        {/* Run label + close */}
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          {runLabel && (
            <div style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 10 }}>
              <Pulse color={C.accent} size={5} />
              <span style={{ color: C.text4, fontFamily: C.mono }}>{runLabel}</span>
            </div>
          )}
          <button
            onClick={onClose}
            style={{
              background: "none",
              border: "none",
              color: C.text4,
              cursor: "pointer",
              padding: 3,
              borderRadius: 3,
              lineHeight: 1,
            }}
          >
            <XIcon size={10} />
          </button>
        </div>
      </div>

      {/* Stream tab */}
      {activeTab === "stream" && (
        <div
          ref={scrollRef}
          onScroll={handleScroll}
          style={{
            flex: 1,
            overflowY: "auto",
            padding: "8px 16px",
            fontFamily: C.mono,
            fontSize: 11,
            lineHeight: 1.75,
          }}
        >
          {(!events || events.length === 0) && (
            <div style={{ color: C.text4 }}>No events yet</div>
          )}
          {events?.map((ev) => {
            const evType = ev.event_type.split("_").pop() ?? ev.event_type;
            return (
              <div key={ev.id} style={{ display: "flex", gap: 8 }}>
                <span
                  style={{
                    width: 36,
                    textAlign: "right",
                    flexShrink: 0,
                    fontWeight: 500,
                    color: eventColor(ev.event_type),
                  }}
                >
                  {evType}
                </span>
                <span style={{ color: "#A1A6AE" }}>
                  {typeof ev.payload === "string"
                    ? ev.payload
                    : JSON.stringify(ev.payload)}
                </span>
              </div>
            );
          })}
          <div style={{ color: C.text4, paddingLeft: 44, opacity: 0.4 }}>
            {"\u2588"}
          </div>
        </div>
      )}

      {/* Real terminal tab — kept mounted so the PTY session persists */}
      {activeTab === "terminal" && (
        <RealTerminal conversationId={conversationId} cwd={cwd} />
      )}
    </div>
  );
}
