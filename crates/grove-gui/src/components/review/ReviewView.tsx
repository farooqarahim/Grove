import { useState, useRef, useCallback, useEffect, useMemo } from "react";
import {
  XIcon, ChevronDown, Plus, Undo, Commit,
  Refresh, Copy, Search,
} from "@/components/ui/icons";
import { FileTree, buildFileTree } from "@/components/ui/FileTree";
import {
  gitRevertAll, gitStageAll, gitStageFiles, gitUnstageFiles,
  gitProjectStageFiles, gitProjectUnstageFiles, gitProjectStageAll, gitProjectRevertAll,
  type GitStatusEntry,
} from "@/lib/api";
import { C } from "@/lib/theme";
import type {
  FileDiffEntry,
  ReviewContext,
  ReviewInsights,
  ReviewFinding,
  ReviewFindingSeverity,
} from "@/types";
import { useQueryClient } from "@tanstack/react-query";
import { qk } from "@/lib/queryKeys";

// ── Types ────────────────────────────────────────────────────────────────────

interface DiffLine {
  type: "context" | "add" | "del" | "hunk";
  oldNum?: number;
  newNum?: number;
  text: string;
}

type AreaTab = "unstaged" | "staged" | "committed" | "all";
type RailTab = "files" | "ai";

interface ReviewViewProps {
  open: boolean;
  onClose: () => void;
  context: ReviewContext;
  gitStatus?: GitStatusEntry[];
  insights?: ReviewInsights | null;
  onCommit: () => void;
  onRefresh?: () => void;
  onSelectFile?: (path: string) => void;
  selectedFile: string | null;
}

// ── Diff parser ──────────────────────────────────────────────────────────────

function parseDiffLines(raw: string): DiffLine[] {
  const lines: DiffLine[] = [];
  let oldNum = 0;
  let newNum = 0;

  for (const line of raw.split("\n")) {
    if (line.startsWith("@@")) {
      const match = line.match(/@@ -(\d+)(?:,\d+)? \+(\d+)/);
      if (match) {
        oldNum = parseInt(match[1], 10);
        newNum = parseInt(match[2], 10);
      }
      lines.push({ type: "hunk", text: line });
    } else if (line.startsWith("+") && !line.startsWith("+++")) {
      lines.push({ type: "add", newNum, text: line.slice(1) });
      newNum++;
    } else if (line.startsWith("-") && !line.startsWith("---")) {
      lines.push({ type: "del", oldNum, text: line.slice(1) });
      oldNum++;
    } else if (!line.startsWith("diff ") && !line.startsWith("index ") && !line.startsWith("---") && !line.startsWith("+++")) {
      lines.push({ type: "context", oldNum, newNum, text: line });
      oldNum++;
      newNum++;
    }
  }
  return lines;
}

// ── Resize hook ──────────────────────────────────────────────────────────────

function useResize(init: number, min: number, max: number) {
  const [width, setWidth] = useState(init);
  const dragging = useRef(false);
  const startX = useRef(0);
  const startW = useRef(0);

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      dragging.current = true;
      startX.current = e.clientX;
      startW.current = width;
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
    },
    [width]
  );

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!dragging.current) return;
      const dx = startX.current - e.clientX;
      setWidth(Math.min(max, Math.max(min, startW.current + dx)));
    };
    const onUp = () => {
      dragging.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [min, max]);

  return [width, onMouseDown] as const;
}

// ── Severity helpers ─────────────────────────────────────────────────────────

const SEVERITY_ORDER: Record<ReviewFindingSeverity, number> = {
  critical: 0,
  major: 1,
  minor: 2,
  info: 3,
};

const SEVERITY_COLORS: Record<ReviewFindingSeverity, { bg: string; text: string; border: string }> = {
  critical: { bg: "rgba(239,68,68,0.10)", text: "#EF4444", border: "rgba(239,68,68,0.25)" },
  major:    { bg: "rgba(245,158,11,0.10)", text: "#F59E0B", border: "rgba(245,158,11,0.25)" },
  minor:    { bg: "rgba(59,130,246,0.08)", text: "#3B82F6", border: "rgba(59,130,246,0.20)" },
  info:     { bg: "rgba(113,118,127,0.06)", text: "#71767F", border: "rgba(113,118,127,0.15)" },
};

// ── File stat computation from diff lines ────────────────────────────────────

