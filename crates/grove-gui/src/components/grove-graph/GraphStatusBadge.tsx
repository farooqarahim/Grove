const STATUS_COLORS: Record<string, { color: string; bg: string; bdr: string }> = {
  // Neutral
  open:        { color: "#8b8d98", bg: "rgba(255,255,255,0.04)", bdr: "rgba(255,255,255,0.08)" },
  idle:        { color: "#8b8d98", bg: "rgba(255,255,255,0.04)", bdr: "rgba(255,255,255,0.08)" },
  pending:     { color: "#5c5e6a", bg: "rgba(255,255,255,0.03)", bdr: "rgba(255,255,255,0.06)" },
  // Active
  inprogress:  { color: "#fb923c", bg: "rgba(251,146,60,0.08)",  bdr: "rgba(251,146,60,0.2)"  },
  running:     { color: "#fb923c", bg: "rgba(251,146,60,0.08)",  bdr: "rgba(251,146,60,0.2)"  },
  building:    { color: "#fb923c", bg: "rgba(251,146,60,0.08)",  bdr: "rgba(251,146,60,0.2)"  },
  queued:      { color: "#a78bfa", bg: "rgba(167,139,250,0.1)",  bdr: "rgba(167,139,250,0.2)" },
  // Fixing / error
  fixing:      { color: "#f87171", bg: "rgba(248,113,113,0.08)", bdr: "rgba(248,113,113,0.2)" },
  failed:      { color: "#f87171", bg: "rgba(248,113,113,0.08)", bdr: "rgba(248,113,113,0.2)" },
  aborted:     { color: "#f87171", bg: "rgba(248,113,113,0.08)", bdr: "rgba(248,113,113,0.2)" },
  error:       { color: "#f87171", bg: "rgba(248,113,113,0.08)", bdr: "rgba(248,113,113,0.2)" },
  // Success
  closed:      { color: "#3ecf8e", bg: "rgba(62,207,142,0.08)",  bdr: "rgba(62,207,142,0.2)"  },
  passed:      { color: "#3ecf8e", bg: "rgba(62,207,142,0.08)",  bdr: "rgba(62,207,142,0.2)"  },
  done:        { color: "#3ecf8e", bg: "rgba(62,207,142,0.08)",  bdr: "rgba(62,207,142,0.2)"  },
  complete:    { color: "#3ecf8e", bg: "rgba(62,207,142,0.08)",  bdr: "rgba(62,207,142,0.2)"  },
  // Paused / warning
  paused:      { color: "#f59e0b", bg: "rgba(245,158,11,0.1)",   bdr: "rgba(245,158,11,0.25)" },
  // Analysis states
  judging:     { color: "#a78bfa", bg: "rgba(167,139,250,0.1)",  bdr: "rgba(167,139,250,0.2)" },
  validating:  { color: "#60a5fa", bg: "rgba(96,165,250,0.08)",  bdr: "rgba(96,165,250,0.2)"  },
  verdict:     { color: "#fb923c", bg: "rgba(251,146,60,0.08)",  bdr: "rgba(251,146,60,0.2)"  },
  generating:  { color: "#a78bfa", bg: "rgba(167,139,250,0.1)",  bdr: "rgba(167,139,250,0.2)" },
  draft_ready: { color: "#60a5fa", bg: "rgba(96,165,250,0.08)",  bdr: "rgba(96,165,250,0.2)"  },
  planning:    { color: "#60a5fa", bg: "rgba(96,165,250,0.08)",  bdr: "rgba(96,165,250,0.2)"  },
  parsing:     { color: "#fb923c", bg: "rgba(251,146,60,0.08)",  bdr: "rgba(251,146,60,0.2)"  },
};

const DEFAULT_COLOR = { color: "#8b8d98", bg: "rgba(255,255,255,0.04)", bdr: "rgba(255,255,255,0.08)" };

function formatLabel(status: string): string {
  if (status === "inprogress") return "In Progress";
  if (status === "draft_ready") return "Draft Ready";
  return status.charAt(0).toUpperCase() + status.slice(1);
}

interface GraphStatusBadgeProps {
  status: string;
  size?: "sm" | "md";
}

export function GraphStatusBadge({ status, size = "sm" }: GraphStatusBadgeProps) {
  const col = STATUS_COLORS[status] ?? DEFAULT_COLOR;
  const fontSize = size === "sm" ? 10 : 11;
  const padding = size === "sm" ? "2px 6px" : "2px 8px";

  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        borderRadius: 4,
        fontWeight: 600,
        letterSpacing: "0.03em",
        whiteSpace: "nowrap",
        lineHeight: "16px",
        background: col.bg,
        color: col.color,
        border: `1px solid ${col.bdr}`,
        fontSize,
        padding,
      }}
    >
      {formatLabel(status)}
    </span>
  );
}
