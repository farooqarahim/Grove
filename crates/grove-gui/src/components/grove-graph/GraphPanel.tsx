import React, { useCallback, useState, useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { qk } from "@/lib/queryKeys";
import {
  listGraphs,
  getActiveGraph,
  getGraphDetail,
  setActiveGraph,
  pauseGraph,
  resumeGraph,
  abortGraph,
  restartGraph,
  setGraphExecutionMode,
  reportGraphBug,
} from "@/lib/api";
import type { GraphDetail, GraphExecutionMode, GraphRecord, GraphStepRecord } from "@/types";
import { GraphControlBar } from "./GraphControlBar";
import { GraphStatusBadge } from "./GraphStatusBadge";
import { GraphTree } from "./GraphTree";
import { StepDetailDrawer } from "./StepDetailDrawer";
import { PhaseValidationDrawer } from "./PhaseValidationDrawer";
import { CreateGraphModal } from "@/components/modals/CreateGraphModal";
import { ClarificationModal } from "./ClarificationModal";
import { DocumentEditorPanel } from "./DocumentEditorPanel";

const MONO = "'JetBrains Mono', 'Fira Code', 'SF Mono', monospace";

/* ── Graph list (master view) ── */

function fmtRelative(dateStr: string): string {
  const diff = Math.floor((Date.now() - new Date(dateStr).getTime()) / 1000);
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

function listDotColor(g: GraphRecord): string {
  const s: string = g.runtime_status !== "idle" ? g.runtime_status : g.status;
  if (s === "running" || s === "queued" || s === "inprogress") return "#fb923c";
  if (s === "closed" || s === "done" || s === "passed" || s === "complete") return "#3ecf8e";
  if (s === "failed" || s === "aborted" || s === "error") return "#f87171";
  if (s === "paused") return "#f59e0b";
  return "#3a3c47";
}

const IDENTITY_COLORS = [
  "#3b82f6", "#8b5cf6", "#ec4899", "#f59e0b",
  "#10b981", "#ef4444", "#06b6d4", "#f97316",
  "#84cc16", "#a855f7", "#14b8a6", "#e879f9",
];

function identityColor(id: string): string {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) | 0;
  return IDENTITY_COLORS[Math.abs(h) % IDENTITY_COLORS.length];
}

function GraphListRow({
  graph,
  isActive,
  onClick,
}: {
  graph: GraphRecord;
  isActive: boolean;
  onClick: () => void;
}) {
  const badgeStatus: string = graph.runtime_status !== "idle" ? graph.runtime_status : graph.status;
  const running = graph.runtime_status === "running" || graph.runtime_status === "queued";
  const failed = graph.status === "failed" || graph.runtime_status === "aborted";
  const dc = listDotColor(graph);
  const ic = identityColor(graph.id);
  const total = graph.steps_created_count;
  const closed = graph.steps_closed_count;
  const hasSteps = total > 0;
  const hasPhases = graph.phases_created_count > 0;
  const initial = graph.title.trim().charAt(0).toUpperCase() || "G";

  const stepColor = !hasSteps ? "#3a3c47"
    : closed === total ? "#3ecf8e"
    : closed > 0 ? "#fb923c"
    : "#4a4d5a";

  const rowBg = running
    ? "rgba(251,146,60,0.03)"
    : failed
      ? "rgba(248,113,113,0.03)"
      : isActive
        ? "rgba(255,255,255,0.03)"
        : "transparent";

  return (
    <button
      onClick={onClick}
      style={{
        display: "flex", alignItems: "center", gap: 12,
        width: "100%", padding: "11px 14px",
        background: rowBg,
        border: "none", cursor: "pointer", textAlign: "left",
        transition: "background 0.12s",
      }}
      onMouseEnter={e => { e.currentTarget.style.background = "rgba(255,255,255,0.05)"; }}
      onMouseLeave={e => { e.currentTarget.style.background = rowBg; }}
    >
      {/* Avatar */}
      <div style={{ position: "relative", flexShrink: 0 }}>
        <span style={{
          width: 34, height: 34, borderRadius: 9,
          background: `${ic}18`,
          border: `1px solid ${ic}35`,
          display: "inline-flex", alignItems: "center", justifyContent: "center",
          fontSize: 14, fontWeight: 700, color: ic,
        }}>
          {initial}
        </span>
        <span style={{
          position: "absolute", bottom: -1, right: -1,
          width: 9, height: 9, borderRadius: "50%",
          background: dc, border: "2px solid #0d0e11",
          ...(running ? { animation: "graph-dot-pulse 1.8s ease-in-out infinite" } : {}),
        }} />
      </div>

      {/* Text block */}
      <div style={{ flex: 1, minWidth: 0 }}>
        {/* Title */}
        <div style={{ display: "flex", alignItems: "center", gap: 7, marginBottom: 4 }}>
          <span style={{
            fontSize: 13, fontWeight: 600,
            color: isActive ? "#f0f2f5" : "#c9cdd8",
            flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
          }}>
            {graph.title}
          </span>
          <GraphStatusBadge status={badgeStatus} size="sm" />
        </div>

        {/* Meta row */}
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          {hasPhases && (
            <span style={{ fontSize: 10, color: "#4a4d5a" }}>
              {graph.phases_created_count} {graph.phases_created_count === 1 ? "phase" : "phases"}
            </span>
          )}
          {hasPhases && hasSteps && (
            <span style={{ fontSize: 10, color: "#2a2c30" }}>·</span>
          )}
          {hasSteps && (
            <span style={{ fontSize: 10, color: stepColor, fontVariantNumeric: "tabular-nums", fontFamily: MONO }}>
              {closed}/{total}
            </span>
          )}
          <span style={{ flex: 1 }} />
          <span style={{ fontSize: 10, color: "#3a3c47" }}>
            {fmtRelative(graph.updated_at)}
          </span>
        </div>
      </div>

      {/* Chevron */}
      <svg width={9} height={9} viewBox="0 0 12 12" fill="none" stroke="#2e3038" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" style={{ flexShrink: 0 }}>
        <polyline points="4,2 8,6 4,10" />
      </svg>
    </button>
  );
}

function GraphList({
  graphs,
  activeGraphId,
  onSelect,
  onNew,
}: {
  graphs: GraphRecord[];
  activeGraphId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
}) {
  const running = graphs.filter(g => g.runtime_status === "running" || g.runtime_status === "queued");
  const rest = graphs.filter(g => g.runtime_status !== "running" && g.runtime_status !== "queued");
  const sorted = [...running, ...rest];

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", background: "#0d0e11" }}>

      {/* Header */}
      <div style={{
        display: "flex", alignItems: "center", gap: 10,
        padding: "13px 16px 11px",
        borderBottom: "1px solid #222329", flexShrink: 0,
      }}>
        <span style={{ fontSize: 12, fontWeight: 700, color: "#5c5e6a", letterSpacing: "0.08em", textTransform: "uppercase", flex: 1 }}>
          Graphs
        </span>
        {running.length > 0 && (
          <span style={{
            fontSize: 10, fontWeight: 600, color: "#fb923c",
            background: "rgba(251,146,60,0.1)", padding: "2px 7px", borderRadius: 3,
          }}>
            {running.length} running
          </span>
        )}
        <span style={{ fontSize: 11, color: "#3a3c47", fontVariantNumeric: "tabular-nums" }}>
          {graphs.length}
        </span>
        <button
          onClick={onNew}
          style={{
            display: "flex", alignItems: "center", gap: 5,
            padding: "4px 9px", borderRadius: 4,
            background: "rgba(62,207,142,0.1)", border: "1px solid rgba(62,207,142,0.2)",
            color: "#3ecf8e", cursor: "pointer", fontSize: 11, fontWeight: 600,
            transition: "background 0.15s",
          }}
          onMouseEnter={e => { e.currentTarget.style.background = "rgba(62,207,142,0.16)"; }}
          onMouseLeave={e => { e.currentTarget.style.background = "rgba(62,207,142,0.1)"; }}
        >
          <svg width={9} height={9} viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round">
            <line x1={6} y1={1} x2={6} y2={11} />
            <line x1={1} y1={6} x2={11} y2={6} />
          </svg>
          New
        </button>
      </div>

      {/* List */}
      <div style={{ flex: 1, overflowY: "auto", padding: "8px 0" }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          {sorted.map(g => (
            <GraphListRow
              key={g.id}
              graph={g}
              isActive={g.id === activeGraphId}
              onClick={() => onSelect(g.id)}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

/* ── Keyframe animations (injected once into the document) ── */
const GRAPH_KEYFRAMES = `
@keyframes graph-dot-pulse {
  0%, 100% { transform: scale(1); opacity: 1; }
  50% { transform: scale(1.6); opacity: 0.5; }
}
@keyframes graph-activity-spin {
  to { transform: rotate(360deg); }
}
@keyframes graph-shimmer {
  0% { background-position: -200% 0; }
  100% { background-position: 200% 0; }
}
@keyframes graph-slide-progress {
  0% { background-position: 0% 0; }
  100% { background-position: 200% 0; }
}
.graph-active-dot { animation: graph-dot-pulse 1.8s ease-in-out infinite; }
`;

/* ── Activity ticker ── */
interface Activity {
  src?: string;
  msg: string;
  color?: string;
}

function ActivityTicker({ activities, visible }: { activities: Activity[]; visible: boolean }) {
  const [idx, setIdx] = useState(0);
  const [fade, setFade] = useState(true);

  useEffect(() => {
    if (!visible || activities.length === 0) return;
    const iv = setInterval(() => {
      setFade(false);
      setTimeout(() => {
        setIdx((p) => (p + 1) % activities.length);
        setFade(true);
      }, 200);
    }, 3200);
    return () => clearInterval(iv);
  }, [visible, activities.length]);

  if (!visible || activities.length === 0) return null;
  const a = activities[idx];
  const c = a.color ?? "#fb923c";

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "0 16px 8px",
        opacity: fade ? 1 : 0,
        transition: "opacity 0.2s",
      }}
    >
      <svg
        width={12}
        height={12}
        viewBox="0 0 12 12"
        fill="none"
        stroke={c}
        strokeWidth={2}
        strokeLinecap="round"
        style={{ flexShrink: 0, animation: "graph-activity-spin 1.2s linear infinite" }}
      >
        <path d="M6 1v2M6 9v2M1 6h2M9 6h2M2.64 2.64l1.42 1.42M7.94 7.94l1.42 1.42M2.64 9.36l1.42-1.42M7.94 4.06l1.42-1.42" />
      </svg>
      {a.src && (
        <span
          style={{
            fontFamily: MONO,
            fontSize: 10,
            fontWeight: 600,
            color: c,
            letterSpacing: "0.03em",
            opacity: 0.8,
          }}
        >
          {a.src}
        </span>
      )}
      <span style={{ fontSize: 12, color: "#8b8d98" }}>{a.msg}</span>
    </div>
  );
}

/* ── Overall progress bar ── */
function OverallProgressBar({
  closed,
  total,
  isRunning,
  isParsing,
}: {
  closed: number;
  total: number;
  isRunning: boolean;
  isParsing: boolean;
}) {
  const pct = total > 0 ? (closed / total) * 100 : 0;

  if (isParsing) {
    return (
      <div style={{ padding: "10px 16px 0" }}>
        <div
          style={{
            height: 4,
            borderRadius: 2,
            overflow: "hidden",
            background: `linear-gradient(90deg, rgba(251,146,60,0.06), rgba(251,146,60,0.25), rgba(251,146,60,0.06))`,
            backgroundSize: "200% 100%",
            animation: "graph-slide-progress 2s ease-in-out infinite",
          }}
        />
      </div>
    );
  }

  if (total === 0) return null;

  return (
    <div style={{ padding: "10px 16px 0" }}>
      <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
        <div
          style={{
            flex: 1,
            height: 4,
            borderRadius: 2,
            background: "rgba(255,255,255,0.05)",
            overflow: "hidden",
            position: "relative",
          }}
        >
          <div
            style={{
              width: `${pct}%`,
              height: "100%",
              borderRadius: 2,
              background: "#3ecf8e",
              transition: "width 1s cubic-bezier(0.16,1,0.3,1)",
              position: "relative",
              zIndex: 2,
            }}
          />
          {isRunning && pct < 100 && (
            <div
              style={{
                position: "absolute",
                top: 0,
                left: `${pct}%`,
                width: "20%",
                height: "100%",
                background: "linear-gradient(90deg, rgba(251,146,60,0.6), rgba(251,146,60,0))",
                borderRadius: 2,
                zIndex: 1,
                animation: "graph-dot-pulse 2s ease-in-out infinite",
              }}
            />
          )}
        </div>
        <span style={{ fontFamily: MONO, fontSize: 11, color: "#8b8d98", flexShrink: 0 }}>
          {closed}/{total}
        </span>
      </div>
    </div>
  );
}

/* ── Empty / parsing states ── */
function EmptyState({ status, parsingStatus }: { status: "parsing" | "waiting" | "empty"; parsingStatus?: string }) {
  if (status === "parsing") {
    const label =
      parsingStatus === "planning" ? "Planning..." :
      parsingStatus === "generating" ? "Generating..." :
      "Parsing...";
    const subtitle =
      parsingStatus === "planning" ? "Generating foundational documents..." :
      parsingStatus === "generating" ? "Creating document from specification..." :
      "Extracting phases, steps, and dependencies...";

    return (
      <div
        style={{
          flex: 1,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          padding: "48px 24px",
        }}
      >
        <div style={{ textAlign: "center", maxWidth: 340 }}>
          <div
            style={{
              width: 52,
              height: 52,
              borderRadius: 13,
              margin: "0 auto 18px",
              background: "rgba(251,146,60,0.08)",
              border: "1px solid rgba(251,146,60,0.2)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <svg
              width={22}
              height={22}
              viewBox="0 0 22 22"
              fill="none"
              stroke="#fb923c"
              strokeWidth={1.8}
              strokeLinecap="round"
              style={{ animation: "graph-activity-spin 1.5s linear infinite" }}
            >
              <path d="M11 2v3M11 17v3M2 11h3M17 11h3M4.22 4.22l2.12 2.12M15.66 15.66l2.12 2.12M4.22 17.78l2.12-2.12M15.66 6.34l2.12-2.12" />
            </svg>
          </div>
          <p style={{ fontSize: 14, fontWeight: 500, color: "#e2e4e9", marginBottom: 6 }}>
            {label}
          </p>
          <p style={{ fontSize: 12.5, color: "#5c5e6a", lineHeight: 1.6, marginBottom: 20 }}>
            {subtitle}
          </p>
          <div style={{ display: "flex", justifyContent: "center", gap: 6, flexWrap: "wrap" }}>
            {[48, 72, 56, 64, 44].map((w, i) => (
              <div
                key={i}
                style={{
                  width: w,
                  height: 7,
                  borderRadius: 4,
                  background:
                    "linear-gradient(90deg, rgba(255,255,255,0.03) 25%, rgba(255,255,255,0.08) 50%, rgba(255,255,255,0.03) 75%)",
                  backgroundSize: "200% 100%",
                  animation: `graph-shimmer 2s ease-in-out infinite ${i * 0.15}s`,
                }}
              />
            ))}
          </div>
        </div>
      </div>
    );
  }

  if (status === "waiting") {
    return (
      <div
        style={{
          flex: 1,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          padding: "48px 24px",
        }}
      >
        <div style={{ textAlign: "center", maxWidth: 300 }}>
          <div
            style={{
              width: 52,
              height: 52,
              borderRadius: 13,
              margin: "0 auto 18px",
              background: "rgba(96,165,250,0.08)",
              border: "1px solid rgba(96,165,250,0.2)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <svg
              width={22}
              height={22}
              viewBox="0 0 24 24"
              fill="none"
              stroke="#60a5fa"
              strokeWidth={1.8}
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M12 2L2 7l10 5 10-5-10-5z" />
              <path d="M2 17l10 5 10-5M2 12l10 5 10-5" />
            </svg>
          </div>
          <p style={{ fontSize: 14, fontWeight: 500, color: "#e2e4e9", marginBottom: 6 }}>
            Ready to start
          </p>
          <p style={{ fontSize: 12.5, color: "#5c5e6a", lineHeight: 1.6 }}>
            Upload a PRD or describe your project to generate phases and steps.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "48px 24px",
      }}
    >
      <div style={{ textAlign: "center", maxWidth: 300 }}>
        <div
          style={{
            width: 52,
            height: 52,
            borderRadius: 13,
            margin: "0 auto 18px",
            background: "rgba(255,255,255,0.03)",
            border: "1px dashed #2a2c33",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <svg
            width={22}
            height={22}
            viewBox="0 0 24 24"
            fill="none"
            stroke="#5c5e6a"
            strokeWidth={1.5}
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <rect x={3} y={3} width={18} height={18} rx={2} />
            <path d="M3 9h18M9 21V9" />
          </svg>
        </div>
        <p style={{ fontSize: 14, fontWeight: 500, color: "#e2e4e9", marginBottom: 6 }}>
          No phases yet
        </p>
        <p style={{ fontSize: 12.5, color: "#5c5e6a", lineHeight: 1.6 }}>
          This graph doesn't have any phases or steps defined.
        </p>
      </div>
    </div>
  );
}

/* ── Tab bar ── */
function TabBar({
  activeTab,
  onChange,
  phasesCount,
}: {
  activeTab: "phases" | "logs";
  onChange: (tab: "phases" | "logs") => void;
  phasesCount: number;
}) {
  return (
    <div
      style={{
        display: "flex",
        borderBottom: "1px solid #222329",
        flexShrink: 0,
      }}
    >
      {(["phases", "logs"] as const).map((tab) => {
        const isActive = activeTab === tab;
        const count = tab === "phases" ? phasesCount : 0;
        return (
          <button
            key={tab}
            onClick={() => onChange(tab)}
            style={{
              background: "transparent",
              border: "none",
              cursor: "pointer",
              padding: "9px 16px",
              fontSize: 13,
              fontWeight: isActive ? 500 : 400,
              color: isActive ? "#e2e4e9" : "#5c5e6a",
              display: "flex",
              alignItems: "center",
              gap: 6,
              transition: "color 0.15s",
              position: "relative",
              outline: "none",
            }}
          >
            {tab.charAt(0).toUpperCase() + tab.slice(1)}
            {count > 0 && (
              <span
                style={{
                  fontSize: 10,
                  fontWeight: 600,
                  fontFamily: MONO,
                  padding: "1px 6px",
                  borderRadius: 99,
                  background: isActive ? "rgba(62,207,142,0.08)" : "rgba(255,255,255,0.04)",
                  color: isActive ? "#3ecf8e" : "#5c5e6a",
                  border: `1px solid ${isActive ? "rgba(62,207,142,0.2)" : "transparent"}`,
                }}
              >
                {count}
              </span>
            )}
            {isActive && (
              <div
                style={{
                  position: "absolute",
                  bottom: -1,
                  left: 0,
                  right: 0,
                  height: 2,
                  borderRadius: "2px 2px 0 0",
                  background: "#3ecf8e",
                }}
              />
            )}
          </button>
        );
      })}
    </div>
  );
}

/* ── Logs placeholder ── */
function LogsPlaceholder() {
  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "48px 24px",
      }}
    >
      <div style={{ textAlign: "center", maxWidth: 300 }}>
        <div
          style={{
            width: 52,
            height: 52,
            borderRadius: 13,
            margin: "0 auto 18px",
            background: "rgba(255,255,255,0.03)",
            border: "1px dashed #2a2c33",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <svg
            width={22}
            height={22}
            viewBox="0 0 24 24"
            fill="none"
            stroke="#5c5e6a"
            strokeWidth={1.5}
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z" />
            <polyline points="14 2 14 8 20 8" />
            <line x1={16} y1={13} x2={8} y2={13} />
            <line x1={16} y1={17} x2={8} y2={17} />
            <polyline points="10 9 9 9 8 9" />
          </svg>
        </div>
        <p style={{ fontSize: 14, fontWeight: 500, color: "#e2e4e9", marginBottom: 6 }}>
          Logs coming soon
        </p>
        <p style={{ fontSize: 12.5, color: "#5c5e6a", lineHeight: 1.6 }}>
          Orchestration logs will appear here during execution.
        </p>
      </div>
    </div>
  );
}

/* ── Footer ── */
function GraphFooter({
  graph,
  onStart,
  onResume,
  onAbort,
  onRestart,
}: {
  graph: GraphRecord;
  onStart: () => void;
  onResume: () => void;
  onAbort: () => void;
  onRestart: (full: boolean) => void;
}) {
  const { runtime_status, parsing_status, rerun_count, max_reruns } = graph;

  let cta: React.ReactNode = null;

  if (runtime_status === "idle" && parsing_status === "complete") {
    cta = (
      <button
        onClick={onStart}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 5,
          fontSize: 12.5,
          fontWeight: 600,
          color: "#fff",
          background: "rgba(62,207,142,0.85)",
          border: "1px solid #3ecf8e",
          borderRadius: 7,
          padding: "7px 16px",
          cursor: "pointer",
          transition: "opacity 0.15s",
        }}
        onMouseEnter={(e) => { e.currentTarget.style.opacity = "0.85"; }}
        onMouseLeave={(e) => { e.currentTarget.style.opacity = "1"; }}
      >
        <svg width={11} height={11} viewBox="0 0 12 12" fill="white" stroke="none">
          <path d="M2 1.5l9 4.5-9 4.5z" />
        </svg>
        Start
      </button>
    );
  } else if (runtime_status === "paused") {
    cta = (
      <button
        onClick={onResume}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 5,
          fontSize: 12.5,
          fontWeight: 600,
          color: "#fff",
          background: "rgba(62,207,142,0.85)",
          border: "1px solid #3ecf8e",
          borderRadius: 7,
          padding: "7px 16px",
          cursor: "pointer",
          transition: "opacity 0.15s",
        }}
        onMouseEnter={(e) => { e.currentTarget.style.opacity = "0.85"; }}
        onMouseLeave={(e) => { e.currentTarget.style.opacity = "1"; }}
      >
        <svg width={11} height={11} viewBox="0 0 12 12" fill="white" stroke="none">
          <path d="M2 1.5l9 4.5-9 4.5z" />
        </svg>
        Resume
      </button>
    );
  } else if (
    runtime_status === "queued" ||
    parsing_status === "parsing" ||
    parsing_status === "planning"
  ) {
    cta = (
      <button
        onClick={onAbort}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 5,
          fontSize: 12.5,
          fontWeight: 600,
          color: "#fb923c",
          background: "rgba(251,146,60,0.08)",
          border: "1px solid rgba(251,146,60,0.2)",
          borderRadius: 7,
          padding: "7px 16px",
          cursor: "pointer",
          transition: "opacity 0.15s",
        }}
      >
        Cancel
      </button>
    );
  } else if (
    (runtime_status === "aborted" || graph.status === "failed") &&
    rerun_count < max_reruns
  ) {
    cta = (
      <button
        onClick={() => onRestart(false)}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 5,
          fontSize: 12.5,
          fontWeight: 600,
          color: "#a78bfa",
          background: "rgba(167,139,250,0.1)",
          border: "1px solid rgba(167,139,250,0.2)",
          borderRadius: 7,
          padding: "7px 16px",
          cursor: "pointer",
          transition: "opacity 0.15s",
        }}
      >
        Restart
      </button>
    );
  }

  return (
    <div
      style={{
        borderTop: "1px solid #222329",
        padding: "10px 14px",
        display: "flex",
        alignItems: "center",
        gap: 8,
        flexShrink: 0,
      }}
    >
      <button
        style={{
          display: "flex",
          alignItems: "center",
          gap: 5,
          fontSize: 12,
          fontWeight: 500,
          color: "#8b8d98",
          background: "transparent",
          border: "1px solid #222329",
          borderRadius: 7,
          padding: "6px 11px",
          cursor: "pointer",
          transition: "border-color 0.15s, background 0.15s",
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.borderColor = "#2a2c33";
          e.currentTarget.style.background = "rgba(255,255,255,0.03)";
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.borderColor = "#222329";
          e.currentTarget.style.background = "transparent";
        }}
      >
        <svg
          width={13}
          height={13}
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth={1.8}
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <line x1={6} y1={3} x2={6} y2={15} />
          <circle cx={6} cy={18} r={3} />
          <line x1={18} y1={6} x2={18} y2={15} />
          <circle cx={18} cy={3} r={3} />
          <line x1={12} y1={21} x2={12} y2={9} />
          <circle cx={12} cy={6} r={3} />
        </svg>
        Diff
      </button>

      <button
        style={{
          display: "flex",
          alignItems: "center",
          gap: 5,
          fontSize: 12,
          fontWeight: 500,
          color: "#8b8d98",
          background: "transparent",
          border: "1px solid #222329",
          borderRadius: 7,
          padding: "6px 11px",
          cursor: "pointer",
          transition: "border-color 0.15s, background 0.15s",
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.borderColor = "#2a2c33";
          e.currentTarget.style.background = "rgba(255,255,255,0.03)";
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.borderColor = "#222329";
          e.currentTarget.style.background = "transparent";
        }}
      >
        Report
      </button>

      <div style={{ flex: 1 }} />

      {cta}
    </div>
  );
}

