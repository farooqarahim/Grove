import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { qk } from "@/lib/queryKeys";
import { listAutomationSteps, listAutomationRuns, getAutomationRunSteps } from "@/lib/api";
import { C, lbl } from "@/lib/theme";
import { Plus, ChevronR } from "@/components/ui/icons";
import { AddStepModal } from "./AddStepModal";
import type { AutomationStep, AutomationRunStep } from "@/types";

interface Props {
  automationId: string;
}

// ── Status dot colors ────────────────────────────────────────────────

function stepStatusDot(state: string): { color: string; label: string } {
  switch (state) {
    case "completed": return { color: "#31B97B", label: "Completed" };
    case "failed":    return { color: "#EF4444", label: "Failed" };
    case "running":
    case "queued":    return { color: "#3B82F6", label: state.charAt(0).toUpperCase() + state.slice(1) };
    case "skipped":   return { color: "#F59E0B", label: "Skipped" };
    default:          return { color: "#52575F", label: "Pending" };
  }
}

// ── StepNode ─────────────────────────────────────────────────────────

function StepNode({
  step,
  runStep,
  expanded,
  onToggle,
}: {
  step: AutomationStep;
  runStep: AutomationRunStep | null;
  expanded: boolean;
  onToggle: () => void;
}) {
  const status = runStep ? stepStatusDot(runStep.state) : stepStatusDot("pending");

  return (
    <div style={{ display: "flex", flexDirection: "column", minWidth: 180 }}>
      <button
        onClick={onToggle}
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 6,
          padding: "16px 20px",
          background: C.surface,
          border: `1px solid ${expanded ? C.borderHover : C.border}`,
          borderRadius: 8,
          cursor: "pointer",
          fontFamily: "inherit",
          transition: "border-color .12s, background .12s",
          minWidth: 160,
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.borderColor = C.borderHover;
          e.currentTarget.style.background = C.surfaceHover;
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.borderColor = expanded ? C.borderHover : C.border;
          e.currentTarget.style.background = C.surface;
        }}
      >
        <span style={{ fontSize: 12, fontWeight: 700, color: C.text1 }}>
          {step.step_key}
        </span>
        {step.provider && (
          <span style={{ fontSize: 10, color: "#64748b" }}>
            {step.provider}
          </span>
        )}
        <span
          title={status.label}
          style={{
            width: 8,
            height: 8,
            borderRadius: "50%",
            background: status.color,
            boxShadow: runStep?.state === "running" ? `0 0 6px ${status.color}` : undefined,
          }}
        />
        <span style={{ display: "inline-flex", transform: expanded ? "rotate(90deg)" : "rotate(0deg)", transition: "transform .15s" }}>
          <ChevronR size={10} />
        </span>
      </button>

      {/* Expanded detail panel */}
      {expanded && (
        <div
          style={{
            marginTop: 8,
            padding: "12px 14px",
            background: C.surfaceHover,
            border: `1px solid ${C.border}`,
            borderRadius: 6,
            fontSize: 12,
            color: C.text2,
          }}
        >
          <div style={{ marginBottom: 8 }}>
            <div style={{ ...lbl, marginBottom: 4 }}>Objective</div>
            <div style={{ color: "#94a3b8", lineHeight: 1.5 }}>{step.objective}</div>
          </div>

          {step.condition && (
            <div style={{ marginBottom: 8 }}>
              <div style={{ ...lbl, marginBottom: 4 }}>Condition</div>
              <div style={{ fontFamily: C.mono, fontSize: 11, color: C.purple }}>{step.condition}</div>
            </div>
          )}

          {step.depends_on.length > 0 && (
            <div style={{ marginBottom: 8 }}>
              <div style={{ ...lbl, marginBottom: 4 }}>Depends on</div>
              <div style={{ display: "flex", gap: 4, flexWrap: "wrap" }}>
                {step.depends_on.map((dep) => (
                  <span
                    key={dep}
                    style={{
                      fontSize: 10,
                      fontWeight: 600,
                      color: C.blue,
                      background: C.blueDim,
                      padding: "2px 7px",
                      borderRadius: 4,
                    }}
                  >
                    {dep}
                  </span>
                ))}
              </div>
            </div>
          )}

          {/* Overrides */}
          {(step.model || step.pipeline || step.permission_mode) && (
            <div>
              <div style={{ ...lbl, marginBottom: 4 }}>Overrides</div>
              <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
                {step.model && (
                  <Row label="Model" value={step.model} />
                )}
                {step.pipeline && (
                  <Row label="Pipeline" value={step.pipeline} />
                )}
                {step.permission_mode && (
                  <Row label="Permissions" value={step.permission_mode} />
                )}
              </div>
            </div>
          )}

          {/* Run step info */}
          {runStep && (
            <div style={{ marginTop: 8, paddingTop: 8, borderTop: `1px solid ${C.border}` }}>
              <div style={{ ...lbl, marginBottom: 4 }}>Latest run</div>
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <span
                  style={{
                    width: 6,
                    height: 6,
                    borderRadius: "50%",
                    background: status.color,
                    flexShrink: 0,
                  }}
                />
                <span style={{ fontSize: 11, color: status.color, fontWeight: 600 }}>
                  {status.label}
                </span>
                {runStep.error && (
                  <span style={{ fontSize: 11, color: C.danger, marginLeft: 4 }}>
                    {runStep.error}
                  </span>
                )}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ display: "flex", justifyContent: "space-between", fontSize: 11 }}>
      <span style={{ color: "#64748b" }}>{label}</span>
      <span style={{ color: C.text2, fontFamily: C.mono, fontSize: 10 }}>{value}</span>
    </div>
  );
}

