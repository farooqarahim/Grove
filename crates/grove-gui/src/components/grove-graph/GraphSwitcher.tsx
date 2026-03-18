import { useState, useRef, useEffect } from "react";
import type { GraphRecord } from "@/types";
import { GraphStatusBadge } from "./GraphStatusBadge";

interface GraphSwitcherProps {
  graphs: GraphRecord[];
  activeGraphId: string | null;
  onSwitch: (graphId: string) => void;
  onNewGraph?: () => void;
}

function dotColor(g: GraphRecord): string {
  const s: string = g.runtime_status !== "idle" ? g.runtime_status : g.status;
  if (s === "running" || s === "queued" || s === "inprogress") return "#fb923c";
  if (s === "closed" || s === "done" || s === "passed" || s === "complete") return "#3ecf8e";
  if (s === "failed" || s === "aborted" || s === "error") return "#f87171";
  if (s === "paused") return "#f59e0b";
  return "#3a3c47";
}

function isRunning(g: GraphRecord): boolean {
  return g.runtime_status === "running" || g.runtime_status === "queued";
}

export function GraphSwitcher({ graphs, activeGraphId, onSwitch, onNewGraph }: GraphSwitcherProps) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const active = graphs.find(g => g.id === activeGraphId) ?? graphs[0];
  const hasMultiple = graphs.length > 1;
  const backgroundRunning = graphs.filter(g => g.id !== activeGraphId && isRunning(g)).length;

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  if (!active) return null;

  const activeBadgeStatus = active.runtime_status !== "idle" ? active.runtime_status : active.status;

  return (
    <div ref={containerRef} style={{ borderBottom: "1px solid #222329", flexShrink: 0, position: "relative" }}>

      {/* ── Header row ── */}
      <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "10px 14px" }}>

        {/* Left: title + status + chevron */}
        <button
          onClick={() => hasMultiple && setOpen(o => !o)}
          style={{
            display: "flex", alignItems: "center", gap: 8,
            flex: 1, minWidth: 0,
            background: "transparent", border: "none",
            cursor: hasMultiple ? "pointer" : "default",
            padding: 0, textAlign: "left",
          }}
        >
          <span style={{
            fontSize: 14, fontWeight: 600, color: "#e2e4e9",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
            flex: 1,
          }}>
            {active.title}
          </span>

          <GraphStatusBadge status={activeBadgeStatus} size="sm" />

          {hasMultiple && (
            <span style={{ display: "flex", alignItems: "center", gap: 5, flexShrink: 0 }}>
              {backgroundRunning > 0 && (
                <span style={{
                  width: 5, height: 5, borderRadius: "50%",
                  background: "#fb923c", display: "inline-block",
                }} />
              )}
              <span style={{ fontSize: 11, color: "#5c5e6a", fontVariantNumeric: "tabular-nums" }}>
                {graphs.length}
              </span>
              <svg
                width={10} height={10} viewBox="0 0 12 12"
                fill="none" stroke="#5c5e6a" strokeWidth={2}
                strokeLinecap="round" strokeLinejoin="round"
                style={{
                  transform: open ? "rotate(180deg)" : "rotate(0deg)",
                  transition: "transform 0.15s",
                }}
              >
                <polyline points="2,4 6,8 10,4" />
              </svg>
            </span>
          )}
        </button>

        {/* Right: + new */}
        {onNewGraph && (
          <button
            onClick={onNewGraph}
            title="New Graph"
            style={{
              display: "flex", alignItems: "center", justifyContent: "center",
              width: 24, height: 24, borderRadius: 4, flexShrink: 0,
              background: "rgba(255,255,255,0.04)", border: "1px solid #222329",
              color: "#5c5e6a", cursor: "pointer",
              transition: "background 0.15s, border-color 0.15s",
            }}
            onMouseEnter={e => {
              e.currentTarget.style.background = "rgba(255,255,255,0.07)";
              e.currentTarget.style.borderColor = "#2a2c33";
            }}
            onMouseLeave={e => {
              e.currentTarget.style.background = "rgba(255,255,255,0.04)";
              e.currentTarget.style.borderColor = "#222329";
            }}
          >
            <svg width={10} height={10} viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round">
              <line x1={6} y1={1} x2={6} y2={11} />
              <line x1={1} y1={6} x2={11} y2={6} />
            </svg>
          </button>
        )}
      </div>

      {/* ── Dropdown list ── */}
      {open && hasMultiple && (
        <div style={{
          position: "absolute", top: "100%", left: 0, right: 0, zIndex: 50,
          background: "#16181d",
          border: "1px solid #222329", borderTop: "none",
          maxHeight: 300, overflowY: "auto",
        }}>
          {graphs.map((g, i) => {
            const isActive = g.id === activeGraphId;
            const badgeStatus = g.runtime_status !== "idle" ? g.runtime_status : g.status;
            const dc = dotColor(g);
            const isLast = i === graphs.length - 1;
            return (
              <button
                key={g.id}
                onClick={() => { onSwitch(g.id); setOpen(false); }}
                style={{
                  display: "flex", alignItems: "center", gap: 10,
                  width: "100%", padding: "8px 14px",
                  background: isActive ? "rgba(255,255,255,0.04)" : "transparent",
                  border: "none",
                  borderBottom: isLast ? "none" : "1px solid #1a1c21",
                  cursor: "pointer", textAlign: "left",
                  transition: "background 0.1s",
                }}
                onMouseEnter={e => {
                  if (!isActive) e.currentTarget.style.background = "rgba(255,255,255,0.025)";
                }}
                onMouseLeave={e => {
                  if (!isActive) e.currentTarget.style.background = "transparent";
                }}
              >
                <span style={{
                  width: 6, height: 6, borderRadius: "50%",
                  background: dc, flexShrink: 0,
                  ...(isRunning(g) ? { animation: "pulse 2s infinite" } : {}),
                }} />
                <span style={{
                  fontSize: 13, flex: 1,
                  overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                  color: isActive ? "#e2e4e9" : "#8b8d98",
                  fontWeight: isActive ? 500 : 400,
                }}>
                  {g.title}
                </span>
                <GraphStatusBadge status={badgeStatus} size="sm" />
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