/* ── Bug report modal ── */
interface BugReportState {
  open: boolean;
  description: string;
  stepId: string | null;
  phaseId: string | null;
  submitting: boolean;
  error: string | null;
}

function BugReportModal({
  state,
  onDescChange,
  onSubmit,
  onClose,
}: {
  state: BugReportState;
  onDescChange: (v: string) => void;
  onSubmit: () => void;
  onClose: () => void;
}) {
  if (!state.open) return null;
  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 300,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.6)",
        backdropFilter: "blur(6px)",
        WebkitBackdropFilter: "blur(6px)",
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 440,
          background: "#1a1b20",
          borderRadius: 10,
          border: "1px solid #2a2c33",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            padding: "14px 18px",
            background: "rgba(255,255,255,0.03)",
            borderBottom: "1px solid #2a2c33",
            fontSize: 13,
            fontWeight: 700,
            color: "#e2e4e9",
          }}
        >
          Report Bug
        </div>
        <div style={{ padding: "16px 18px", display: "flex", flexDirection: "column", gap: 12 }}>
          <textarea
            autoFocus
            value={state.description}
            onChange={(e) => onDescChange(e.target.value)}
            placeholder="Describe what went wrong…"
            rows={4}
            style={{
              width: "100%",
              background: "#0d0e11",
              border: "1px solid #2a2c33",
              borderRadius: 6,
              padding: "8px 10px",
              color: "#e2e4e9",
              fontSize: 12,
              resize: "none",
              outline: "none",
              lineHeight: 1.6,
              boxSizing: "border-box",
              fontFamily: "inherit",
            }}
          />
          {state.error && (
            <div style={{ fontSize: 11, color: "#f87171" }}>{state.error}</div>
          )}
        </div>
        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 8,
            padding: "12px 18px",
            background: "#0d0e11",
            borderTop: "1px solid #2a2c33",
          }}
        >
          <button
            onClick={onClose}
            style={{
              padding: "6px 14px",
              borderRadius: 5,
              background: "transparent",
              border: "none",
              color: "#5c5e6a",
              fontSize: 11,
              cursor: "pointer",
            }}
          >
            Cancel
          </button>
          <button
            onClick={onSubmit}
            disabled={!state.description.trim() || state.submitting}
            style={{
              padding: "6px 14px",
              borderRadius: 5,
              background:
                state.description.trim() && !state.submitting
                  ? "rgba(248,113,113,0.7)"
                  : "rgba(255,255,255,0.06)",
              border: "none",
              color:
                state.description.trim() && !state.submitting ? "#fff" : "#5c5e6a",
              fontSize: 11,
              fontWeight: 600,
              cursor:
                state.description.trim() && !state.submitting ? "pointer" : "default",
            }}
          >
            {state.submitting ? "Sending…" : "Submit Report"}
          </button>
        </div>
      </div>
    </div>
  );
}

