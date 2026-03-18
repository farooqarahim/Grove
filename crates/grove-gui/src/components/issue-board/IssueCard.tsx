import type { Issue } from "@/types";
import { LABEL_COLORS, PRIORITY_CONFIG } from "./constants";
import { compositeId, displayProvider, formatRelative, normalizePriority } from "./helpers";

interface IssueCardProps {
  issue: Issue;
  isSelected: boolean;
  onClick: () => void;
}

export function IssueCard({ issue, isSelected, onClick }: IssueCardProps) {
  const cid = compositeId(issue);
  const displayPriority = normalizePriority(issue.priority);
  const pc = PRIORITY_CONFIG[displayPriority] ?? PRIORITY_CONFIG.None;
  const initials = issue.assignee ? issue.assignee.slice(0, 2).toUpperCase() : null;
  const created = issue.created_at ? formatRelative(issue.created_at) : "";

  return (
    <div
      onClick={onClick}
      className="ib-card"
      style={{
        background: isSelected ? "rgba(15,23,42,0.9)" : "rgba(15,23,42,0.6)",
        border: isSelected ? "1px solid rgba(71,85,105,0.4)" : "1px solid rgba(51,65,85,0.25)",
        borderRadius: 10,
        padding: "12px 14px",
        cursor: "pointer",
        transition: "all .2s ease",
        display: "flex",
        gap: 10,
        alignItems: "flex-start",
        userSelect: "none",
        transform: isSelected ? "translateY(-1px)" : undefined,
      }}
    >
      <div style={{ flex: 1, minWidth: 0 }}>
        {/* ID + priority badge */}
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
          <span style={{ fontSize: 11, fontWeight: 600, color: "#475569", fontFamily: "monospace" }}>
            {issue.external_id || cid}
          </span>
          {displayPriority !== "None" && (
            <span style={{
              fontSize: 10, fontWeight: 700, padding: "1px 6px", borderRadius: 4,
              background: pc.bg, color: pc.color, border: `1px solid ${pc.border}`,
              letterSpacing: "0.03em",
            }}>
              {displayPriority.toUpperCase()}
            </span>
          )}
        </div>
        {/* Title */}
        <div style={{ fontSize: 13, fontWeight: 500, color: "#e2e8f0", lineHeight: 1.4, marginBottom: 8 }}>
          {issue.title}
        </div>
        {/* Labels + created */}
        <div style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>
          {issue.labels.slice(0, 3).map(l => {
            const lc = LABEL_COLORS[l] ?? { color: "#94a3b8", bg: "rgba(148,163,184,0.08)" };
            return (
              <span key={l} style={{ fontSize: 10.5, padding: "1px 7px", borderRadius: 4, background: lc.bg, color: lc.color, fontWeight: 500 }}>
                {l}
              </span>
            );
          })}
          {created && (
            <span style={{ marginLeft: "auto", fontSize: 11, color: "#334155" }}>{created}</span>
          )}
        </div>
        {/* Assignee + source */}
        <div style={{ display: "flex", alignItems: "center", gap: 6, marginTop: 8 }}>
          {initials ? (
            <div style={{
              width: 20, height: 20, borderRadius: 6, fontSize: 9, fontWeight: 700,
              background: "rgba(99,102,241,0.15)", color: "#818cf8",
              display: "flex", alignItems: "center", justifyContent: "center", flexShrink: 0,
            }}>{initials}</div>
          ) : (
            <div style={{
              width: 20, height: 20, borderRadius: 6, border: "1px dashed rgba(71,85,105,0.4)",
              display: "flex", alignItems: "center", justifyContent: "center",
              fontSize: 10, color: "#334155", flexShrink: 0,
            }}>?</div>
          )}
          <span style={{
            fontSize: 10, padding: "1px 6px", borderRadius: 4,
            background: "rgba(51,65,85,0.2)", color: "#64748b", fontWeight: 500,
          }}>{displayProvider(issue.provider)}</span>
        </div>
      </div>
    </div>
  );
}
