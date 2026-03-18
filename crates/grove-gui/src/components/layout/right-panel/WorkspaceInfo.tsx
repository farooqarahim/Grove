import { ArrowDown, ArrowUp, GitBranch } from "@/components/ui/icons";
import { C } from "@/lib/theme";
import type { BranchStatus, GitLogEntry } from "@/types";

interface WorkspaceInfoProps {
  branchStatus: BranchStatus | null;
  latestCommit: GitLogEntry | null;
  syncSummary: string;
  branchNeedsSync: boolean;
  pullError: string | null;
  undoError: string | null;
}

export function WorkspaceInfo({
  branchStatus,
  latestCommit,
  syncSummary,
  branchNeedsSync,
  pullError,
  undoError,
}: WorkspaceInfoProps) {
  return (
    <div style={{ padding: 16 }}>
      <div style={{ padding: 0 }}>
        <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: 12 }}>
          <div style={{ minWidth: 0, flex: 1 }}>
            <div style={{ fontSize: 10, color: C.text4, textTransform: "uppercase", letterSpacing: "0.06em", fontWeight: 600 }}>
              Workspace
            </div>
            <div style={{
              marginTop: 6,
              display: "flex",
              alignItems: "center",
              gap: 6,
              fontSize: 13,
              fontWeight: 700,
              color: C.text1,
              minWidth: 0,
            }}>
              <span style={{ color: C.accent }}><GitBranch size={13} /></span>
              <span style={{ fontFamily: C.mono, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {branchStatus?.branch ?? "\u2014"}
              </span>
            </div>
          </div>

          <div style={{ textAlign: "right", flexShrink: 0 }}>
            <div style={{ fontSize: 10, color: C.text4, textTransform: "uppercase", letterSpacing: "0.06em", fontWeight: 600 }}>
              Sync
            </div>
            <div style={{ marginTop: 6, fontSize: 12, fontWeight: 700, color: branchNeedsSync ? C.accent : C.text1 }}>
              {syncSummary}
            </div>
            <div style={{ marginTop: 4, display: "flex", justifyContent: "flex-end", gap: 8, fontSize: 10 }}>
              {branchStatus && branchStatus.ahead > 0 && (
                <span style={{ color: C.accent, display: "inline-flex", alignItems: "center", gap: 3 }}>
                  <ArrowUp size={10} /> {branchStatus.ahead}
                </span>
              )}
              {branchStatus && branchStatus.behind > 0 && (
                <span style={{ color: C.warn, display: "inline-flex", alignItems: "center", gap: 3 }}>
                  <ArrowDown size={10} /> {branchStatus.behind}
                </span>
              )}
            </div>
          </div>
        </div>

        <div style={{
          marginTop: 8,
          fontSize: 11,
          color: C.text1,
          lineHeight: 1.55,
          display: "-webkit-box",
          WebkitLineClamp: 2,
          WebkitBoxOrient: "vertical",
          overflow: "hidden",
        }}>
          {latestCommit?.subject ?? "No recent commit available"}
        </div>
        {latestCommit && (
          <div style={{ marginTop: 4, fontSize: 10, color: C.text3, fontFamily: C.mono }}>
            {latestCommit.hash.slice(0, 7)}
            {latestCommit.is_pushed ? " \u2022 pushed" : " \u2022 local commit"}
          </div>
        )}

        {(pullError || undoError || branchStatus?.remote_error) && (
          <div style={{ marginTop: 8, fontSize: 10, color: "#EF4444", lineHeight: 1.5 }}>
            {pullError ?? undoError ?? branchStatus?.remote_error}
          </div>
        )}
      </div>
    </div>
  );
}
