import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { qk } from "@/lib/queryKeys";
import { listAutomationRuns, getAutomationRunSteps, cancelAutomationRun } from "@/lib/api";
import { C } from "@/lib/theme";
import { Dot, XIcon, ChevronDown, ChevronR } from "@/components/ui/icons";

interface Props {
  automationId: string;
}

// ── Helpers ──────────────────────────────────────────────────────────

function formatRelative(iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  const diff = Date.now() - d.getTime();
  const m = Math.floor(diff / 60000);
  if (m < 1) return "just now";
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const days = Math.floor(h / 24);
  if (days < 7) return `${days}d ago`;
  if (days < 30) return `${Math.floor(days / 7)}w ago`;
  return d.toLocaleDateString();
}

function formatDuration(startedAt: string | null, completedAt: string | null): string {
  if (!startedAt) return "\u2014";
  const start = new Date(startedAt).getTime();
  if (isNaN(start)) return "\u2014";
  const end = completedAt ? new Date(completedAt).getTime() : Date.now();
  if (isNaN(end)) return "\u2014";
  const diffMs = end - start;
  if (diffMs < 0) return "\u2014";
  const secs = Math.floor(diffMs / 1000);
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  const remSecs = secs % 60;
  if (mins < 60) return `${mins}m ${remSecs.toString().padStart(2, "0")}s`;
  const hrs = Math.floor(mins / 60);
  const remMins = mins % 60;
  return `${hrs}h ${remMins.toString().padStart(2, "0")}m`;
}

function runStatusColor(state: string): string {
  switch (state) {
    case "completed": return "#31B97B";
    case "failed":    return "#EF4444";
    case "running":   return "#3B82F6";
    case "cancelled": return "#F59E0B";
    default:          return "#52575F";
  }
}

function stepStatusLabel(state: string): { color: string; icon: string } {
  switch (state) {
    case "completed": return { color: "#31B97B", icon: "\u2713" };
    case "failed":    return { color: "#EF4444", icon: "\u2717" };
    case "running":
    case "queued":    return { color: "#3B82F6", icon: "\u25CF" };
    case "skipped":   return { color: "#F59E0B", icon: "\u25CB" };
    default:          return { color: "#52575F", icon: "\u25CB" };
  }
}

// ── Expanded run row ─────────────────────────────────────────────────

function RunStepsDetail({ runId }: { runId: string }) {
  const { data: runSteps = [], isLoading } = useQuery({
    queryKey: qk.automationRunSteps(runId),
    queryFn: () => getAutomationRunSteps(runId),
    enabled: !!runId,
  });

  if (isLoading) {
    return (
      <div style={{ padding: "10px 16px", fontSize: 11, color: "#64748b" }}>
        Loading steps...
      </div>
    );
  }

  if (runSteps.length === 0) {
    return (
      <div style={{ padding: "10px 16px", fontSize: 11, color: "#64748b" }}>
        No step records.
      </div>
    );
  }

  return (
    <div style={{ padding: "8px 0 4px" }}>
      {runSteps.map((rs) => {
        const st = stepStatusLabel(rs.state);
        return (
          <div
            key={rs.id}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 10,
              padding: "6px 24px",
              fontSize: 11,
            }}
          >
            <span style={{ color: "#64748b", minWidth: 36 }}>Step:</span>
            <span style={{ fontWeight: 600, color: C.text2, minWidth: 100 }}>
              {rs.step_key}
            </span>
            <span style={{ color: st.color, fontWeight: 600, minWidth: 16, textAlign: "center" }}>
              {st.icon}
            </span>
            <span style={{ color: st.color, minWidth: 80 }}>
              {rs.state}
            </span>
            <span style={{ color: "#64748b", fontFamily: C.mono, fontSize: 10, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {rs.error
                ? `Error: ${rs.error}`
                : rs.task_id
                  ? rs.task_id.slice(0, 20)
                  : rs.state === "skipped"
                    ? "condition not met"
                    : ""}
            </span>
          </div>
        );
      })}
    </div>
  );
}

// ── Main component ───────────────────────────────────────────────────

