import { statusColor, StatusIcon } from "@/components/ui/icons";
import { formatRunAgentLabel } from "@/lib/runLabels";
import { C } from "@/lib/theme";
import type { SessionRecord } from "@/types";

interface PipelineVizProps {
  sessions: SessionRecord[];
  pipeline?: string | null;
}

export function PipelineViz({ sessions, pipeline }: PipelineVizProps) {
  if (sessions.length === 0) return null;

  return (
    <div className="flex items-center gap-1 flex-wrap">
      {sessions.map((s, i) => {
        const sc = statusColor(s.state);
        return (
          <div key={s.id} className="flex items-center gap-1">
            <div
              className="inline-flex items-center gap-1 text-sm font-medium rounded"
              style={{
                padding: "3px 10px",
                background: sc.bg,
                color: sc.text,
                lineHeight: "18px",
              }}
            >
              <StatusIcon status={s.state} size={8} />
              {formatRunAgentLabel(s.agent_type, pipeline)}
            </div>
            {i < sessions.length - 1 && (
              <span className="text-2xs" style={{ color: C.text4, opacity: 0.5 }}>{"\u2192"}</span>
            )}
          </div>
        );
      })}
    </div>
  );
}
