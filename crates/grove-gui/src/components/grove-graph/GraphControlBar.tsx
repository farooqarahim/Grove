import React, { useState, useRef, useEffect, useCallback } from "react";
import { PlayIcon, PauseIcon, StopIcon, RestartIcon, BugIcon } from "@/components/ui/icons";
import type { GraphExecutionMode, GraphRecord } from "@/types";

const MONO = "'JetBrains Mono', 'Fira Code', 'SF Mono', monospace";

const btnBase: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 5,
  padding: "5px 10px",
  borderRadius: 5,
  border: "none",
  fontSize: 11,
  fontWeight: 600,
  cursor: "pointer",
  lineHeight: 1,
  transition: "opacity 0.15s",
  whiteSpace: "nowrap",
};

interface GraphControlBarProps {
  graph: GraphRecord;
  onStart: () => void;
  onPause: () => void;
  onResume: () => void;
  onAbort: () => void;
  onRestart: (fullRestart: boolean) => void;
  onBugReport: () => void;
  onModeChange: (mode: GraphExecutionMode) => void;
}

function RestartDropdown({
  rerunCount,
  maxReruns,
  onRestart,
}: {
  rerunCount: number;
  maxReruns: number;
  onRestart: (fullRestart: boolean) => void;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  const handleClickOutside = useCallback((e: MouseEvent) => {
    if (ref.current && !ref.current.contains(e.target as Node)) {
      setOpen(false);
    }
  }, []);

  useEffect(() => {
    if (open) {
      document.addEventListener("mousedown", handleClickOutside);
      return () => document.removeEventListener("mousedown", handleClickOutside);
    }
  }, [open, handleClickOutside]);

  const dropdownItemStyle: React.CSSProperties = {
    display: "block",
    width: "100%",
    padding: "7px 12px",
    background: "transparent",
    border: "none",
    color: "#e2e4e9",
    fontSize: 11,
    textAlign: "left",
    cursor: "pointer",
    lineHeight: 1.4,
  };

  return (
    <div ref={ref} style={{ position: "relative" }}>
      <button
        onClick={() => setOpen((v) => !v)}
        style={{
          ...btnBase,
          background: "rgba(167,139,250,0.1)",
          color: "#a78bfa",
          border: "1px solid rgba(167,139,250,0.2)",
        }}
      >
        <RestartIcon />
        Restart
        <span style={{ fontFamily: MONO, fontSize: 9, opacity: 0.65, fontWeight: 400 }}>
          ({rerunCount}/{maxReruns})
        </span>
        <svg
          width={8}
          height={8}
          viewBox="0 0 8 8"
          fill="none"
          stroke="currentColor"
          strokeWidth={1.5}
          strokeLinecap="round"
          strokeLinejoin="round"
          style={{ marginLeft: 2 }}
        >
          <polyline points="1.5,3 4,5.5 6.5,3" />
        </svg>
      </button>
      {open && (
        <div
          style={{
            position: "absolute",
            top: "calc(100% + 4px)",
            left: 0,
            minWidth: 168,
            background: "#1a1b20",
            border: "1px solid #2a2c33",
            borderRadius: 6,
            overflow: "hidden",
            zIndex: 50,
            boxShadow: "0 8px 24px rgba(0,0,0,0.50)",
          }}
        >
          <button
            onClick={() => { setOpen(false); onRestart(false); }}
            onMouseEnter={(e) => { e.currentTarget.style.background = "rgba(255,255,255,0.04)"; }}
            onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; }}
            style={dropdownItemStyle}
          >
            <div style={{ fontWeight: 600 }}>Quick Restart</div>
            <div style={{ fontSize: 10, color: "#5c5e6a", marginTop: 2 }}>
              Resume from failed steps
            </div>
          </button>
          <div style={{ height: 1, background: "#2a2c33" }} />
          <button
            onClick={() => { setOpen(false); onRestart(true); }}
            onMouseEnter={(e) => { e.currentTarget.style.background = "rgba(255,255,255,0.04)"; }}
            onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; }}
            style={dropdownItemStyle}
          >
            <div style={{ fontWeight: 600 }}>Full Restart</div>
            <div style={{ fontSize: 10, color: "#5c5e6a", marginTop: 2 }}>
              Re-plan and re-execute from scratch
            </div>
          </button>
        </div>
      )}
    </div>
  );
}

