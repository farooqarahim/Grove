import { memo } from "react";
import { Tag } from "@/components/ui/badge";
import { Check, ChevronR, Pulse, statusColor, StatusIcon } from "@/components/ui/icons";
import { formatDuration } from "@/lib/hooks";
import { C, lbl } from "@/lib/theme";
import type { PlanStep, SessionRecord } from "@/types";

const ACTIVE_AGENT_STATES = ["executing", "running", "planning", "verifying"];

interface AgentDetailProps {
  session: SessionRecord;
  planStep: PlanStep | undefined;
  isExpanded: boolean;
  onToggle: () => void;
}

export const AgentDetail = memo(function AgentDetail({ session, planStep, isExpanded, onToggle }: AgentDetailProps) {
  const sc = statusColor(session.state);
  const duration = formatDuration(session.started_at, session.ended_at);
  const isDone = session.state === "completed";
  const completedCount = isDone ? (planStep?.todos.length ?? 0) : 0;
  const totalCount = planStep?.todos.length ?? 0;
  const isActiveAgent = ACTIVE_AGENT_STATES.includes(session.state);
  const isStalled = isActiveAgent && !!session.stalled_since;

  return (
    <div
      onClick={(e) => { e.stopPropagation(); onToggle(); }}
      className="cursor-pointer transition-all overflow-hidden"
      style={{
        borderRadius: 6,
        background: isExpanded ? "rgba(40, 38, 84, 0.5)" : "rgba(255, 255, 255, 0.06)",
      }}
    >
      {/* Agent header */}
      <div className="flex items-center justify-between" style={{ padding: "11px 14px" }}>
        <div className="flex items-center gap-2">
          <div
            className="flex items-center justify-center"
            style={{
              width: 26, height: 26, borderRadius: 6,
              background: sc.bg,
            }}
          >
            <span style={{ color: sc.text }}><StatusIcon status={session.state} size={10} /></span>
          </div>
          <span className="text-base font-semibold" style={{ color: C.text1 }}>{session.agent_type}</span>
          {isActiveAgent && !isStalled && (
            <Pulse color="#31B97B" size={6} />
          )}
          {isStalled && (
            <span
              title={`Stalled since ${session.stalled_since}`}
              style={{
                fontSize: 9, fontWeight: 600, padding: "1px 5px",
                borderRadius: 2, background: "rgba(239,68,68,0.12)",
                color: "#EF4444", textTransform: "uppercase",
              }}
            >
              stalled
            </span>
          )}
          {planStep?.title && <Tag>{planStep.title}</Tag>}
        </div>
        <div className="flex items-center gap-3">
          <span className="text-xs font-mono" style={{ color: C.text4 }}>{duration}</span>
          <span className="transition-transform" style={{ color: C.text4, transform: isExpanded ? "rotate(90deg)" : "" }}>
            <ChevronR size={10} />
          </span>
        </div>
      </div>

      {/* Agent detail */}
      {isExpanded && (
        <div className="animate-fade-in" style={{ padding: "0 14px 14px" }}>
          {/* Summary */}
          {(planStep?.result_summary || planStep?.description) && (
            <div
              className="mb-2"
              style={{
                borderRadius: 6, background: "rgba(255,255,255,0.02)",
                padding: "10px 14px",
              }}
            >
              <div style={{ ...lbl, marginBottom: 4 }}>Summary</div>
              <p className="m-0 text-sm leading-relaxed" style={{ color: "#DDE0E7" }}>
                {planStep.result_summary || planStep.description}
              </p>
            </div>
          )}

          {/* Todos */}
          {totalCount > 0 && planStep && (
            <div
              style={{
                borderRadius: 6, background: "rgba(255,255,255,0.02)",
                padding: "10px 14px",
              }}
            >
              <div className="flex justify-between mb-1.5">
                <span style={lbl}>Tasks</span>
                <span className="text-xs font-mono" style={{ color: C.text4 }}>
                  {completedCount}/{totalCount}
                </span>
              </div>
              {/* Progress bar */}
              <div className="overflow-hidden mb-3" style={{ height: 2, borderRadius: 2, background: "rgba(255,255,255,0.04)" }}>
                <div
                  className="transition-all"
                  style={{
                    height: "100%", borderRadius: 2, background: C.accent, opacity: 0.6,
                    transitionDuration: "500ms",
                    width: totalCount > 0 ? `${(completedCount / totalCount) * 100}%` : "0%",
                  }}
                />
              </div>
              {planStep.todos.map((todo, ti) => {
                const done = isDone;
                const current = !done && ["executing", "running", "planning", "verifying"].includes(session.state) && ti === 0;
                return (
                  <div key={ti} className="flex items-center gap-2" style={{ padding: "4px 0" }}>
                    <div
                      className="flex items-center justify-center shrink-0 transition-all"
                      style={{
                        width: 16, height: 16, borderRadius: 4,
                        outline: done ? `1.5px solid ${C.accent}60` : current ? "1.5px solid #3B82F640" : "none",
                        background: done ? `${C.accent}12` : current ? "rgba(59,130,246,0.08)" : "transparent",
                        ...(current ? { animation: "pulse 2s infinite" } : {}),
                      }}
                    >
                      {done && <Check size={8} />}
                    </div>
                    <span
                      className="text-sm transition-colors"
                      style={{
                        color: done ? C.text4 : C.text2,
                        textDecoration: done ? "line-through" : "none",
                      }}
                    >{todo}</span>
                  </div>
                );
              })}
            </div>
          )}

          {/* Agent micro stats */}
          {planStep?.files && (
            <div className="flex gap-3.5 text-xs pt-2" style={{ color: C.text4 }}>
              <span>Files <b className="font-medium" style={{ color: C.text3 }}>{planStep.files.length}</b></span>
            </div>
          )}
        </div>
      )}
    </div>
  );
});