export function RunsTab({ automationId }: Props) {
  const [expandedRunId, setExpandedRunId] = useState<string | null>(null);
  const [cancellingId, setCancellingId] = useState<string | null>(null);

  const { data: runs = [], refetch } = useQuery({
    queryKey: qk.automationRuns(automationId),
    queryFn: () => listAutomationRuns(automationId, 50),
    refetchInterval: 10000,
  });

  async function handleCancel(runId: string) {
    setCancellingId(runId);
    try {
      await cancelAutomationRun(runId);
      await refetch();
    } finally {
      setCancellingId(null);
    }
  }

  if (runs.length === 0) {
    return (
      <div style={{ padding: "48px 0", textAlign: "center" }}>
        <div style={{ fontSize: 13, color: "#64748b" }}>
          No runs yet. Trigger this automation manually or wait for its schedule.
        </div>
      </div>
    );
  }

  return (
    <div style={{ padding: "4px 0" }}>
      {/* Table header */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "minmax(140px, 1fr) 110px 100px 90px 80px 40px",
          gap: 8,
          padding: "8px 16px",
          fontSize: 10,
          fontWeight: 600,
          color: "#64748b",
          textTransform: "uppercase" as const,
          letterSpacing: "0.06em",
          borderBottom: `1px solid ${C.border}`,
        }}
      >
        <span>Run ID</span>
        <span>Status</span>
        <span>Triggered</span>
        <span>Duration</span>
        <span>Steps</span>
        <span />
      </div>

      {/* Rows */}
      {runs.map((run) => {
        const isExpanded = expandedRunId === run.id;
        const statusCol = runStatusColor(run.state);
        const isCancelling = cancellingId === run.id;

        return (
          <div key={run.id}>
            <button
              onClick={() => setExpandedRunId(isExpanded ? null : run.id)}
              style={{
                display: "grid",
                gridTemplateColumns: "minmax(140px, 1fr) 110px 100px 90px 80px 40px",
                gap: 8,
                width: "100%",
                padding: "10px 16px",
                background: isExpanded ? C.surfaceHover : "transparent",
                border: "none",
                borderBottom: `1px solid ${C.border}`,
                cursor: "pointer",
                fontFamily: "inherit",
                fontSize: 12,
                textAlign: "left",
                transition: "background .12s",
                alignItems: "center",
              }}
              onMouseEnter={(e) => {
                if (!isExpanded) e.currentTarget.style.background = C.surfaceHover;
              }}
              onMouseLeave={(e) => {
                if (!isExpanded) e.currentTarget.style.background = "transparent";
              }}
            >
              {/* Run ID */}
              <span style={{ fontFamily: C.mono, fontSize: 11, color: C.text2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {run.id.slice(0, 16)}
              </span>

              {/* Status */}
              <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <Dot status={run.state} size={7} />
                <span style={{ color: statusCol, fontWeight: 600, fontSize: 11 }}>
                  {run.state}
                </span>
              </span>

              {/* Triggered */}
              <span style={{ color: "#64748b", fontSize: 11 }}>
                {formatRelative(run.created_at)}
              </span>

              {/* Duration */}
              <span style={{ fontFamily: C.mono, fontSize: 11, color: C.text2 }}>
                {run.state === "running" && !run.completed_at
                  ? formatDuration(run.started_at, null)
                  : formatDuration(run.started_at, run.completed_at)}
              </span>

              {/* Steps placeholder — we show a brief summary if expanded */}
              <span style={{ fontSize: 11, color: "#64748b" }}>
                {"\u2014"}
              </span>

              {/* Expand chevron */}
              <span style={{ display: "inline-flex", color: "#52575F", justifyContent: "center" }}>
                {isExpanded ? <ChevronDown size={10} /> : <ChevronR size={10} />}
              </span>
            </button>

            {/* Expanded content */}
            {isExpanded && (
              <div
                style={{
                  background: C.surfaceHover,
                  borderBottom: `1px solid ${C.border}`,
                  paddingBottom: 8,
                }}
              >
                <RunStepsDetail runId={run.id} />

                {/* Cancel button for running runs */}
                {(run.state === "running" || run.state === "pending") && (
                  <div style={{ padding: "4px 24px 8px" }}>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        handleCancel(run.id);
                      }}
                      disabled={isCancelling}
                      style={{
                        display: "inline-flex",
                        alignItems: "center",
                        gap: 5,
                        padding: "5px 12px",
                        borderRadius: 5,
                        border: `1px solid rgba(239,68,68,0.3)`,
                        background: C.dangerDim,
                        color: C.danger,
                        fontSize: 11,
                        fontWeight: 600,
                        cursor: isCancelling ? "default" : "pointer",
                        opacity: isCancelling ? 0.6 : 1,
                        fontFamily: "inherit",
                        transition: "background .12s",
                      }}
                      onMouseEnter={(e) => {
                        if (!isCancelling) e.currentTarget.style.background = "rgba(239,68,68,0.18)";
                      }}
                      onMouseLeave={(e) => {
                        if (!isCancelling) e.currentTarget.style.background = C.dangerDim;
                      }}
                    >
                      <XIcon size={9} />
                      {isCancelling ? "Cancelling..." : "Cancel Run"}
                    </button>
                  </div>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
