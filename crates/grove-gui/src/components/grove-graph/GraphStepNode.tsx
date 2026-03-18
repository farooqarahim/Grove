import React from "react";
import { GraphStepRecord } from "../../types";
import { GradeIndicator } from "./GradeIndicator";
import { StepTypeBadge } from "./StepTypeBadge";
import { GraphStatusBadge } from "./GraphStatusBadge";

const MONO = "'JetBrains Mono', 'Fira Code', 'SF Mono', monospace";

function StepDot({ status }: { status: string }) {
  const isDone = status === "closed" || status === "passed" || status === "done";
  const isActive = status === "inprogress" || status === "building" || status === "running";
  const isFixing = status === "fixing" || status === "failed";

  if (isDone) {
    return (
      <div
        style={{
          width: 16,
          height: 16,
          borderRadius: "50%",
          background: "rgba(62,207,142,0.08)",
          border: "1.5px solid rgba(62,207,142,0.2)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexShrink: 0,
        }}
      >
        <svg
          width={9}
          height={9}
          viewBox="0 0 9 9"
          fill="none"
          stroke="#3ecf8e"
          strokeWidth={2.5}
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <polyline points="1.5,4.5 3.5,6.5 7.5,2.5" />
        </svg>
      </div>
    );
  }

  if (isActive || isFixing) {
    const c = isFixing ? "#f87171" : "#fb923c";
    return (
      <div
        style={{
          width: 16,
          height: 16,
          borderRadius: "50%",
          background: `${c}12`,
          border: `1.5px solid ${c}33`,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexShrink: 0,
        }}
      >
        <div
          className="graph-active-dot"
          style={{ width: 6, height: 6, borderRadius: "50%", background: c }}
        />
      </div>
    );
  }

  return (
    <div
      style={{
        width: 16,
        height: 16,
        borderRadius: "50%",
        border: "1.5px solid #222329",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        flexShrink: 0,
      }}
    >
      <div style={{ width: 4, height: 4, borderRadius: "50%", background: "#5c5e6a" }} />
    </div>
  );
}

interface GraphStepNodeProps {
  step: GraphStepRecord;
  onClick: () => void;
}

export function GraphStepNode({ step, onClick }: GraphStepNodeProps) {
  const [hovered, setHovered] = React.useState(false);
  const showIter = step.run_iteration > 0;

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") onClick(); }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        padding: "8px 12px 8px 16px",
        borderRadius: 6,
        cursor: "pointer",
        background: hovered ? "rgba(255,255,255,0.03)" : "transparent",
        transition: "background 0.15s",
        outline: "none",
      }}
    >
      <StepDot status={step.status} />

      <span
        style={{
          fontFamily: MONO,
          fontSize: 11,
          color: "#5c5e6a",
          minWidth: 14,
          textAlign: "right",
          flexShrink: 0,
        }}
      >
        {step.ordinal}
      </span>

      <span
        style={{
          fontSize: 13,
          color: "#e2e4e9",
          flex: 1,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {step.task_name}
      </span>

      <div style={{ display: "flex", alignItems: "center", gap: 5, flexShrink: 0 }}>
        <StepTypeBadge stepType={step.step_type} />
        <GraphStatusBadge status={step.status} size="sm" />
        {showIter && (
          <span
            style={{
              fontFamily: MONO,
              fontSize: 10,
              color: "#5c5e6a",
              whiteSpace: "nowrap",
            }}
          >
            Iter {step.run_iteration}/{step.max_iterations}
          </span>
        )}
        {step.grade !== null && <GradeIndicator grade={step.grade} size="sm" />}
      </div>
    </div>
  );
}