/* ═══════════════════════════════════════════════════════
   MAIN PANEL
   ═══════════════════════════════════════════════════════ */

interface GraphPanelProps {
  conversationId: string;
}

export function GraphPanel({ conversationId }: GraphPanelProps) {
  const queryClient = useQueryClient();

  // ── Local state ──────────────────────────────────────────────────────────
  const [selectedStepId, setSelectedStepId] = useState<string | null>(null);
  const [selectedPhaseId, setSelectedPhaseId] = useState<string | null>(null);
  const [createModalOpen, setCreateModalOpen] = useState(false);
  const [clarificationOpen, setClarificationOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<"phases" | "logs">("phases");
  const [viewMode, setViewMode] = useState<"list" | "detail">("list");
  const [bugReport, setBugReport] = useState<BugReportState>({
    open: false,
    description: "",
    stepId: null,
    phaseId: null,
    submitting: false,
    error: null,
  });

  // ── Inject keyframes once ────────────────────────────────────────────────
  useEffect(() => {
    const id = "grove-graph-keyframes";
    if (!document.getElementById(id)) {
      const style = document.createElement("style");
      style.id = id;
      style.textContent = GRAPH_KEYFRAMES;
      document.head.appendChild(style);
    }
  }, []);

  // ── Queries ───────────────────────────────────────────────────────────────
  const { data: activeGraph } = useQuery({
    queryKey: qk.graphActive(conversationId),
    queryFn: () => getActiveGraph(conversationId),
    refetchInterval: 3_000, // always fast — lightweight query, ensures status transitions are caught
  });

  const activeGraphId = activeGraph?.id ?? null;
  const isRunning =
    activeGraph?.runtime_status === "running" || activeGraph?.runtime_status === "queued";
  const isPollingFast = isRunning
    || activeGraph?.parsing_status === "generating"
    || activeGraph?.parsing_status === "planning"
    || activeGraph?.parsing_status === "parsing";

  const { data: graphs = [], isLoading: graphsLoading } = useQuery({
    queryKey: qk.graphs(conversationId),
    queryFn: () => listGraphs(conversationId),
    refetchInterval: isPollingFast ? 3_000 : 10_000,
  });

  const { data: detail, isLoading: detailLoading } = useQuery({
    queryKey: activeGraphId
      ? qk.graphDetail(activeGraphId)
      : ["graphs", "detail", "__none__"],
    queryFn: () =>
      activeGraphId
        ? getGraphDetail(activeGraphId)
        : Promise.reject(new Error("No active graph")),
    enabled: !!activeGraphId,
    refetchInterval: isPollingFast ? 3_000 : 10_000,
  });

  const showDocEditor =
    detail?.graph?.parsing_status === "generating" ||
    detail?.graph?.parsing_status === "draft_ready" ||
    detail?.graph?.parsing_status === "error";

  // ── Drawer resolution ─────────────────────────────────────────────────────
  const selectedStep: GraphStepRecord | null = React.useMemo(() => {
    if (!selectedStepId || !detail) return null;
    for (const pd of detail.phases) {
      const s = pd.steps.find((s) => s.id === selectedStepId);
      if (s) return s;
    }
    return null;
  }, [selectedStepId, detail]);

  const selectedPhaseDetail = React.useMemo(() => {
    if (!selectedPhaseId || !detail) return null;
    return detail.phases.find((pd) => pd.phase.id === selectedPhaseId) ?? null;
  }, [selectedPhaseId, detail]);

  // ── Activity feed ─────────────────────────────────────────────────────────
  const activities = React.useMemo<Activity[]>(() => {
    if (!detail?.graph) return [];
    const g = detail.graph;
    const items: Activity[] = [];
    if (g.progress_summary) {
      items.push({ msg: g.progress_summary, color: "#fb923c" });
    }
    if (g.current_phase) {
      items.push({ src: "phase", msg: g.current_phase, color: "#fb923c" });
    }
    if (g.next_step) {
      items.push({ src: "next", msg: g.next_step, color: "#5c5e6a" });
    }
    return items;
  }, [detail]);

  // ── Invalidation helper ────────────────────────────────────────────────────
  const invalidate = useCallback(
    (graphId: string) => {
      void queryClient.invalidateQueries({ queryKey: qk.graphDetail(graphId) });
      void queryClient.invalidateQueries({ queryKey: qk.graphActive(conversationId) });
      void queryClient.invalidateQueries({ queryKey: qk.graphs(conversationId) });
    },
    [queryClient, conversationId],
  );

  // ── Handlers ──────────────────────────────────────────────────────────────
  const handleStart = useCallback(() => {
    if (!activeGraphId) return;
    setClarificationOpen(true);
  }, [activeGraphId]);

  const handleClarificationStarted = useCallback(() => {
    setClarificationOpen(false);
    if (activeGraphId) invalidate(activeGraphId);
  }, [activeGraphId, invalidate]);

  const handlePause = useCallback(async () => {
    if (!activeGraphId) return;
    await pauseGraph(activeGraphId);
    invalidate(activeGraphId);
  }, [activeGraphId, invalidate]);

  const handleResume = useCallback(async () => {
    if (!activeGraphId) return;
    await resumeGraph(activeGraphId);
    invalidate(activeGraphId);
  }, [activeGraphId, invalidate]);

  const handleAbort = useCallback(async () => {
    if (!activeGraphId) return;
    await abortGraph(activeGraphId);
    invalidate(activeGraphId);
  }, [activeGraphId, invalidate]);

  const handleRestart = useCallback(
    async (fullRestart: boolean) => {
      if (!activeGraphId) return;
      await restartGraph(activeGraphId, fullRestart);
      invalidate(activeGraphId);
    },
    [activeGraphId, invalidate],
  );

  const handleModeChange = useCallback(
    async (mode: GraphExecutionMode) => {
      if (!activeGraphId) return;
      await setGraphExecutionMode(activeGraphId, mode);
      invalidate(activeGraphId);
    },
    [activeGraphId, invalidate],
  );

  const handleSwitch = useCallback(
    async (graphId: string) => {
      await setActiveGraph(graphId);
      void queryClient.invalidateQueries({ queryKey: qk.graphActive(conversationId) });
    },
    [queryClient, conversationId],
  );

  const handleListSelect = useCallback(
    async (graphId: string) => {
      await handleSwitch(graphId);
      setViewMode("detail");
    },
    [handleSwitch],
  );

  const handleBugReport = useCallback(() => {
    setBugReport((prev) => ({
      ...prev,
      open: true,
      description: "",
      error: null,
      stepId: selectedStepId,
      phaseId: selectedPhaseId,
    }));
  }, [selectedStepId, selectedPhaseId]);

  const handleBugSubmit = useCallback(async () => {
    if (!activeGraphId || !bugReport.description.trim()) return;
    setBugReport((prev) => ({ ...prev, submitting: true, error: null }));
    try {
      await reportGraphBug(
        activeGraphId,
        bugReport.description,
        bugReport.stepId ?? undefined,
        bugReport.phaseId ?? undefined,
      );
      setBugReport({
        open: false,
        description: "",
        stepId: null,
        phaseId: null,
        submitting: false,
        error: null,
      });
    } catch (e) {
      setBugReport((prev) => ({
        ...prev,
        submitting: false,
        error: e instanceof Error ? e.message : String(e),
      }));
    }
  }, [activeGraphId, bugReport]);

  const handleCreated = useCallback(
    (createdDetail: GraphDetail) => {
      void queryClient.invalidateQueries({ queryKey: qk.graphs(conversationId) });
      void queryClient.invalidateQueries({ queryKey: qk.graphActive(conversationId) });
      void queryClient.setQueryData(qk.graphDetail(createdDetail.graph.id), createdDetail);
    },
    [queryClient, conversationId],
  );

  // ── Empty state — no graphs ───────────────────────────────────────────────
  if (!graphsLoading && graphs.length === 0) {
    return (
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          height: "100%",
          gap: 14,
        }}
      >
        <div
          style={{
            width: 52,
            height: 52,
            borderRadius: 13,
            background: "rgba(255,255,255,0.03)",
            border: "1px dashed #2a2c33",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <svg
            width={24}
            height={24}
            viewBox="0 0 40 40"
            fill="none"
            stroke="#5c5e6a"
            strokeWidth={1.5}
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <circle cx={20} cy={10} r={4} />
            <circle cx={8} cy={30} r={4} />
            <circle cx={32} cy={30} r={4} />
            <line x1={20} y1={14} x2={20} y2={22} />
            <line x1={20} y1={22} x2={9} y2={22} />
            <line x1={20} y1={22} x2={31} y2={22} />
            <line x1={9} y1={22} x2={9} y2={26} />
            <line x1={31} y1={22} x2={31} y2={26} />
          </svg>
        </div>
        <div style={{ textAlign: "center" }}>
          <div style={{ fontSize: 14, fontWeight: 600, color: "#e2e4e9", marginBottom: 6 }}>
            No graphs yet
          </div>
          <div style={{ fontSize: 12, color: "#5c5e6a" }}>Create one to get started</div>
        </div>
        <button
          onClick={() => setCreateModalOpen(true)}
          style={{
            marginTop: 4,
            padding: "8px 20px",
            borderRadius: 7,
            background: "rgba(62,207,142,0.15)",
            border: "1px solid rgba(62,207,142,0.3)",
            color: "#3ecf8e",
            fontSize: 13,
            fontWeight: 600,
            cursor: "pointer",
          }}
        >
          Create Graph
        </button>

        <CreateGraphModal
          open={createModalOpen}
          onClose={() => setCreateModalOpen(false)}
          conversationId={conversationId}
          onCreated={handleCreated}
        />
      </div>
    );
  }

  // ── Loading skeleton ──────────────────────────────────────────────────────
  if (graphsLoading && graphs.length === 0) {
    return (
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: "100%",
          color: "#5c5e6a",
          fontSize: 12,
        }}
      >
        Loading…
      </div>
    );
  }

  // ── Derive state flags ────────────────────────────────────────────────────
  const graph = detail?.graph ?? activeGraph;
  const phases = detail?.phases ?? [];
  const totalSteps = graph?.steps_created_count ?? 0;
  const closedSteps = graph?.steps_closed_count ?? 0;
  const isParsing =
    graph?.parsing_status === "parsing"
    || graph?.parsing_status === "planning"
    || graph?.parsing_status === "generating";
  const hasPhases = phases.length > 0;

  // ── List view ────────────────────────────────────────────────────────────
  if (viewMode === "list") {
    return (
      <div style={{ display: "flex", flexDirection: "column", height: "100%", background: "#0d0e11", overflow: "hidden" }}>
        <GraphList
          graphs={graphs as GraphRecord[]}
          activeGraphId={activeGraphId}
          onSelect={(id) => void handleListSelect(id)}
          onNew={() => setCreateModalOpen(true)}
        />
        <CreateGraphModal
          open={createModalOpen}
          onClose={() => setCreateModalOpen(false)}
          conversationId={conversationId}
          onCreated={handleCreated}
        />
      </div>
    );
  }

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        background: "#0d0e11",
        overflow: "hidden",
      }}
    >
      {/* 1. Back button + title bar (replaces GraphSwitcher) */}
      <div style={{
        display: "flex", alignItems: "center", gap: 8,
        padding: "10px 14px",
        borderBottom: "1px solid #222329", flexShrink: 0,
      }}>
        <button
          onClick={() => setViewMode("list")}
          style={{
            display: "flex", alignItems: "center", gap: 5,
            background: "transparent", border: "none",
            color: "#5c5e6a", cursor: "pointer", padding: 0,
            fontSize: 12, flexShrink: 0,
            transition: "color 0.15s",
          }}
          onMouseEnter={e => { e.currentTarget.style.color = "#8b8d98"; }}
          onMouseLeave={e => { e.currentTarget.style.color = "#5c5e6a"; }}
        >
          <svg width={12} height={12} viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
            <polyline points="8,2 4,6 8,10" />
          </svg>
          All Graphs
        </button>
        {graph && (() => {
          const ic = identityColor(graph.id);
          const initial = graph.title.trim().charAt(0).toUpperCase() || "G";
          return (
            <span style={{
              width: 20, height: 20, borderRadius: 5, flexShrink: 0,
              background: `${ic}1a`, border: `1px solid ${ic}40`,
              display: "inline-flex", alignItems: "center", justifyContent: "center",
              fontSize: 10, fontWeight: 700, color: ic,
            }}>
              {initial}
            </span>
          );
        })()}
        <div style={{ flex: 1, minWidth: 0 }}>
          <span style={{
            fontSize: 14, fontWeight: 600, color: "#e2e4e9",
            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
            display: "block",
          }}>
            {graph?.title ?? ""}
          </span>
          {(() => {
            const agent = detail?.phases?.find(pd => pd.phase.execution_agent)?.phase.execution_agent;
            if (!agent) return null;
            return (
              <span style={{
                fontSize: 10, color: "#4a4d5a",
                fontFamily: MONO, display: "block",
                overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                marginTop: 1,
              }}>
                {agent}
              </span>
            );
          })()}
        </div>
        {graph && (
          <GraphStatusBadge
            status={graph.runtime_status !== "idle" ? graph.runtime_status : graph.status}
            size="sm"
          />
        )}
        <button
          onClick={() => setCreateModalOpen(true)}
          title="New Graph"
          style={{
            display: "flex", alignItems: "center", justifyContent: "center",
            width: 24, height: 24, borderRadius: 4, flexShrink: 0,
            background: "rgba(255,255,255,0.04)", border: "1px solid #222329",
            color: "#5c5e6a", cursor: "pointer",
          }}
          onMouseEnter={e => { e.currentTarget.style.background = "rgba(255,255,255,0.07)"; }}
          onMouseLeave={e => { e.currentTarget.style.background = "rgba(255,255,255,0.04)"; }}
        >
          <svg width={10} height={10} viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round">
            <line x1={6} y1={1} x2={6} y2={11} />
            <line x1={1} y1={6} x2={11} y2={6} />
          </svg>
        </button>
      </div>

      {/* 2a. Document editor (draft/generating) */}
      {detail && showDocEditor ? (
        <DocumentEditorPanel
          graph={detail.graph}
          onSaved={() => invalidate(detail.graph.id)}
          onDiscarded={() => {
            void queryClient.invalidateQueries({ queryKey: qk.graphs(conversationId) });
            void queryClient.invalidateQueries({ queryKey: qk.graphActive(conversationId) });
          }}
        />
      ) : (
        <>
          {/* 2b. Control bar */}
          {detail && (
            <GraphControlBar
              graph={detail.graph}
              onStart={() => void handleStart()}
              onPause={() => void handlePause()}
              onResume={() => void handleResume()}
              onAbort={() => void handleAbort()}
              onRestart={(full) => void handleRestart(full)}
              onBugReport={handleBugReport}
              onModeChange={(mode) => void handleModeChange(mode)}
            />
          )}

          {/* 3. Overall progress bar + activity ticker */}
          {graph && (
            <OverallProgressBar
              closed={closedSteps}
              total={totalSteps}
              isRunning={graph.runtime_status === "running"}
              isParsing={isParsing}
            />
          )}

          {graph && isRunning && graph.runtime_status !== "queued" && activities.length > 0 && (
            <div style={{ padding: "8px 4px 0" }}>
              <ActivityTicker activities={activities} visible={isRunning} />
            </div>
          )}

          {/* 4. Tabs (only when there are phases) */}
          {hasPhases && (
            <TabBar
              activeTab={activeTab}
              onChange={setActiveTab}
              phasesCount={phases.length}
            />
          )}

          {/* 5. Content area */}
          <div style={{ flex: 1, overflowY: "auto" }}>
            {detailLoading && !detail && (
              <div
                style={{
                  padding: "24px",
                  color: "#5c5e6a",
                  fontSize: 12,
                  textAlign: "center",
                }}
              >
                Loading graph…
              </div>
            )}

            {detail && activeTab === "phases" && (
              <>
                {hasPhases ? (
                  <GraphTree
                    phases={phases}
                    onStepClick={(stepId) => setSelectedStepId(stepId)}
                    onPhaseClick={(phaseId) => setSelectedPhaseId(phaseId)}
                  />
                ) : isParsing ? (
                  <EmptyState status="parsing" parsingStatus={graph?.parsing_status} />
                ) : graph?.parsing_status === "complete" ? (
                  <EmptyState status="waiting" />
                ) : (
                  <EmptyState status="empty" />
                )}
              </>
            )}

            {detail && activeTab === "logs" && <LogsPlaceholder />}
          </div>

          {/* 6. Footer */}
          {graph && (
            <GraphFooter
              graph={graph}
              onStart={() => void handleStart()}
              onResume={() => void handleResume()}
              onAbort={() => void handleAbort()}
              onRestart={(full) => void handleRestart(full)}
            />
          )}
        </>
      )}

      {/* Drawers */}
      <StepDetailDrawer
        step={selectedStep}
        open={selectedStepId !== null}
        onClose={() => setSelectedStepId(null)}
      />

      <PhaseValidationDrawer
        phase={selectedPhaseDetail?.phase ?? null}
        steps={selectedPhaseDetail?.steps ?? []}
        open={selectedPhaseId !== null}
        onClose={() => setSelectedPhaseId(null)}
      />

      {/* Modals */}
      <CreateGraphModal
        open={createModalOpen}
        onClose={() => setCreateModalOpen(false)}
        conversationId={conversationId}
        onCreated={handleCreated}
      />

      {activeGraphId && (
        <ClarificationModal
          graphId={activeGraphId}
          open={clarificationOpen}
          onClose={() => setClarificationOpen(false)}
          onStarted={handleClarificationStarted}
        />
      )}

      <BugReportModal
        state={bugReport}
        onDescChange={(v) => setBugReport((prev) => ({ ...prev, description: v }))}
        onSubmit={() => void handleBugSubmit()}
        onClose={() =>
          setBugReport({
            open: false,
            description: "",
            stepId: null,
            phaseId: null,
            submitting: false,
            error: null,
          })
        }
      />
    </div>
  );
}