// ── Arrow connector ──────────────────────────────────────────────────

function ArrowConnector() {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        padding: "0 4px",
        color: "#52575F",
        fontSize: 16,
        userSelect: "none",
        alignSelf: "flex-start",
        marginTop: 28,
      }}
    >
      <svg width="32" height="12" viewBox="0 0 32 12">
        <line x1="0" y1="6" x2="26" y2="6" stroke="#52575F" strokeWidth="1.5" />
        <polyline points="23,2 28,6 23,10" fill="none" stroke="#52575F" strokeWidth="1.5" />
      </svg>
    </div>
  );
}

// ── Main component ───────────────────────────────────────────────────

export function StepsTab({ automationId }: Props) {
  const [expandedStepId, setExpandedStepId] = useState<string | null>(null);
  const [showAddStep, setShowAddStep] = useState(false);

  const { data: steps = [] } = useQuery({
    queryKey: qk.automationSteps(automationId),
    queryFn: () => listAutomationSteps(automationId),
  });

  const { data: runs = [] } = useQuery({
    queryKey: qk.automationRuns(automationId),
    queryFn: () => listAutomationRuns(automationId, 1),
  });

  const latestRun = runs[0] ?? null;

  const { data: runSteps = [] } = useQuery({
    queryKey: qk.automationRunSteps(latestRun?.id ?? ""),
    queryFn: () => latestRun ? getAutomationRunSteps(latestRun.id) : Promise.resolve([]),
    enabled: !!latestRun,
  });

  // Build a map from step_id -> latest run step status
  const runStepMap = new Map<string, AutomationRunStep>();
  for (const rs of runSteps) {
    runStepMap.set(rs.step_id, rs);
  }

  // Sort steps by ordinal
  const sorted = [...steps].sort((a, b) => a.ordinal - b.ordinal);

  // Group steps by ordinal for parallel display
  const ordinalGroups: AutomationStep[][] = [];
  let currentOrdinal = -1;
  for (const step of sorted) {
    if (step.ordinal !== currentOrdinal) {
      ordinalGroups.push([step]);
      currentOrdinal = step.ordinal;
    } else {
      ordinalGroups[ordinalGroups.length - 1].push(step);
    }
  }

  const existingStepKeys = sorted.map(s => s.step_key);
  const nextOrdinal = sorted.length > 0 ? sorted[sorted.length - 1].ordinal + 1 : 0;

  if (steps.length === 0) {
    return (
      <div style={{ padding: "48px 0", textAlign: "center" }}>
        <div style={{ fontSize: 13, color: "#64748b", marginBottom: 16 }}>
          No steps defined. Add steps to build your automation workflow.
        </div>
        <button
          onClick={() => setShowAddStep(true)}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 6,
            padding: "8px 16px",
            borderRadius: 6,
            border: `1px solid ${C.border}`,
            background: C.surface,
            color: C.text2,
            fontSize: 12,
            fontWeight: 600,
            cursor: "pointer",
            fontFamily: "inherit",
            transition: "background .12s, border-color .12s",
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = C.surfaceHover;
            e.currentTarget.style.borderColor = C.borderHover;
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = C.surface;
            e.currentTarget.style.borderColor = C.border;
          }}
        >
          <Plus size={12} />
          Add Step
        </button>
        <AddStepModal
          open={showAddStep}
          automationId={automationId}
          existingStepKeys={[]}
          nextOrdinal={0}
          onClose={() => setShowAddStep(false)}
        />
      </div>
    );
  }

  return (
    <div style={{ padding: "4px 0" }}>
      {/* DAG flow */}
      <div
        style={{
          display: "flex",
          alignItems: "flex-start",
          gap: 0,
          overflowX: "auto",
          paddingBottom: 16,
        }}
      >
        {ordinalGroups.map((group, gi) => (
          <div key={gi} style={{ display: "contents" }}>
            {gi > 0 && <ArrowConnector />}
            {group.length === 1 ? (
              <StepNode
                step={group[0]}
                runStep={runStepMap.get(group[0].id) ?? null}
                expanded={expandedStepId === group[0].id}
                onToggle={() =>
                  setExpandedStepId(expandedStepId === group[0].id ? null : group[0].id)
                }
              />
            ) : (
              <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                {group.map((step) => (
                  <StepNode
                    key={step.id}
                    step={step}
                    runStep={runStepMap.get(step.id) ?? null}
                    expanded={expandedStepId === step.id}
                    onToggle={() =>
                      setExpandedStepId(expandedStepId === step.id ? null : step.id)
                    }
                  />
                ))}
              </div>
            )}
          </div>
        ))}
      </div>

      {/* Add Step button */}
      <div style={{ marginTop: 20 }}>
        <button
          onClick={() => setShowAddStep(true)}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 6,
            padding: "8px 16px",
            borderRadius: 6,
            border: `1px dashed ${C.border}`,
            background: "transparent",
            color: "#64748b",
            fontSize: 12,
            fontWeight: 600,
            cursor: "pointer",
            fontFamily: "inherit",
            transition: "background .12s, color .12s, border-color .12s",
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = C.surfaceHover;
            e.currentTarget.style.color = C.text2;
            e.currentTarget.style.borderColor = C.borderHover;
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = "transparent";
            e.currentTarget.style.color = "#64748b";
            e.currentTarget.style.borderColor = C.border;
          }}
        >
          <Plus size={12} />
          Add Step
        </button>
      </div>

      <AddStepModal
        open={showAddStep}
        automationId={automationId}
        existingStepKeys={existingStepKeys}
        nextOrdinal={nextOrdinal}
        onClose={() => setShowAddStep(false)}
      />
    </div>
  );
}
