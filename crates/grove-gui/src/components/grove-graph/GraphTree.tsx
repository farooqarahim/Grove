import { useState } from "react";
import { PhaseDetail } from "../../types";
import { GraphPhaseNode } from "./GraphPhaseNode";

interface GraphTreeProps {
  phases: PhaseDetail[];
  onStepClick: (stepId: string) => void;
  onPhaseClick: (phaseId: string) => void;
}

export function GraphTree({ phases, onStepClick, onPhaseClick }: GraphTreeProps) {
  const sortedPhases = [...phases].sort((a, b) => a.phase.ordinal - b.phase.ordinal);

  const [expandedIds, setExpandedIds] = useState<Set<string>>(() => {
    const ids = new Set<string>();
    for (const pd of phases) {
      ids.add(pd.phase.id);
    }
    return ids;
  });

  function togglePhase(phaseId: string) {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(phaseId)) {
        next.delete(phaseId);
      } else {
        next.add(phaseId);
      }
      return next;
    });
  }

  if (sortedPhases.length === 0) {
    return (
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: "100%",
          minHeight: 120,
          color: "#5c5e6a",
          fontSize: 13,
          fontStyle: "italic",
        }}
      >
        No phases yet
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", padding: "4px 0" }}>
      {sortedPhases.map((pd) => (
        <GraphPhaseNode
          key={pd.phase.id}
          phase={pd.phase}
          steps={pd.steps}
          isExpanded={expandedIds.has(pd.phase.id)}
          onToggle={() => togglePhase(pd.phase.id)}
          onStepClick={onStepClick}
          onPhaseClick={() => onPhaseClick(pd.phase.id)}
        />
      ))}
    </div>
  );
}
