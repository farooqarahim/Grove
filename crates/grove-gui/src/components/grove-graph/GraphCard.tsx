import { C } from "@/lib/theme";
import { GraphStatusBadge } from "./GraphStatusBadge";
import { GraphProgressBar } from "./GraphProgressBar";

interface GraphCardProps {
  title: string;
  status: string;
  runtimeStatus: string;
  closedSteps: number;
  totalSteps: number;
  timestamp: string;
}

function formatTimestamp(ts: string): string {
  const d = new Date(ts);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function GraphCard({
  title,
  status,
  runtimeStatus,
  closedSteps,
  totalSteps,
  timestamp,
}: GraphCardProps) {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 8,
        padding: "12px 16px",
        borderRadius: 6,
        background: C.surface,
        border: `1px solid ${C.border}`,
      }}
    >
      {/* Header row */}
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        {/* Icon */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: 32,
            height: 32,
            borderRadius: 6,
            background: "rgba(129,140,248,0.10)",
            color: "#818CF8",
            flexShrink: 0,
          }}
        >
          <svg
            width={14}
            height={14}
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            strokeWidth={1.8}
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <circle cx={8} cy={4} r={2} />
            <circle cx={2} cy={12} r={2} />
            <circle cx={14} cy={12} r={2} />
            <line x1={8} y1={6} x2={8} y2={10} />
            <line x1={8} y1={10} x2={3} y2={10} />
            <line x1={8} y1={10} x2={13} y2={10} />
            <line x1={3} y1={10} x2={3} y2={12} />
            <line x1={13} y1={10} x2={13} y2={12} />
          </svg>
        </div>
        {/* Title + badges */}
        <div style={{ flex: 1, minWidth: 0 }}>
          <div
            style={{
              fontSize: 12,
              fontWeight: 600,
              color: C.text1,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              marginBottom: 3,
            }}
          >
            {title}
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <GraphStatusBadge status={status} size="sm" />
            {runtimeStatus !== "idle" && (
              <GraphStatusBadge status={runtimeStatus} size="sm" />
            )}
          </div>
        </div>
        {/* Timestamp */}
        <span style={{ fontSize: 10, color: C.text4, flexShrink: 0 }}>
          {formatTimestamp(timestamp)}
        </span>
      </div>

      {/* Progress bar */}
      {totalSteps > 0 && (
        <GraphProgressBar closedSteps={closedSteps} totalSteps={totalSteps} height={3} />
      )}
    </div>
  );
}