export function GraphControlBar({
  graph,
  onStart,
  onPause,
  onResume,
  onAbort,
  onRestart,
  onBugReport,
  onModeChange,
}: GraphControlBarProps) {
  const {
    status,
    runtime_status,
    parsing_status,
    rerun_count,
    max_reruns,
    progress_summary,
    phases_created_count,
    steps_created_count,
    execution_mode,
  } = graph;

  const isComplete = status === "closed";
  const isFailed = status === "failed";
  const canStart = runtime_status === "idle" && parsing_status === "complete" && !isComplete;

  const progressText =
    progress_summary ??
    (phases_created_count > 0 || steps_created_count > 0
      ? `${phases_created_count} phases · ${steps_created_count} steps`
      : null);

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "8px 14px",
        borderBottom: "1px solid #222329",
        flexShrink: 0,
        minHeight: 42,
      }}
    >
      {/* Left: action buttons */}
      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
        {runtime_status === "idle" && isComplete && (
          <span
            style={{
              ...btnBase,
              background: "rgba(62,207,142,0.12)",
              color: "#3ecf8e",
              border: "1px solid rgba(62,207,142,0.25)",
              cursor: "default",
            }}
          >
            Completed
          </span>
        )}

        {runtime_status === "idle" && isFailed && rerun_count >= max_reruns && (
          <span
            style={{
              ...btnBase,
              background: "rgba(248,113,113,0.08)",
              color: "#f87171",
              border: "1px solid rgba(248,113,113,0.2)",
              cursor: "default",
            }}
          >
            Failed
          </span>
        )}

        {runtime_status === "idle" && !isComplete && !isFailed && (
          <button
            onClick={onStart}
            disabled={!canStart}
            style={{
              ...btnBase,
              background: canStart ? "rgba(62,207,142,0.15)" : "rgba(255,255,255,0.05)",
              color: canStart ? "#3ecf8e" : "#5c5e6a",
              border: `1px solid ${canStart ? "rgba(62,207,142,0.3)" : "rgba(255,255,255,0.06)"}`,
              opacity: canStart ? 1 : 0.6,
            }}
          >
            <PlayIcon />
            Start
          </button>
        )}

        {runtime_status === "queued" && (
          <>
            <span
              style={{
                ...btnBase,
                background: "rgba(167,139,250,0.1)",
                color: "#a78bfa",
                border: "1px solid rgba(167,139,250,0.2)",
                cursor: "default",
              }}
            >
              Queued
            </span>
            <button
              onClick={onAbort}
              style={{
                ...btnBase,
                background: "rgba(248,113,113,0.08)",
                color: "#f87171",
                border: "1px solid rgba(248,113,113,0.2)",
              }}
            >
              <StopIcon />
              Cancel
            </button>
          </>
        )}

        {runtime_status === "running" && (
          <>
            <button
              onClick={onPause}
              style={{
                ...btnBase,
                background: "rgba(245,158,11,0.08)",
                color: "#f59e0b",
                border: "1px solid rgba(245,158,11,0.2)",
              }}
            >
              <PauseIcon />
              Pause
            </button>
            <button
              onClick={onAbort}
              style={{
                ...btnBase,
                background: "rgba(248,113,113,0.08)",
                color: "#f87171",
                border: "1px solid rgba(248,113,113,0.2)",
              }}
            >
              <StopIcon />
              Abort
            </button>
          </>
        )}

        {runtime_status === "paused" && (
          <>
            <button
              onClick={onResume}
              style={{
                ...btnBase,
                background: "rgba(62,207,142,0.08)",
                color: "#3ecf8e",
                border: "1px solid rgba(62,207,142,0.2)",
              }}
            >
              <PlayIcon />
              Resume
            </button>
            <button
              onClick={onAbort}
              style={{
                ...btnBase,
                background: "rgba(248,113,113,0.08)",
                color: "#f87171",
                border: "1px solid rgba(248,113,113,0.2)",
              }}
            >
              <StopIcon />
              Abort
            </button>
          </>
        )}

        {(runtime_status === "aborted" || graph.status === "failed") &&
          rerun_count < max_reruns && (
            <RestartDropdown
              rerunCount={rerun_count}
              maxReruns={max_reruns}
              onRestart={onRestart}
            />
          )}

        {(parsing_status === "planning" || parsing_status === "parsing" || parsing_status === "generating") && (
          <span
            style={{
              ...btnBase,
              background: "rgba(96,165,250,0.08)",
              color: "#60a5fa",
              border: "1px solid rgba(96,165,250,0.2)",
              cursor: "default",
            }}
          >
            {parsing_status === "planning" ? "Planning…" : parsing_status === "generating" ? "Generating…" : "Parsing…"}
          </span>
        )}
      </div>

      {/* Center: progress text */}
      <div style={{ flex: 1, overflow: "hidden", minWidth: 0 }}>
        {progressText && (
          <span
            style={{
              fontFamily: MONO,
              fontSize: 11,
              color: "#5c5e6a",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              display: "block",
            }}
          >
            {progressText}
          </span>
        )}
      </div>

      {/* Right: mode toggle + bug report */}
      <div style={{ display: "flex", alignItems: "center", gap: 6, flexShrink: 0 }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            background: "rgba(255,255,255,0.04)",
            border: "1px solid #222329",
            borderRadius: 5,
            overflow: "hidden",
          }}
        >
          {(["sequential", "parallel"] as GraphExecutionMode[]).map((mode) => {
            const isActive = execution_mode === mode;
            return (
              <button
                key={mode}
                onClick={() => onModeChange(mode)}
                style={{
                  padding: "4px 10px",
                  border: "none",
                  background: isActive ? "rgba(255,255,255,0.06)" : "transparent",
                  color: isActive ? "#e2e4e9" : "#5c5e6a",
                  fontSize: 11,
                  fontWeight: isActive ? 600 : 400,
                  cursor: "pointer",
                  letterSpacing: "0.02em",
                  transition: "background 0.15s, color 0.15s",
                }}
              >
                {mode === "sequential" ? "Seq" : "Par"}
              </button>
            );
          })}
        </div>

        <button
          onClick={onBugReport}
          title="Report a bug"
          style={{
            ...btnBase,
            padding: "5px 7px",
            background: "transparent",
            color: "#5c5e6a",
            border: "1px solid transparent",
          }}
        >
          <BugIcon />
        </button>
      </div>
    </div>
  );
}
