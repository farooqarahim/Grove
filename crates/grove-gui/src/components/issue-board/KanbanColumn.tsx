import type { Issue } from "@/types";
import type { DisplayColumn } from "./constants";
import { compositeId } from "./helpers";
import { IssueCard } from "./IssueCard";
import { PlusIcon } from "./Icons";

interface KanbanColumnProps {
  column: DisplayColumn;
  selectedIssueId: string | null;
  onSelectIssue: (issue: Issue) => void;
  onAddClick: () => void;
}

export function KanbanColumn({ column, selectedIssueId, onSelectIssue, onAddClick }: KanbanColumnProps) {
  return (
    <div
      style={{
        minWidth: 260, maxWidth: 320, flex: "1 1 260px",
        display: "flex", flexDirection: "column", height: "100%",
        background: `linear-gradient(180deg, ${column.accent}09 0%, rgba(2,6,23,0) 22%)`,
        border: "1px solid rgba(51,65,85,0.12)",
        borderRadius: 18,
        padding: "12px 12px 0",
        boxShadow: "inset 0 1px 0 rgba(255,255,255,0.02)",
      }}
    >
      {/* Header */}
      <div style={{
        display: "flex", alignItems: "center", gap: 8, padding: "2px 4px 14px",
        borderBottom: `1px solid ${column.accent}2a`,
      }}>
        <div style={{
          width: 9, height: 9, borderRadius: "50%", background: column.accent,
          boxShadow: `0 0 12px ${column.accent}55`, flexShrink: 0,
        }} />
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={{ fontSize: 13, fontWeight: 650, color: "#dbe7fb", letterSpacing: "-0.015em", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
            {column.title}
          </div>
          {column.subtitle && (
            <div style={{ marginTop: 3, fontSize: 10.5, color: "#64748b", fontWeight: 600, letterSpacing: "0.04em", textTransform: "uppercase" }}>
              {column.subtitle}
            </div>
          )}
        </div>
        <span style={{
          fontSize: 11, fontWeight: 700, color: "#93a6c6", background: "rgba(15,23,42,0.55)",
          border: "1px solid rgba(71,85,105,0.25)", borderRadius: 999, padding: "0 8px", minWidth: 24, textAlign: "center", lineHeight: "22px",
        }}>{column.count}</span>
        <button
          onClick={onAddClick}
          className="ib-add-btn"
          style={{
            background: "rgba(15,23,42,0.55)", border: "1px solid rgba(51,65,85,0.25)", color: "#64748b",
            cursor: "pointer", padding: 6, borderRadius: 8, display: "flex", transition: "color .15s",
          }}
        >
          <PlusIcon />
        </button>
      </div>

      {/* Cards */}
      <div style={{ flex: 1, overflowY: "auto", padding: "10px 0", display: "flex", flexDirection: "column", gap: 6 }}>
        {column.issues.length === 0 ? (
          <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", flexDirection: "column", gap: 8, padding: "32px 12px 40px" }}>
            <div style={{
              width: 42, height: 42, borderRadius: 14,
              border: `1.5px dashed ${column.accent}33`,
              display: "flex", alignItems: "center", justifyContent: "center",
              color: column.accent, fontSize: 18,
              background: `${column.accent}10`,
            }}>
              <PlusIcon />
            </div>
            <span style={{ color: "#475569", fontSize: 12, fontWeight: 600, textAlign: "center" }}>
              No issues in this lane
            </span>
          </div>
        ) : (
          column.issues.map(issue => (
            <IssueCard
              key={compositeId(issue)}
              issue={issue}
              isSelected={compositeId(issue) === selectedIssueId}
              onClick={() => onSelectIssue(issue)}
            />
          ))
        )}
      </div>
    </div>
  );
}
