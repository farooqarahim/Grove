import { C } from "@/lib/theme";
import type { PhaseCheckpointDto } from "@/lib/api";

interface Agent {
  id: string;
  label: string;
}

const AGENTS: Agent[] = [
  { id: "build_prd", label: "PRD" },
  { id: "plan_system_design", label: "Design" },
  { id: "builder", label: "Build" },
  { id: "reviewer", label: "Review" },
  { id: "judge", label: "Judge" },
];

interface PhaseProgressBarProps {
  /** Which pipeline agents are in this run */
  pipelineAgents: string[];
  /** Current agent being executed (or null if done) */
  currentAgent: string | null;
  /** Phase checkpoints with gate decisions */
  checkpoints: PhaseCheckpointDto[];
}

function agentStatus(
  agentId: string,
  currentAgent: string | null,
  checkpoints: PhaseCheckpointDto[],
): "completed" | "active" | "gate_pending" | "upcoming" {
  const cp = checkpoints.find((c) => c.agent === agentId);

  if (cp) {
    if (cp.status === "pending") return "gate_pending";
    if (cp.status === "approved" || cp.status === "skipped") return "completed";
    if (cp.status === "rejected") return "completed"; // rejected but still completed
  }

  if (currentAgent === agentId) return "active";

  // If current agent is after this one in the pipeline, it's completed
  const agents = AGENTS.map((a) => a.id);
  const currentIdx = currentAgent ? agents.indexOf(currentAgent) : -1;
  const thisIdx = agents.indexOf(agentId);
  if (currentIdx > thisIdx) return "completed";

  return "upcoming";
}

function statusColor(status: ReturnType<typeof agentStatus>) {
  switch (status) {
    case "completed":
      return { bg: "#22543d", text: "#68d391", border: "#2f855a" };
    case "active":
      return { bg: "#2b6cb0", text: "#90cdf4", border: "#3182ce" };
    case "gate_pending":
      return { bg: "#744210", text: "#fbd38d", border: "#d69e2e" };
    case "upcoming":
      return { bg: C.surface, text: C.text4, border: C.border };
  }
}

export function PhaseProgressBar({ pipelineAgents, currentAgent, checkpoints }: PhaseProgressBarProps) {
  const visibleAgents = AGENTS.filter((a) => pipelineAgents.includes(a.id));

  return (
    <div style={{ display: "flex", alignItems: "center", gap: 2 }}>
      {visibleAgents.map((agent, i) => {
        const status = agentStatus(agent.id, currentAgent, checkpoints);
        const colors = statusColor(status);
        return (
          <div key={agent.id} style={{ display: "flex", alignItems: "center", gap: 2 }}>
            <div
              style={{
                padding: "3px 10px",
                fontSize: 11,
                fontWeight: status === "active" ? 600 : 500,
                color: colors.text,
                background: colors.bg,
                border: `1px solid ${colors.border}`,
                borderRadius: 5,
                display: "flex",
                alignItems: "center",
                gap: 4,
              }}
            >
              {status === "completed" && <span style={{ fontSize: 9 }}>&#10003;</span>}
              {status === "active" && <span style={{ fontSize: 9, animation: "pulse 1.5s infinite" }}>&#9679;</span>}
              {status === "gate_pending" && <span style={{ fontSize: 9 }}>&#9632;</span>}
              {agent.label}
            </div>
            {i < visibleAgents.length - 1 && (
              <span style={{ color: C.text4, opacity: 0.4, fontSize: 10 }}>{"\u2192"}</span>
            )}
          </div>
        );
      })}
    </div>
  );
}