function computeFileStats(diffContent: string): { additions: number; deletions: number } {
  let additions = 0;
  let deletions = 0;
  for (const line of diffContent.split("\n")) {
    if (line.startsWith("+") && !line.startsWith("+++")) additions++;
    else if (line.startsWith("-") && !line.startsWith("---")) deletions++;
  }
  return { additions, deletions };
}

// ── Main component ───────────────────────────────────────────────────────────

export function ReviewView({
  open,
  onClose,
  context,
  gitStatus,
  insights,
  onCommit,
  onRefresh,
  onSelectFile,
  selectedFile,
}: ReviewViewProps) {
  const queryClient = useQueryClient();
  const [areaTab, setAreaTab] = useState<AreaTab>("unstaged");
  const [railTab, setRailTab] = useState<RailTab>("files");
  const [filterText, setFilterText] = useState("");
  const [expandedArtifacts, setExpandedArtifacts] = useState<Set<number>>(new Set());
  const [treeWidth, treeResizeHandler] = useResize(220, 160, 320);
  const [isMutating, setIsMutating] = useState(false);

  const { files, diffs, branch, capabilities, mode, runId, projectRoot } = context;

  // ── Computed file groups ─────────────────────────────────────────────────

  const stagedFiles = useMemo(() => files.filter(f => f.area === "staged"), [files]);
  const unstagedFiles = useMemo(() => files.filter(f => f.area === "unstaged" || f.area === "untracked"), [files]);
  const committedFiles = useMemo(() => files.filter(f => f.area === "committed"), [files]);

  const activeFiles = useMemo(() => {
    switch (areaTab) {
      case "staged": return stagedFiles;
      case "committed": return committedFiles;
      case "all": return files;
      default: return unstagedFiles;
    }
  }, [areaTab, stagedFiles, unstagedFiles, committedFiles, files]);

  // ── Filter matching ──────────────────────────────────────────────────────

  const filteredFiles = useMemo(() => {
    if (!filterText.trim()) return activeFiles;
    const lower = filterText.toLowerCase();
    return activeFiles.filter(f => f.path.toLowerCase().includes(lower));
  }, [activeFiles, filterText]);

  // ── Diff lines for selected file ─────────────────────────────────────────

  const selectedDiffContent = selectedFile ? (diffs[selectedFile] ?? null) : null;

  const diffLines = useMemo(
    () => selectedDiffContent ? parseDiffLines(selectedDiffContent) : [],
    [selectedDiffContent],
  );

  const selectedFileStats = useMemo(
    () => selectedDiffContent ? computeFileStats(selectedDiffContent) : { additions: 0, deletions: 0 },
    [selectedDiffContent],
  );

  // ── Staging lookup from gitStatus ────────────────────────────────────────

  const stagedPaths = useMemo(
    () => new Set(gitStatus?.filter(s => s.area === "staged").map(s => s.path) ?? []),
    [gitStatus],
  );

  const fileStatsMap: Record<string, { additions: number; deletions: number }> = useMemo(() => {
    const map: Record<string, { additions: number; deletions: number }> = {};
    if (gitStatus) {
      for (const entry of gitStatus) {
        if (!map[entry.path]) {
          map[entry.path] = { additions: entry.additions, deletions: entry.deletions };
        }
      }
    }
    // Supplement from diffs for files without gitStatus
    for (const f of files) {
      if (!map[f.path] && diffs[f.path]) {
        map[f.path] = computeFileStats(diffs[f.path]);
      }
    }
    return map;
  }, [gitStatus, files, diffs]);

  // ── Total stats for header ───────────────────────────────────────────────

  const totalStats = useMemo(() => {
    let add = 0;
    let del = 0;
    for (const f of activeFiles) {
      const s = fileStatsMap[f.path];
      if (s) { add += s.additions; del += s.deletions; }
    }
    return { additions: add, deletions: del };
  }, [activeFiles, fileStatsMap]);

  // ── Auto-switch area tab on mount / data change ──────────────────────────

  useEffect(() => {
    const preferredTab: AreaTab =
      unstagedFiles.length > 0 ? "unstaged" :
      stagedFiles.length > 0 ? "staged" :
      committedFiles.length > 0 ? "committed" :
      "unstaged";

    if (areaTab === "all") return; // "all" is sticky

    const currentTabHasFiles =
      (areaTab === "unstaged" && unstagedFiles.length > 0) ||
      (areaTab === "staged" && stagedFiles.length > 0) ||
      (areaTab === "committed" && committedFiles.length > 0);

    if (!currentTabHasFiles && areaTab !== preferredTab) {
      setAreaTab(preferredTab);
    }
  }, [areaTab, unstagedFiles.length, stagedFiles.length, committedFiles.length]);

  // ── Auto-select first file when active set changes ───────────────────────

  useEffect(() => {
    if (filteredFiles.length === 0) return;
    if (!selectedFile || !filteredFiles.some(f => f.path === selectedFile)) {
      onSelectFile?.(filteredFiles[0].path);
    }
  }, [filteredFiles, selectedFile, onSelectFile]);

  // Must be before early return (Rules of Hooks)
  if (!open) return null;

  // ── Selected file area badge ─────────────────────────────────────────────

  const selectedFileEntry = selectedFile ? files.find(f => f.path === selectedFile) : null;
  const selectedArea = selectedFileEntry?.area ?? null;

  // ── Staging handlers ─────────────────────────────────────────────────────

  const handleStageFile = async (path: string) => {
    if (areaTab === "committed") return;
    if (isMutating) return;
    setIsMutating(true);
    try {
      if (mode === "run" && runId) {
        await gitStageFiles(runId, [path]);
      } else if (mode === "project" && projectRoot) {
        await gitProjectStageFiles(projectRoot, [path]);
      }
      invalidateAndRefresh();
    } catch (_) {
      // Staging failure is non-fatal; the UI will show stale state until next refresh.
    } finally {
      setIsMutating(false);
    }
  };

  const handleUnstageFile = async (path: string) => {
    if (areaTab === "committed") return;
    if (isMutating) return;
    setIsMutating(true);
    try {
      if (mode === "run" && runId) {
        await gitUnstageFiles(runId, [path]);
      } else if (mode === "project" && projectRoot) {
        await gitProjectUnstageFiles(projectRoot, [path]);
      }
      invalidateAndRefresh();
    } catch (_) {
      // Unstaging failure is non-fatal.
    } finally {
      setIsMutating(false);
    }
  };

  const handleStageAll = async () => {
    if (!capabilities.canStage) return;
    if (isMutating) return;
    setIsMutating(true);
    try {
      if (mode === "run" && runId) {
        await gitStageAll(runId);
      } else if (mode === "project" && projectRoot) {
        await gitProjectStageAll(projectRoot);
      }
      invalidateAndRefresh();
    } catch (_) {
      // Stage-all failure is non-fatal.
    } finally {
      setIsMutating(false);
    }
  };

  const handleRevertAll = async () => {
    if (!capabilities.canRevert) return;
    if (isMutating) return;
    if (!window.confirm("Revert all uncommitted changes? This cannot be undone.")) return;
    setIsMutating(true);
    try {
      if (mode === "run" && runId) {
        await gitRevertAll(runId);
      } else if (mode === "project" && projectRoot) {
        await gitProjectRevertAll(projectRoot);
      }
      invalidateAndRefresh();
    } catch (_) {
      // Revert failure is non-fatal.
    } finally {
      setIsMutating(false);
    }
  };

  const invalidateAndRefresh = () => {
    if (runId) {
      queryClient.invalidateQueries({ queryKey: qk.panelData(runId) });
    }
    if (projectRoot) {
      queryClient.invalidateQueries({ queryKey: qk.projectPanelData(projectRoot) });
    }
    onRefresh?.();
  };

  // ── Clipboard ────────────────────────────────────────────────────────────

  const handleCopyDiff = () => {
    if (selectedDiffContent) {
      navigator.clipboard.writeText(selectedDiffContent).catch(() => {});
    }
  };

  // ── Area tab button ──────────────────────────────────────────────────────

  const canStageInTab = capabilities.canStage && areaTab !== "committed";

  // ── AI Review helpers ────────────────────────────────────────────────────

  const toggleArtifact = (idx: number) => {
    setExpandedArtifacts(prev => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  };

  // ── Render ─────────────────────────────────────────────────────────────────

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 50,
        background: C.base,
        display: "flex",
        flexDirection: "column",
        fontSize: 12,
      }}
    >
      {/* ═══ HEADER ═══ */}
      <div
        style={{
          height: 42,
          minHeight: 42,
          background: C.surface,
          display: "flex",
          alignItems: "center",
          padding: "0 12px",
          justifyContent: "space-between",
          borderBottom: `1px solid ${C.border}`,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          {/* Area tabs */}
          <div style={{ display: "flex", borderRadius: 6, overflow: "hidden", gap: 2 }}>
            {([
              { key: "unstaged" as AreaTab, label: "Unstaged", count: unstagedFiles.length },
              { key: "staged" as AreaTab, label: "Staged", count: stagedFiles.length },
              { key: "committed" as AreaTab, label: "Committed", count: committedFiles.length },
              { key: "all" as AreaTab, label: "All", count: files.length },
            ] as const).map(({ key, label, count }) => (
              <button
                key={key}
                onClick={() => setAreaTab(key)}
                style={{
                  padding: "3px 10px",
                  background: areaTab === key ? C.surfaceActive : "transparent",
                  border: "none",
                  color: areaTab === key ? C.text1 : C.text4,
                  fontSize: 11,
                  fontWeight: 500,
                  cursor: "pointer",
                  borderRadius: 6,
                }}
              >
                {label} {"\u00B7"} {count}
              </button>
            ))}
          </div>

          {/* Branch info */}
          {branch && (
            <span style={{ fontSize: 11, color: C.text4, fontFamily: C.mono, marginLeft: 4 }}>
              {branch.branch}
              {(branch.ahead > 0 || branch.behind > 0) && (
                <span style={{ marginLeft: 4 }}>
                  {branch.ahead > 0 && <span style={{ color: "#31B97B" }}>+{branch.ahead}</span>}
                  {branch.ahead > 0 && branch.behind > 0 && " "}
                  {branch.behind > 0 && <span style={{ color: "#EF4444" }}>-{branch.behind}</span>}
                </span>
              )}
            </span>
          )}
        </div>

        {/* Right side of header */}
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <span style={{ fontFamily: C.mono, fontSize: 11 }}>
            <span style={{ color: "#31B97B" }}>+{totalStats.additions}</span>{" "}
            <span style={{ color: "#EF4444" }}>-{totalStats.deletions}</span>
          </span>
          <div style={{ width: 1, height: 20, background: C.surfaceHover, margin: "0 2px" }} />
          <button
            onClick={() => { onRefresh?.(); }}
            className="btn-ghost"
            style={{
              padding: "4px 6px",
              borderRadius: 6,
              background: "transparent",
              color: C.text3,
              cursor: "pointer",
              display: "flex",
              alignItems: "center",
            }}
            title="Refresh files"
          >
            <Refresh size={12} />
          </button>
          <button
            onClick={onClose}
            className="btn-ghost"
            style={{
              display: "flex",
              alignItems: "center",
              gap: 4,
              padding: "4px 10px",
              borderRadius: 6,
              background: "transparent",
              color: C.text3,
              fontSize: 11,
              cursor: "pointer",
            }}
          >
            Close review <XIcon size={9} />
          </button>
        </div>
      </div>

      {/* ═══ BODY ═══ */}
      <div style={{ flex: 1, display: "flex", overflow: "hidden" }}>
        {/* ─── DIFF CANVAS (left, flex:1) ─── */}
        <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
          {/* Sticky file header */}
          {selectedFile && (
            <div
              style={{
                padding: "5px 12px",
                background: C.surfaceHover,
                display: "flex",
                alignItems: "center",
                gap: 8,
                borderBottom: `1px solid ${C.border}`,
                flexShrink: 0,
              }}
            >
              <span
                style={{
                  fontFamily: C.mono,
                  fontSize: 11,
                  color: C.text2,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                  flex: 1,
                }}
              >
                {selectedFile}
              </span>
              {selectedDiffContent && (
                <span style={{ fontFamily: C.mono, fontSize: 10, display: "flex", gap: 4, flexShrink: 0 }}>
                  <span style={{ color: "#31B97B" }}>+{selectedFileStats.additions}</span>
                  <span style={{ color: "#EF4444" }}>-{selectedFileStats.deletions}</span>
                </span>
              )}
              {selectedArea && (
                <span
                  style={{
                    fontSize: 9,
                    fontWeight: 600,
                    padding: "1px 6px",
                    borderRadius: 3,
                    background: selectedArea === "staged" ? "rgba(49,185,123,0.10)"
                      : selectedArea === "committed" ? "rgba(59,130,246,0.08)"
                      : "rgba(245,158,11,0.08)",
                    color: selectedArea === "staged" ? "#31B97B"
                      : selectedArea === "committed" ? "#3B82F6"
                      : "#F59E0B",
                    textTransform: "uppercase",
                    letterSpacing: "0.03em",
                    flexShrink: 0,
                  }}
                >
                  {selectedArea}
                </span>
              )}
              <button
                onClick={handleCopyDiff}
                className="btn-ghost"
                style={{
                  padding: "2px 4px",
                  borderRadius: 4,
                  background: "transparent",
                  color: C.text4,
                  cursor: selectedDiffContent ? "pointer" : "default",
                  opacity: selectedDiffContent ? 1 : 0.4,
                  display: "flex",
                  alignItems: "center",
                  flexShrink: 0,
                }}
                title="Copy diff to clipboard"
              >
                <Copy size={11} />
              </button>
            </div>
          )}

          {/* Diff content with horizontal scroll */}
          <div
            style={{
              flex: 1,
              overflowY: "auto",
              overflowX: "auto",
              fontFamily: C.mono,
              fontSize: 11,
              lineHeight: 1.7,
            }}
          >
            {diffLines.length > 0 ? (
              diffLines.map((line, i) => {
                const isDel = line.type === "del";
                const isAdd = line.type === "add";
                const isHunk = line.type === "hunk";

                if (isHunk) {
                  return (
                    <div
                      key={`hunk-${i}-${line.oldNum ?? 'x'}`}
                      style={{
                        padding: "3px 16px",
                        background: "rgba(59,130,246,0.04)",
                        color: "rgba(59,130,246,0.53)",
                        fontSize: 11,
                        fontWeight: 500,
                        whiteSpace: "pre",
                      }}
                    >
                      {line.text}
                    </div>
                  );
                }

                return (
                  <div
                    key={`${line.type}-${line.oldNum ?? 'x'}-${line.newNum ?? 'x'}-${i}`}
                    style={{
                      display: "flex",
                      minHeight: 21,
                      background: isDel
                        ? "rgba(239,68,68,0.07)"
                        : isAdd
                        ? "rgba(49,185,123,0.07)"
                        : "transparent",
                    }}
                  >
                    <span
                      style={{
                        width: 40,
                        textAlign: "right",
                        padding: "0 5px",
                        color: isDel ? "rgba(239,68,68,0.31)" : C.text4,
                        fontSize: 11,
                        userSelect: "none",
                        flexShrink: 0,
                        opacity: 0.5,
                      }}
                    >
                      {line.oldNum ?? ""}
                    </span>
                    <span
                      style={{
                        width: 40,
                        textAlign: "right",
                        padding: "0 5px",
                        color: isAdd ? "rgba(49,185,123,0.31)" : C.text4,
                        fontSize: 11,
                        userSelect: "none",
                        flexShrink: 0,
                        opacity: 0.5,
                      }}
                    >
                      {line.newNum ?? ""}
                    </span>
                    <span
                      style={{
                        width: 18,
                        textAlign: "center",
                        color: isDel ? "#EF4444" : isAdd ? "#31B97B" : "transparent",
                        fontSize: 11,
                        userSelect: "none",
                        flexShrink: 0,
                        fontWeight: 700,
                      }}
                    >
                      {isDel ? "\u2212" : isAdd ? "+" : " "}
                    </span>
                    <span
                      style={{
                        flex: 1,
                        padding: "0 6px",
                        color: isDel ? "#F87171" : isAdd ? C.accent : C.text2,
                        whiteSpace: "pre",
                      }}
                    >
                      {line.text}
                    </span>
                  </div>
                );
              })
            ) : (
              <div
                style={{
                  padding: "40px 20px",
                  textAlign: "center",
                  color: C.text4,
                  fontSize: 12,
                }}
              >
                {selectedFile
                  ? "No diff available"
                  : "Select a file to view its diff"}
              </div>
            )}
          </div>
        </div>

        {/* ─── RIGHT RAIL (resizable) ─── */}
        <div
          style={{
            width: treeWidth,
            minWidth: 160,
            maxWidth: 320,
            background: C.surface,
            display: "flex",
            flexDirection: "column",
            position: "relative",
          }}
        >
          {/* Resize handle */}
          <div
            onMouseDown={treeResizeHandler}
            style={{
              position: "absolute",
              top: 0,
              bottom: 0,
              left: -2,
              width: 5,
              cursor: "col-resize",
              zIndex: 10,
            }}
          >
            <div
              className="resize-line"
              style={{ position: "absolute", top: 0, bottom: 0, left: 2, width: 1 }}
            />
          </div>

          {/* Filter input */}
          <div style={{ padding: "6px 8px", borderBottom: `1px solid ${C.border}` }}>
            <div style={{ position: "relative" }}>
              <span style={{ position: "absolute", left: 7, top: 5, color: C.text4, pointerEvents: "none" }}>
                <Search size={11} />
              </span>
              <input
                type="text"
                placeholder="Filter files..."
                value={filterText}
                onChange={e => setFilterText(e.target.value)}
                style={{
                  width: "100%",
                  padding: "4px 8px 4px 24px",
                  background: C.surfaceHover,
                  border: `1px solid ${C.border}`,
                  borderRadius: 5,
                  color: C.text2,
                  fontSize: 11,
                  fontFamily: C.mono,
                  outline: "none",
                  boxSizing: "border-box",
                }}
              />
            </div>
          </div>

          {/* Rail tab selector */}
          <div
            style={{
              display: "flex",
              borderBottom: `1px solid ${C.border}`,
            }}
          >
            {(["files", "ai"] as const).map(tab => (
              <button
                key={tab}
                onClick={() => setRailTab(tab)}
                style={{
                  flex: 1,
                  padding: "6px 0",
                  background: "transparent",
                  border: "none",
                  borderBottom: railTab === tab ? `2px solid ${C.accent}` : "2px solid transparent",
                  color: railTab === tab ? C.text1 : C.text4,
                  fontSize: 10,
                  fontWeight: 600,
                  textTransform: "uppercase",
                  letterSpacing: "0.04em",
                  cursor: "pointer",
                }}
              >
                {tab === "files" ? `Files (${filteredFiles.length})` : "AI Review"}
              </button>
            ))}
          </div>

          {/* Rail content */}
          <div style={{ flex: 1, overflowY: "auto" }}>
            {railTab === "files" ? (
              <FilesRail
                files={filteredFiles}
                allFiles={activeFiles}
                selectedFile={selectedFile}
                onSelectFile={onSelectFile}
                canStage={canStageInTab}
                stagedPaths={stagedPaths}
                fileStatsMap={fileStatsMap}
                areaTab={areaTab}
                onStageFile={handleStageFile}
                onUnstageFile={handleUnstageFile}
              />
            ) : (
              <AiReviewRail
                mode={mode}
                insights={insights}
                expandedArtifacts={expandedArtifacts}
                onToggleArtifact={toggleArtifact}
                onSelectFile={onSelectFile}
              />
            )}
          </div>
        </div>
      </div>

      {/* ═══ BOTTOM DOCK ═══ */}
      <div
        style={{
          padding: "5px 12px",
          background: C.surface,
          display: "flex",
          justifyContent: "center",
          gap: 8,
          borderTop: `1px solid ${C.border}`,
        }}
      >
        <button
          className="btn-ghost"
          onClick={handleRevertAll}
          disabled={!capabilities.canRevert || isMutating}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 4,
            padding: "4px 12px",
            borderRadius: 6,
            background: "transparent",
            color: (capabilities.canRevert && !isMutating) ? C.text3 : C.text4,
            fontSize: 11,
            cursor: (capabilities.canRevert && !isMutating) ? "pointer" : "default",
            opacity: (capabilities.canRevert && !isMutating) ? 1 : 0.5,
            border: "none",
          }}
        >
          <Undo size={10} /> Revert all
        </button>
        <button
          className="btn-ghost"
          onClick={handleStageAll}
          disabled={!capabilities.canStage || areaTab === "committed" || isMutating}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 4,
            padding: "4px 12px",
            borderRadius: 6,
            background: "transparent",
            color: (capabilities.canStage && areaTab !== "committed" && !isMutating) ? C.text3 : C.text4,
            fontSize: 11,
            cursor: (capabilities.canStage && areaTab !== "committed" && !isMutating) ? "pointer" : "default",
            opacity: (capabilities.canStage && areaTab !== "committed" && !isMutating) ? 1 : 0.5,
            border: "none",
          }}
        >
          <Plus size={10} /> Stage all
        </button>
        <button
          className="btn-ghost"
          onClick={capabilities.canCommit ? onCommit : undefined}
          disabled={!capabilities.canCommit}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 4,
            padding: "4px 12px",
            borderRadius: 6,
            background: capabilities.canCommit ? C.surfaceHover : "transparent",
            color: capabilities.canCommit ? C.text2 : C.text4,
            fontSize: 11,
            fontWeight: 600,
            cursor: capabilities.canCommit ? "pointer" : "default",
            opacity: capabilities.canCommit ? 1 : 0.5,
            border: "none",
          }}
        >
          <Commit size={11} /> Commit <ChevronDown size={8} />
        </button>
      </div>
    </div>
  );
}

