import React from "react";
import { C } from "../../lib/theme";
import { GraphStatusBadge } from "./GraphStatusBadge";
import { GradeIndicator } from "./GradeIndicator";

interface MetadataCardProps {
  // Common fields
  taskObjective: string;
  status: string;
  outcome: string | null;
  aiComments: string | null;
  grade: number | null;
  agent: string | null;
  dependsOnJson: string; // JSON array of IDs
  // Step-specific
  runIteration?: number;
  maxIterations?: number;
  // Phase-specific
  ordinal?: number;
  // Display mode
  expanded?: boolean;
}

const labelStyle: React.CSSProperties = {
  fontSize: 10,
  fontWeight: 600,
  color: "rgba(255,255,255,0.38)",
  textTransform: "uppercase",
  letterSpacing: "0.06em",
  lineHeight: "14px",
  marginBottom: 2,
};

const valueStyle: React.CSSProperties = {
  fontSize: 12,
  color: "rgba(255,255,255,0.75)",
  lineHeight: "16px",
};

function parseDepsCount(dependsOnJson: string): number {
  try {
    const parsed = JSON.parse(dependsOnJson);
    if (Array.isArray(parsed)) return parsed.length;
    return 0;
  } catch {
    return 0;
  }
}

interface FieldProps {
  label: string;
  children: React.ReactNode;
  truncate?: boolean;
  expanded?: boolean;
}

function Field({ label, children, truncate = false, expanded = false }: FieldProps) {
  return (
    <div style={{ minWidth: 0 }}>
      <div style={labelStyle}>{label}</div>
      <div
        style={{
          ...valueStyle,
          overflow: truncate && !expanded ? "hidden" : undefined,
          textOverflow: truncate && !expanded ? "ellipsis" : undefined,
          whiteSpace: truncate && !expanded ? "nowrap" : "pre-wrap",
          wordBreak: "break-word",
        }}
      >
        {children}
      </div>
    </div>
  );
}

export function MetadataCard({
  taskObjective,
  status,
  outcome,
  aiComments,
  grade,
  agent,
  dependsOnJson,
  runIteration,
  maxIterations,
  ordinal,
  expanded = false,
}: MetadataCardProps) {
  const depsCount = parseDepsCount(dependsOnJson);

  const runLabel =
    runIteration !== undefined && maxIterations !== undefined
      ? `${runIteration} / ${maxIterations}`
      : ordinal !== undefined
      ? `Phase ${ordinal}`
      : "—";

  return (
    <div
      style={{
        background: C.surfaceHover,
        border: `1px solid ${C.border}`,
        borderRadius: 8,
        padding: expanded ? "12px 14px" : "8px 12px",
        display: "grid",
        gridTemplateColumns: "1fr 1fr",
        gap: expanded ? "10px 16px" : "6px 12px",
      }}
    >
      <Field label="Run">{runLabel}</Field>

      <Field label="Status">
        <GraphStatusBadge status={status} size="sm" />
      </Field>

      <Field label="DAG">
        {depsCount === 0 ? "No deps" : `${depsCount} dep${depsCount !== 1 ? "s" : ""}`}
      </Field>

      <Field label="Grade">
        <GradeIndicator grade={grade} size="sm" />
      </Field>

      <Field label="Objectives" truncate expanded={expanded}>
        {taskObjective || "—"}
      </Field>

      <Field label="Agent" truncate expanded={expanded}>
        {agent || "—"}
      </Field>

      <Field label="AI Comments" truncate expanded={expanded}>
        {aiComments || "—"}
      </Field>

      <Field label="Outcome" truncate expanded={expanded}>
        {outcome || "—"}
      </Field>
    </div>
  );
}
