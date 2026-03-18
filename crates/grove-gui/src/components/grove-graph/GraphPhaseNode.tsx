import React from "react";
import { GraphPhaseRecord, GraphStepRecord } from "../../types";
import { GraphStatusBadge } from "./GraphStatusBadge";
import { GradeIndicator } from "./GradeIndicator";
import { GraphStepNode } from "./GraphStepNode";

const MONO = "'JetBrains Mono', 'Fira Code', 'SF Mono', monospace";

function PhaseProgressMini({ done, total }: { done: number; total: number }) {
  const pct = total > 0 ? (done / total) * 100 : 0;
  const barColor =
    done === total && total > 0 ? "#3ecf8e" : done === 0 ? "#5c5e6a" : "#fb923c";

  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
      <div
        style={{
          width: 48,
          height: 3,
          borderRadius: 2,
          background: "rgba(255,255,255,0.06)",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            width: `${pct}%`,
            height: "100%",
            borderRadius: 2,
            background: barColor,
            transition: "width 0.5s cubic-bezier(0.16,1,0.3,1)",
          }}
        />
      </div>
      <span
        style={{
          fontFamily: MONO,
          fontSize: 11,
          color: "#5c5e6a",
          minWidth: 24,
          fontVariantNumeric: "tabular-nums",
        }}
      >
        {done}/{total}
      </span>
    </div>
  );
}

interface GraphPhaseNodeProps {
  phase: GraphPhaseRecord;
  steps: GraphStepRecord[];
  isExpanded: boolean;
  onToggle: () => void;
  onStepClick: (stepId: string) => void;
  onPhaseClick: () => void;
}

export function GraphPhaseNode({
  phase,
  steps,
  isExpanded,
  onToggle,
  onStepClick,
  onPhaseClick,
}: GraphPhaseNodeProps) {
  const [headerHovered, setHeaderHovered] = React.useState(false);

  const sortedSteps = [...steps].sort((a, b) => a.ordinal - b.ordinal);
  const closedSteps = steps.filter((s) => s.status === "closed").length;
  const allDone = closedSteps === steps.length && steps.length > 0;
  const isActive = phase.status === "inprogress";
  const headerColor = allDone ? "#3ecf8e" : isActive ? "#e2e4e9" : "#8b8d98";

  const statusTags: string[] = [phase.status];
  if (
    phase.validation_status &&
    phase.validation_status !== "pending" &&
    (phase.validation_status as string) !== phase.status
  ) {
    statusTags.push(phase.validation_status);
  }

  return (
    <div style={{ marginBottom: 2 }}>
      {/* Phase header button */}
      <button
        onClick={onToggle}
        onMouseEnter={() => setHeaderHovered(true)}
        onMouseLeave={() => setHeaderHovered(false)}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          width: "100%",
          padding: "10px 12px",
          background: headerHovered ? "rgba(255,255,255,0.03)" : "transparent",
          border: "none",
          borderRadius: 8,
          cursor: "pointer",
          transition: "background 0.15s",
          outline: "none",
          userSelect: "none",
        }}
      >
        {/* Chevron */}
        <div style={{ color: "#5c5e6a", flexShrink: 0 }}>
          <svg
            width={14}
            height={14}
            viewBox="0 0 14 14"
            fill="none"
            stroke="currentColor"
            strokeWidth={1.8}
            strokeLinecap="round"
            strokeLinejoin="round"
            style={{
              transform: isExpanded ? "rotate(0deg)" : "rotate(-90deg)",
              transition: "transform 0.15s ease",
            }}
          >
            <polyline points="2,4 7,9 12,4" />
          </svg>
        </div>

        {/* P{ordinal} badge */}
        <span
          style={{
            fontFamily: MONO,
            fontSize: 11,
            fontWeight: 600,
            color: allDone ? "#3ecf8e" : "#5c5e6a",
            letterSpacing: "0.04em",
            flexShrink: 0,
          }}
        >
          P{phase.ordinal}
        </span>

        {/* Phase name — click opens detail drawer */}
        <span
          role="button"
          tabIndex={0}
          onClick={(e) => {
            e.stopPropagation();
            onPhaseClick();
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.stopPropagation();
              onPhaseClick();
            }
          }}
          style={{
            fontSize: 13.5,
            fontWeight: 500,
            color: headerColor,
            flex: 1,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            textAlign: "left",
            outline: "none",
          }}
        >
          {phase.task_name}
        </span>

        {/* Right: progress mini + status tags + grade */}
        <div
          style={{
            marginLeft: "auto",
            display: "flex",
            alignItems: "center",
            gap: 8,
            flexShrink: 0,
          }}
          onClick={(e) => e.stopPropagation()}
        >
          <PhaseProgressMini done={closedSteps} total={steps.length} />
          {statusTags.map((s, i) => (
            <GraphStatusBadge key={i} status={s} size="sm" />
          ))}
          {phase.grade !== null && <GradeIndicator grade={phase.grade} size="sm" />}
        </div>
      </button>

      {/* Steps (indented) */}
      {isExpanded && (
        <div style={{ padding: "0 0 6px 18px" }}>
          {sortedSteps.length === 0 ? (
            <div
              style={{
                padding: "8px 16px",
                fontSize: 11,
                color: "#5c5e6a",
                fontStyle: "italic",
              }}
            >
              No steps
            </div>
          ) : (
            sortedSteps.map((step) => (
              <GraphStepNode
                key={step.id}
                step={step}
                onClick={() => onStepClick(step.id)}
              />
            ))
          )}
        </div>
      )}
    </div>
  );
}