// ── Files rail sub-component ─────────────────────────────────────────────────

interface FilesRailProps {
  files: FileDiffEntry[];
  allFiles: FileDiffEntry[];
  selectedFile: string | null;
  onSelectFile?: (path: string) => void;
  canStage: boolean;
  stagedPaths: Set<string>;
  fileStatsMap: Record<string, { additions: number; deletions: number }>;
  areaTab: AreaTab;
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
}

function FilesRail({
  files,
  allFiles,
  selectedFile,
  onSelectFile,
  canStage,
  stagedPaths,
  fileStatsMap,
  areaTab,
  onStageFile,
  onUnstageFile,
}: FilesRailProps) {
  if (files.length === 0) {
    return (
      <div style={{ padding: "24px 12px", textAlign: "center", color: C.text4, fontSize: 11 }}>
        {allFiles.length === 0
          ? (areaTab === "committed" ? "No committed branch changes"
            : areaTab === "all" ? "No files"
            : `No ${areaTab} files`)
          : "No files match filter"
        }
      </div>
    );
  }

  return (
    <FileTree
      tree={buildFileTree(files)}
      selected={selectedFile ?? ""}
      onSelect={onSelectFile}
      renderRight={(path) => {
        const isStaged = stagedPaths.has(path);
        const stats = fileStatsMap[path];
        return (
          <>
            {canStage ? (
              <input
                type="checkbox"
                checked={isStaged}
                onChange={() => isStaged ? onUnstageFile(path) : onStageFile(path)}
                style={{ width: 12, height: 12, cursor: "pointer", accentColor: C.accent }}
              />
            ) : areaTab === "committed" ? (
              <span style={{ fontSize: 9, color: C.text4, fontFamily: C.mono }}>
                committed
              </span>
            ) : null}
            {stats && (stats.additions > 0 || stats.deletions > 0) && (
              <span style={{ fontSize: 9, fontFamily: C.mono, display: "flex", gap: 3 }}>
                {stats.additions > 0 && <span style={{ color: "#31B97B" }}>+{stats.additions}</span>}
                {stats.deletions > 0 && <span style={{ color: "#EF4444" }}>-{stats.deletions}</span>}
              </span>
            )}
          </>
        );
      }}
    />
  );
}

