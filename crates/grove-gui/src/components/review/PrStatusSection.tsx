import { useState } from "react";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { PullRequest, Merge, ChevronDown, Loader } from "@/components/ui/icons";
import { C } from "@/lib/theme";
import { gitMergePr, gitProjectMergePr } from "@/lib/api";
import type { PrStatus } from "@/types";

const PANEL_BLOCK_BG = "#1C1F27";

async function openUrl(url: string) {
  try {
    await openExternal(url);
  } catch {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}

interface PrStatusSectionProps {
  prStatus: PrStatus;
  runId?: string;
  projectRoot?: string;
  onMerged?: () => void;
}

const STATE_BADGES: Record<string, { label: string; color: string; bg: string }> = {
  OPEN: { label: "Open", color: "#31B97B", bg: "rgba(49,185,123,0.10)" },
  CLOSED: { label: "Closed", color: "#EF4444", bg: "rgba(239,68,68,0.10)" },
  MERGED: { label: "Merged", color: "#A78BFA", bg: "rgba(167,139,250,0.10)" },
};

const MERGE_STATE_INFO: Record<string, { label: string; color: string }> = {
  CLEAN: { label: "Ready to merge", color: "#31B97B" },
  DIRTY: { label: "Merge conflicts", color: "#EF4444" },
  BLOCKED: { label: "Checks required", color: "#F59E0B" },
  BEHIND: { label: "Branch behind", color: "#F59E0B" },
  HAS_HOOKS: { label: "Waiting on hooks", color: "#F59E0B" },
  UNSTABLE: { label: "Unstable", color: "#EF4444" },
  UNKNOWN: { label: "Unknown", color: "#71767F" },
};

export function PrStatusSection({ prStatus, runId, projectRoot, onMerged }: PrStatusSectionProps) {
  const [strategy, setStrategy] = useState("squash");
  const [showStrategyMenu, setShowStrategyMenu] = useState(false);
  const [merging, setMerging] = useState(false);
  const [mergeError, setMergeError] = useState<string | null>(null);

  const stateBadge = STATE_BADGES[prStatus.state] ?? STATE_BADGES.OPEN;
  const mergeInfo = MERGE_STATE_INFO[prStatus.merge_state] ?? MERGE_STATE_INFO.UNKNOWN;
  const canMerge = prStatus.state === "OPEN" && prStatus.merge_state === "CLEAN";
  const isMerged = prStatus.state === "MERGED";

  const handleMerge = async () => {
    if (!runId && !projectRoot) return;
    setMerging(true);
    setMergeError(null);
    try {
      if (runId) {
        await gitMergePr(runId, strategy);
      } else {
        await gitProjectMergePr(projectRoot!, strategy);
      }
      onMerged?.();
    } catch (e) {
      setMergeError(e instanceof Error ? e.message : String(e));
    } finally {
      setMerging(false);
    }
  };

  const strategies: [string, string][] = [
    ["squash", "Squash and merge"],
    ["merge", "Create merge commit"],
    ["rebase", "Rebase and merge"],
  ];

  return (
    <div style={{
      padding: 12,
      borderRadius: 4,
      background: PANEL_BLOCK_BG,
    }}>
      <div style={{ display: "flex", alignItems: "flex-start", gap: 10, marginBottom: 10 }}>
        <div style={{
          width: 32,
          height: 32,
          borderRadius: 4,
          background: "rgba(59,130,246,0.14)",
          color: "#7DD3FC",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          flexShrink: 0,
        }}>
          <PullRequest size={14} />
        </div>
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 4, flexWrap: "wrap" }}>
            <span style={{ fontSize: 10, color: C.text3, textTransform: "uppercase", letterSpacing: "0.08em", fontWeight: 700 }}>
              PR Details
            </span>
            <span style={{
              fontSize: 9,
              fontWeight: 700,
              padding: "3px 7px",
              borderRadius: 4,
              background: stateBadge.bg,
              color: stateBadge.color,
              textTransform: "uppercase",
              letterSpacing: "0.04em",
            }}>
              {prStatus.is_draft ? "Draft" : stateBadge.label}
            </span>
            {!isMerged && (
              <span style={{
                fontSize: 9,
                fontWeight: 600,
                padding: "3px 7px",
                borderRadius: 4,
                background: "rgba(255,255,255,0.04)",
                color: mergeInfo.color,
              }}>
                {mergeInfo.label}
              </span>
            )}
          </div>
          <a
            href={prStatus.url}
            target="_blank"
            rel="noopener noreferrer"
            style={{
              display: "block",
              fontSize: 12,
              fontWeight: 600,
              color: C.text1,
              textDecoration: "none",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              marginBottom: 4,
            }}
          >
            #{prStatus.number} {prStatus.title}
          </a>
          <div style={{ display: "flex", gap: 6, flexWrap: "wrap", fontSize: 10 }}>
            <span style={{ color: "#31B97B" }}>+{prStatus.additions}</span>
            <span style={{ color: "#EF4444" }}>-{prStatus.deletions}</span>
            <span style={{ color: "#DDE0E7" }}>{prStatus.changed_files} changed files</span>
          </div>
        </div>
      </div>

      {!isMerged && prStatus.merge_state === "DIRTY" && (
        <div
          style={{
            fontSize: 10,
            color: "#EF4444",
            background: "rgba(239,68,68,0.08)",
            borderRadius: 4,
            padding: "8px 10px",
            marginBottom: 10,
          }}
        >
          <div style={{ marginBottom: prStatus.conflicting_files.length > 0 ? 4 : 0 }}>
            Merge conflict detected
            {prStatus.conflicting_files.length > 0
              ? ` in ${prStatus.conflicting_files.length} file(s).`
              : "."}{" "}
            Resolve it in the PR and retry merge from here.
          </div>
          {prStatus.conflicting_files.length > 0 && (
            <div style={{ color: "#F87171", lineHeight: 1.5 }}>
              {prStatus.conflicting_files.slice(0, 6).map((file) => (
                <div key={file}>{file}</div>
              ))}
              {prStatus.conflicting_files.length > 6 && (
                <div>+{prStatus.conflicting_files.length - 6} more</div>
              )}
            </div>
          )}
        </div>
      )}

      {!isMerged && (
        <div style={{ display: "flex", gap: 6, position: "relative" }}>
          <button
            onClick={() => { void openUrl(prStatus.url); }}
            style={{
              padding: "8px 11px",
              borderRadius: 4,
              border: "none",
              background: "rgba(59,130,246,0.10)",
              color: "#7DD3FC",
              fontSize: 11,
              fontWeight: 600,
              cursor: "pointer",
            }}
          >
            View PR
          </button>
          <button
            onClick={handleMerge}
            disabled={!canMerge || merging}
            style={{
              flex: 1,
              padding: "8px 12px",
              borderRadius: 4,
              border: "none",
              background: canMerge ? "rgba(49,185,123,0.12)" : "rgba(255,255,255,0.03)",
              color: canMerge ? C.accent : C.text4,
              fontSize: 11,
              fontWeight: 600,
              cursor: canMerge ? "pointer" : "default",
              opacity: merging ? 0.6 : 1,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              gap: 5,
            }}
          >
            {merging ? <Loader size={11} /> : <Merge size={11} />}
            {merging ? "Merging..." : strategies.find(([k]) => k === strategy)?.[1] ?? "Merge"}
          </button>
          <button
            onClick={() => setShowStrategyMenu(!showStrategyMenu)}
            style={{
              width: 32,
              borderRadius: 4,
              border: "none",
              background: "rgba(255,255,255,0.03)",
              color: "#DDE0E7",
              cursor: "pointer",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <ChevronDown size={9} />
          </button>

          {showStrategyMenu && (
            <div style={{
              position: "absolute",
              top: "calc(100% + 6px)",
              right: 0,
              background: C.surface,
              borderRadius: 4,
              overflow: "hidden",
              zIndex: 10,
              minWidth: 180,
            }}>
              {strategies.map(([key, label]) => (
                <button
                  key={key}
                  onClick={() => { setStrategy(key); setShowStrategyMenu(false); }}
                  className="hover-row"
                  style={{
                    width: "100%",
                    padding: "8px 10px",
                    background: strategy === key ? C.surfaceActive : "transparent",
                    border: "none",
                    color: C.text2,
                    fontSize: 11,
                    cursor: "pointer",
                    textAlign: "left",
                  }}
                >
                  {label}
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      {mergeError && (
        <div style={{ fontSize: 10, color: "#EF4444", marginTop: 6, textAlign: "center" }}>
          {mergeError}
        </div>
      )}
    </div>
  );
}
