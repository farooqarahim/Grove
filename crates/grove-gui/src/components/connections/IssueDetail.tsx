import { C } from "@/lib/theme";
import type { Issue } from "@/types";

interface IssueDetailProps {
  issue: Issue;
}

export function IssueDetail({ issue }: IssueDetailProps) {
  return (
    <div style={{
      padding: "10px 14px", borderRadius: 6,
      background: "rgba(99,102,241,0.06)",


    }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
        <span style={{
          fontSize: 11, fontWeight: 700, color: C.accent,
          fontFamily: C.mono,
        }}>
          #{issue.external_id}
        </span>
        <span style={{ fontSize: 12, fontWeight: 600, color: C.text1, flex: 1 }}>
          {issue.title}
        </span>
      </div>

      <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginBottom: 6 }}>
        <span style={{
          fontSize: 9, padding: "2px 6px", borderRadius: 6,
          background: "rgba(99,102,241,0.15)", color: "#818CF8",
          fontWeight: 600,
        }}>
          {issue.provider}
        </span>
        <span style={{
          fontSize: 9, padding: "2px 6px", borderRadius: 6,
          background: issue.status === "open" ? "rgba(49,185,123,0.15)" : "rgba(156,163,175,0.15)",
          color: issue.status === "open" ? "#31B97B" : C.text4,
          fontWeight: 600,
        }}>
          {issue.status}
        </span>
        {issue.assignee && (
          <span style={{
            fontSize: 9, padding: "2px 6px", borderRadius: 6,
            background: "rgba(255,255,255,0.04)", color: C.text3,
          }}>
            {issue.assignee}
          </span>
        )}
        {issue.labels.map(label => (
          <span key={label} style={{
            fontSize: 9, padding: "2px 6px", borderRadius: 6,
            background: "rgba(245,158,11,0.15)", color: "#F59E0B",
          }}>
            {label}
          </span>
        ))}
      </div>

      {issue.url && (
        <div style={{ fontSize: 10, color: C.text4, marginBottom: 4 }}>
          <span style={{ fontFamily: C.mono, wordBreak: "break-all" }}>{issue.url}</span>
        </div>
      )}

      {issue.body && (
        <div style={{
          marginTop: 6, padding: "8px 10px", borderRadius: 6,
          background: "rgba(0,0,0,0.2)",
          fontSize: 11, color: C.text3, lineHeight: 1.5,
          maxHeight: 120, overflowY: "auto",
          whiteSpace: "pre-wrap", wordBreak: "break-word",
        }}>
          {issue.body}
        </div>
      )}
    </div>
  );
}