// ── AI Review rail sub-component ─────────────────────────────────────────────

interface AiReviewRailProps {
  mode: "run" | "project";
  insights?: ReviewInsights | null;
  expandedArtifacts: Set<number>;
  onToggleArtifact: (idx: number) => void;
  onSelectFile?: (path: string) => void;
}

function AiReviewRail({
  mode,
  insights,
  expandedArtifacts,
  onToggleArtifact,
  onSelectFile,
}: AiReviewRailProps) {
  if (mode === "project") {
    return (
      <div style={{ padding: "24px 12px", textAlign: "center", color: C.text4, fontSize: 11 }}>
        AI review is only available from Grove runs
      </div>
    );
  }

  if (!insights) {
    return (
      <div style={{ padding: "24px 12px", textAlign: "center", color: C.text4, fontSize: 11 }}>
        No AI review available
      </div>
    );
  }

  const sortedFindings = [...insights.findings].sort(
    (a, b) => SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity]
  );

  // Group findings by file (null = general)
  const findingsByFile = new Map<string | null, ReviewFinding[]>();
  for (const f of sortedFindings) {
    const key = f.file;
    const list = findingsByFile.get(key) ?? [];
    list.push(f);
    findingsByFile.set(key, list);
  }

  return (
    <div style={{ padding: "8px" }}>
      {/* Reviewer verdict */}
      {insights.reviewerVerdict && (
        <VerdictCard label="Reviewer" text={insights.reviewerVerdict} />
      )}

      {/* Judge verdict */}
      {insights.judgeVerdict && (
        <VerdictCard label="Judge" text={insights.judgeVerdict} />
      )}

      {/* Findings */}
      {sortedFindings.length > 0 && (
        <div style={{ marginTop: 8 }}>
          <div style={{ fontSize: 10, fontWeight: 600, color: C.text4, textTransform: "uppercase", letterSpacing: "0.04em", marginBottom: 4 }}>
            Findings ({sortedFindings.length})
          </div>
          {Array.from(findingsByFile.entries()).map(([filePath, findings]) => (
            <div key={filePath ?? "__general"} style={{ marginBottom: 6 }}>
              {filePath && (
                <div
                  onClick={() => onSelectFile?.(filePath)}
                  style={{
                    fontSize: 10,
                    fontFamily: C.mono,
                    color: C.blue,
                    cursor: "pointer",
                    padding: "2px 0",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {filePath}
                </div>
              )}
              {findings.map((finding, fi) => (
                <FindingCard key={fi} finding={finding} />
              ))}
            </div>
          ))}
        </div>
      )}

      {/* Source artifacts */}
      {insights.rawMarkdown.length > 0 && (
        <div style={{ marginTop: 8 }}>
          <div style={{ fontSize: 10, fontWeight: 600, color: C.text4, textTransform: "uppercase", letterSpacing: "0.04em", marginBottom: 4 }}>
            Source Artifacts
          </div>
          {insights.rawMarkdown.map((md, idx) => {
            const label = insights.sourceArtifacts[idx] ?? `Artifact ${idx + 1}`;
            const isOpen = expandedArtifacts.has(idx);
            return (
              <div key={idx} style={{ marginBottom: 4 }}>
                <button
                  onClick={() => onToggleArtifact(idx)}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 4,
                    width: "100%",
                    padding: "4px 6px",
                    background: C.surfaceHover,
                    border: `1px solid ${C.border}`,
                    borderRadius: 4,
                    color: C.text3,
                    fontSize: 10,
                    cursor: "pointer",
                    textAlign: "left",
                  }}
                >
                  <ChevronDown size={8} />
                  <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {label}
                  </span>
                </button>
                {isOpen && (
                  <div
                    style={{
                      padding: "6px 8px",
                      background: C.base,
                      border: `1px solid ${C.border}`,
                      borderTop: "none",
                      borderRadius: "0 0 4px 4px",
                      fontFamily: C.mono,
                      fontSize: 10,
                      color: C.text3,
                      whiteSpace: "pre-wrap",
                      wordBreak: "break-word",
                      maxHeight: 300,
                      overflowY: "auto",
                    }}
                  >
                    {md}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ── Verdict card ─────────────────────────────────────────────────────────────

function VerdictCard({ label, text }: { label: string; text: string }) {
  return (
    <div
      style={{
        padding: "6px 8px",
        background: C.surfaceHover,
        border: `1px solid ${C.border}`,
        borderRadius: 6,
        marginBottom: 6,
      }}
    >
      <div style={{ fontSize: 9, fontWeight: 600, color: C.text4, textTransform: "uppercase", letterSpacing: "0.04em", marginBottom: 2 }}>
        {label} Verdict
      </div>
      <div style={{ fontSize: 11, color: C.text2, lineHeight: 1.4 }}>
        {text}
      </div>
    </div>
  );
}

// ── Finding card ─────────────────────────────────────────────────────────────

function FindingCard({ finding }: { finding: ReviewFinding }) {
  const sc = SEVERITY_COLORS[finding.severity];
  return (
    <div
      style={{
        padding: "5px 7px",
        background: sc.bg,
        border: `1px solid ${sc.border}`,
        borderRadius: 4,
        marginBottom: 3,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 4, marginBottom: 2 }}>
        <span
          style={{
            fontSize: 8,
            fontWeight: 700,
            color: sc.text,
            textTransform: "uppercase",
            letterSpacing: "0.04em",
          }}
        >
          {finding.severity}
        </span>
        {finding.line !== null && (
          <span style={{ fontSize: 9, color: C.text4, fontFamily: C.mono }}>
            L{finding.line}
          </span>
        )}
      </div>
      <div style={{ fontSize: 11, color: C.text2, fontWeight: 500, marginBottom: 1 }}>
        {finding.title}
      </div>
      <div style={{ fontSize: 10, color: C.text3, lineHeight: 1.4 }}>
        {finding.body}
      </div>
    </div>
  );
}
