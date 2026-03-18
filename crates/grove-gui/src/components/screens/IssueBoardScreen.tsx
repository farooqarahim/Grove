import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { NewIssueModal } from "@/components/modals/NewIssueModal";
import { KanbanColumn } from "@/components/issue-board/KanbanColumn";
import { IssueDrawer } from "@/components/issue-board/IssueDrawer";
import { BoardEditorModal } from "@/components/issue-board/BoardEditorModal";
import { PlusIcon, SearchIcon, ChevronDownIcon, RefreshIcon, BoardIcon, StackIcon } from "@/components/issue-board/Icons";
import {
  BOARD_CSS, CANONICAL_SEQUENCE, COLUMN_CONFIGS,
  FILTER_PRIORITIES, SOURCES,
  type LayoutMode,
} from "@/components/issue-board/constants";
import {
  buildConfiguredColumns, buildProviderColumns,
  compositeId, displayProvider, emptyProjectSettings, normalizePriority, providerFromSource,
} from "@/components/issue-board/helpers";
import {
  getProjectSettings, issueBoard, issueSyncAll, issueSyncProvider, updateProjectSettings,
} from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import type { Issue, IssueBoardConfig, ProjectRow, ProjectSettings, SyncState } from "@/types";

function formatRelative(ts: string): string {
  const d = new Date(ts);
  if (isNaN(d.getTime())) return ts;
  const diff = Date.now() - d.getTime();
  const m = Math.floor(diff / 60000);
  if (m < 1) return "just now";
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const days = Math.floor(h / 24);
  if (days < 7) return `${days}d ago`;
  if (days < 30) return `${Math.floor(days / 7)}w ago`;
  return d.toLocaleDateString();
}

function makeBoardConfig(columns?: { id: string; label: string; canonical_status: import("@/types").CanonicalStatus }[]): IssueBoardConfig {
  const sourceColumns = columns && columns.length > 0
    ? columns
    : CANONICAL_SEQUENCE.map((status) => ({
      id: status,
      label: COLUMN_CONFIGS[status].label,
      canonical_status: status,
    }));
  return {
    columns: sourceColumns.map((column) => ({
      id: column.id,
      label: column.label,
      canonical_status: column.canonical_status,
      match_rules: {},
      provider_targets: {},
    })),
  };
}

// ── Props ─────────────────────────────────────────────────────────────────────

interface IssueBoardScreenProps {
  projectId: string | null;
  projects: ProjectRow[];
  onProjectChange: (id: string) => void;
}

// ── Main ──────────────────────────────────────────────────────────────────────

export function IssueBoardScreen({ projectId, projects, onProjectChange }: IssueBoardScreenProps) {
  const [selectedIssue, setSelectedIssue] = useState<Issue | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [showBoardEditor, setShowBoardEditor] = useState(false);
  const [activeSource, setActiveSource] = useState("All");
  const [activePriority, setActivePriority] = useState("Any priority");
  const [search, setSearch] = useState("");
  const [prDropdown, setPrDropdown] = useState(false);
  const [projectDropdown, setProjectDropdown] = useState(false);
  const [syncing, setSyncing] = useState<string | null>(null);
  const [syncMsg, setSyncMsg] = useState<string | null>(null);
  const [layoutMode, setLayoutMode] = useState<LayoutMode>("project");
  const [projectSettings, setProjectSettings] = useState<ProjectSettings | null>(null);
  const [boardDraft, setBoardDraft] = useState<IssueBoardConfig | null>(null);
  const [savingBoard, setSavingBoard] = useState(false);

  const { data: board, refetch } = useQuery({
    queryKey: qk.issueBoard(projectId),
    queryFn: () => projectId ? issueBoard(projectId) : Promise.resolve(null),
    refetchInterval: 60000,
    staleTime: 30000,
  });

  useEffect(() => {
    if (!projectId) {
      setProjectSettings(null);
      return;
    }
    getProjectSettings(projectId)
      .then(setProjectSettings)
      .catch(() => setProjectSettings(emptyProjectSettings()));
  }, [projectId]);

  useEffect(() => {
    if (!showBoardEditor) return;
    setBoardDraft(projectSettings?.issue_board ?? makeBoardConfig(board?.columns));
  }, [showBoardEditor, projectSettings, board]);

  const handleSync = async (provider?: string) => {
    if (syncing || !projectId) return;
    setSyncing(provider ?? "all"); setSyncMsg(null);
    try {
      if (provider) {
        const r = await issueSyncProvider(projectId, provider, true);
        setSyncMsg(`+${r.new_count} new`);
      } else {
        const r = await issueSyncAll(projectId, true);
        setSyncMsg(`+${r.total_new} new, ${r.total_updated} updated`);
      }
      refetch();
    } catch (e) {
      setSyncMsg(e instanceof Error ? e.message : String(e));
    } finally {
      setSyncing(null);
      setTimeout(() => setSyncMsg(null), 3500);
    }
  };

  const handleSaveBoard = async () => {
    if (!projectId || !projectSettings || !boardDraft) return;
    setSavingBoard(true);
    try {
      const nextSettings: ProjectSettings = { ...projectSettings, issue_board: boardDraft };
      await updateProjectSettings(projectId, nextSettings);
      setProjectSettings(nextSettings);
      setShowBoardEditor(false);
      setBoardDraft(null);
      await refetch();
      setSyncMsg("Board updated");
      setTimeout(() => setSyncMsg(null), 3500);
    } catch (error) {
      setSyncMsg(error instanceof Error ? error.message : String(error));
    } finally {
      setSavingBoard(false);
    }
  };

  const activeProjects = projects.filter(p => p.state === "active");
  const currentProject = projects.find(p => p.id === projectId);
  const projectName = currentProject
    ? (currentProject.name || currentProject.root_path.split("/").pop() || "Project")
    : null;

  if (!projectId) {
    return (
      <div style={{
        flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
        background: "rgb(17, 20, 25)", padding: 40,
      }}>
        <div style={{ textAlign: "center", maxWidth: 520 }}>
          <div style={{
            width: 48, height: 48, borderRadius: 12, margin: "0 auto 20px",
            background: "rgba(49,185,123,0.1)", border: "1px solid rgba(49,185,123,0.15)",
            display: "flex", alignItems: "center", justifyContent: "center",
          }}>
            <BoardIcon />
          </div>
          <h2 style={{ fontSize: 18, fontWeight: 700, color: "#f1f5f9", letterSpacing: "-0.02em", margin: "0 0 8px" }}>
            Select a project
          </h2>
          <p style={{ fontSize: 13, color: "#475569", margin: "0 0 28px", lineHeight: 1.5 }}>
            Choose a project to view and manage its issue board.
          </p>
          {activeProjects.length === 0 ? (
            <p style={{ fontSize: 13, color: "#334155" }}>No active projects found.</p>
          ) : (
            <div style={{ display: "flex", gap: 10, flexWrap: "wrap", justifyContent: "center" }}>
              {activeProjects.map(p => {
                const name = p.name || p.root_path.split("/").pop() || p.id;
                const sub = p.name ? p.root_path.split("/").pop() : null;
                return (
                  <button
                    key={p.id}
                    onClick={() => onProjectChange(p.id)}
                    style={{
                      padding: "12px 20px", borderRadius: 10, fontSize: 13, fontWeight: 600,
                      cursor: "pointer", fontFamily: "inherit", textAlign: "left",
                      background: "rgba(15,23,42,0.6)", border: "1px solid rgba(51,65,85,0.3)",
                      color: "#e2e8f0", transition: "all .15s", minWidth: 140,
                    }}
                    onMouseEnter={e => {
                      e.currentTarget.style.background = "rgba(49,185,123,0.08)";
                      e.currentTarget.style.borderColor = "rgba(49,185,123,0.25)";
                      e.currentTarget.style.color = "#4ade80";
                    }}
                    onMouseLeave={e => {
                      e.currentTarget.style.background = "rgba(15,23,42,0.6)";
                      e.currentTarget.style.borderColor = "rgba(51,65,85,0.3)";
                      e.currentTarget.style.color = "#e2e8f0";
                    }}
                  >
                    <div style={{ fontWeight: 700 }}>{name}</div>
                    {sub && <div style={{ fontSize: 10, color: "#475569", marginTop: 2, fontWeight: 400 }}>{sub}</div>}
                  </button>
                );
              })}
            </div>
          )}
        </div>
      </div>
    );
  }

  const filterIssue = (issue: Issue): boolean => {
    const src = displayProvider(issue.provider);
    if (activeSource !== "All" && src !== activeSource) return false;
    if (activePriority !== "Any priority" && normalizePriority(issue.priority) !== activePriority) return false;
    if (search && !issue.title.toLowerCase().includes(search.toLowerCase()) && !issue.external_id.toLowerCase().includes(search.toLowerCase())) return false;
    return true;
  };

  const totalFiltered = board ? board.columns.reduce((acc, col) => acc + col.issues.filter(filterIssue).length, 0) : 0;
  const syncStates: SyncState[] = board?.sync_states ?? [];
  const flattenedIssues = board?.columns.flatMap((col) => col.issues) ?? [];
  const selectedProvider = providerFromSource(activeSource);
  const canUseProviderLayout = selectedProvider !== null;
  const usingProviderLayout = layoutMode === "provider" && canUseProviderLayout;
  const providerScopedIssues = canUseProviderLayout
    ? flattenedIssues.filter((issue) => issue.provider === selectedProvider)
    : [];
  const displayColumns = usingProviderLayout
    ? buildProviderColumns(providerScopedIssues, selectedProvider!, filterIssue)
    : buildConfiguredColumns(board?.columns ?? [], filterIssue);
  const editorConfig = boardDraft ?? projectSettings?.issue_board ?? makeBoardConfig(board?.columns);
  const boardDescriptor = usingProviderLayout
    ? `${displayProvider(selectedProvider!)} statuses`
    : "Project board";

  return (
    <div style={{
      flex: 1, display: "flex", flexDirection: "column", overflow: "hidden",
      background: "rgb(17, 20, 25)", color: "#e2e8f0",
      fontFamily: "'Geist', 'DM Sans', -apple-system, sans-serif",
    }}>
      <style>{BOARD_CSS}</style>

      {/* ─── TOP BAR ─── */}
      <div style={{ padding: "20px 28px 0", flexShrink: 0 }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 18 }}>
          <div style={{ display: "flex", alignItems: "baseline", gap: 10 }}>
            <h1 style={{ fontSize: 20, fontWeight: 700, color: "#f1f5f9", letterSpacing: "-0.03em", margin: 0 }}>Issue Board</h1>
            <span style={{ fontSize: 13, color: "#334155", fontWeight: 500 }}>{totalFiltered} total</span>
            <span style={{
              fontSize: 11, fontWeight: 700,
              color: usingProviderLayout ? "#93c5fd" : "#94a3b8",
              background: usingProviderLayout ? "rgba(59,130,246,0.12)" : "rgba(51,65,85,0.2)",
              border: usingProviderLayout ? "1px solid rgba(59,130,246,0.22)" : "1px solid rgba(51,65,85,0.25)",
              borderRadius: 999, padding: "4px 10px", letterSpacing: "0.03em",
            }}>
              {boardDescriptor}
            </span>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            {/* Project picker */}
            <div style={{ position: "relative" }}>
              <button
                onClick={() => setProjectDropdown(v => !v)}
                style={{
                  display: "flex", alignItems: "center", gap: 7, padding: "8px 13px",
                  borderRadius: 10, fontSize: 12, fontWeight: 600, cursor: "pointer", fontFamily: "inherit",
                  background: "rgba(15,23,42,0.7)", border: "1px solid rgba(51,65,85,0.35)",
                  color: "#94a3b8", transition: "all .15s",
                }}
                onMouseEnter={e => { e.currentTarget.style.borderColor = "rgba(99,102,241,0.35)"; e.currentTarget.style.color = "#c7d2fe"; }}
                onMouseLeave={e => { if (!projectDropdown) { e.currentTarget.style.borderColor = "rgba(51,65,85,0.35)"; e.currentTarget.style.color = "#94a3b8"; } }}
              >
                <svg width="13" height="13" viewBox="0 0 16 16" fill="none">
                  <path d="M2 4.5C2 3.67 2.67 3 3.5 3H6.4L8 5H12.5C13.33 5 14 5.67 14 6.5V12C14 12.83 13.33 13.5 12.5 13.5H3.5C2.67 13.5 2 12.83 2 12V4.5Z" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round"/>
                </svg>
                <span style={{ maxWidth: 160, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {projectName}
                </span>
                <ChevronDownIcon />
              </button>

              {projectDropdown && (
                <>
                  <div
                    onClick={() => setProjectDropdown(false)}
                    style={{ position: "fixed", inset: 0, zIndex: 40 }}
                  />
                  <div style={{
                    position: "absolute", top: "calc(100% + 6px)", right: 0, zIndex: 50,
                    background: "#0c1528", border: "1px solid rgba(51,65,85,0.4)", borderRadius: 12,
                    boxShadow: "0 16px 48px rgba(0,0,0,0.5)", overflow: "hidden", minWidth: 220,
                  }}>
                    <div style={{ padding: "8px 12px 6px", borderBottom: "1px solid rgba(51,65,85,0.2)" }}>
                      <span style={{ fontSize: 10, fontWeight: 700, color: "#334155", textTransform: "uppercase", letterSpacing: "0.08em" }}>
                        Switch Project
                      </span>
                    </div>
                    {activeProjects.map(p => {
                      const name = p.name || p.root_path.split("/").pop() || p.id;
                      const sub = p.name ? p.root_path.split("/").pop() : null;
                      const active = p.id === projectId;
                      return (
                        <button
                          key={p.id}
                          onClick={() => { onProjectChange(p.id); setProjectDropdown(false); }}
                          style={{
                            display: "flex", alignItems: "center", gap: 10, width: "100%",
                            padding: "10px 14px", border: "none", cursor: "pointer", fontFamily: "inherit",
                            background: active ? "rgba(49,185,123,0.1)" : "transparent",
                            textAlign: "left", transition: "background .1s",
                          }}
                          onMouseEnter={e => { if (!active) e.currentTarget.style.background = "rgba(51,65,85,0.2)"; }}
                          onMouseLeave={e => { if (!active) e.currentTarget.style.background = "transparent"; }}
                        >
                          <span style={{
                            width: 7, height: 7, borderRadius: "50%", flexShrink: 0,
                            background: active ? "#4ade80" : "rgba(51,65,85,0.6)",
                            boxShadow: active ? "0 0 6px rgba(74,222,128,0.4)" : undefined,
                          }} />
                          <div>
                            <div style={{ fontSize: 13, fontWeight: active ? 700 : 500, color: active ? "#4ade80" : "#e2e8f0" }}>{name}</div>
                            {sub && <div style={{ fontSize: 10, color: "#475569", marginTop: 1 }}>{sub}</div>}
                          </div>
                        </button>
                      );
                    })}
                  </div>
                </>
              )}
            </div>

            <div style={{ width: 1, height: 20, background: "rgba(51,65,85,0.3)" }} />

            <button
              onClick={() => setShowBoardEditor(true)}
              style={{
                display: "flex", alignItems: "center", gap: 6, padding: "9px 14px",
                borderRadius: 10, fontSize: 12, fontWeight: 600, cursor: "pointer", fontFamily: "inherit",
                background: "rgba(59,130,246,0.08)", border: "1px solid rgba(59,130,246,0.18)",
                color: "#93c5fd", transition: "all .2s",
              }}
            >
              <BoardIcon /> Edit Board
            </button>
            {syncStates.length > 0 && (
              <button
                onClick={() => void handleSync()}
                disabled={!!syncing}
                className="ib-sync-btn"
                style={{
                  display: "flex", alignItems: "center", gap: 6, padding: "9px 14px",
                  borderRadius: 10, fontSize: 12, fontWeight: 600,
                  cursor: syncing ? "default" : "pointer", fontFamily: "inherit",
                  background: "rgba(51,65,85,0.2)", border: "1px solid rgba(51,65,85,0.3)",
                  color: syncing ? "#334155" : "#94a3b8", transition: "all .2s",
                }}
              >
                <RefreshIcon /> {syncing === "all" ? "Syncing…" : "Sync All"}
              </button>
            )}
            {syncMsg && <span style={{ fontSize: 11, color: "#4ade80" }}>{syncMsg}</span>}
            <button
              onClick={() => setShowCreate(true)}
              style={{
                display: "flex", alignItems: "center", gap: 6, padding: "9px 18px",
                borderRadius: 10, fontSize: 13, fontWeight: 700, cursor: "pointer", fontFamily: "inherit",
                background: "linear-gradient(135deg,#31B97B,#269962)", color: "#fff",
                border: "1px solid rgba(49,185,123,0.3)", boxShadow: "0 0 24px rgba(49,185,123,0.12)",
                transition: "all .2s",
              }}
            >
              <PlusIcon /> New Issue
            </button>
          </div>
        </div>

        {/* Filter bar */}
        <div style={{
          display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap",
          paddingBottom: 16, borderBottom: "1px solid rgba(51,65,85,0.15)",
        }}>
          {/* Source tabs */}
          <div style={{
            display: "flex", background: "rgba(15,23,42,0.5)", borderRadius: 9,
            border: "1px solid rgba(51,65,85,0.2)", padding: 3,
          }}>
            {SOURCES.map(s => (
              <button
                key={s}
                onClick={() => setActiveSource(s)}
                style={{
                  padding: "5px 13px", borderRadius: 7, fontSize: 12, fontWeight: 600,
                  border: "none", cursor: "pointer", fontFamily: "inherit", transition: "all .15s",
                  background: activeSource === s ? "rgba(49,185,123,0.12)" : "transparent",
                  color: activeSource === s ? "#4ade80" : "#64748b",
                }}
              >{s}</button>
            ))}
          </div>

          {/* Priority dropdown */}
          <div style={{ position: "relative" }}>
            <button
              onClick={() => setPrDropdown(!prDropdown)}
              style={{
                display: "flex", alignItems: "center", gap: 6, padding: "6px 12px",
                borderRadius: 8, fontSize: 12, fontWeight: 600, fontFamily: "inherit",
                background: "rgba(15,23,42,0.5)", border: "1px solid rgba(51,65,85,0.25)",
                color: "#64748b", cursor: "pointer",
              }}
            >
              {activePriority} <ChevronDownIcon />
            </button>
            {prDropdown && (
              <div style={{
                position: "absolute", top: "100%", left: 0, marginTop: 4, zIndex: 50,
                background: "#0f172a", border: "1px solid rgba(51,65,85,0.35)", borderRadius: 10,
                boxShadow: "0 12px 40px rgba(0,0,0,0.4)", overflow: "hidden", minWidth: 160,
              }}>
                {FILTER_PRIORITIES.map(p => (
                  <button
                    key={p}
                    onClick={() => { setActivePriority(p); setPrDropdown(false); }}
                    style={{
                      display: "block", width: "100%", padding: "8px 14px",
                      background: activePriority === p ? "rgba(51,65,85,0.2)" : "transparent",
                      border: "none", color: "#cbd5e1", fontSize: 12, cursor: "pointer",
                      fontFamily: "inherit", textAlign: "left", transition: "background .1s",
                    }}
                    onMouseEnter={e => { e.currentTarget.style.background = "rgba(51,65,85,0.25)"; }}
                    onMouseLeave={e => { e.currentTarget.style.background = activePriority === p ? "rgba(51,65,85,0.2)" : "transparent"; }}
                  >{p}</button>
                ))}
              </div>
            )}
          </div>

          {/* Search */}
          <div style={{
            display: "flex", alignItems: "center", gap: 8, padding: "6px 12px",
            borderRadius: 8, background: "rgba(15,23,42,0.5)", border: "1px solid rgba(51,65,85,0.25)",
            marginLeft: "auto", minWidth: 200, maxWidth: 280, flex: "0 1 auto",
          }}>
            <span style={{ color: "#334155", flexShrink: 0 }}><SearchIcon /></span>
            <input
              value={search}
              onChange={e => setSearch(e.target.value)}
              placeholder="Search issues..."
              style={{ flex: 1, background: "none", border: "none", outline: "none", color: "#e2e8f0", fontSize: 12, fontFamily: "inherit" }}
            />
          </div>
        </div>

        <div style={{
          display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12, paddingTop: 12,
        }}>
          <div style={{ fontSize: 11.5, color: "#64748b", lineHeight: 1.5 }}>
            {usingProviderLayout
              ? `This board mirrors observed ${displayProvider(selectedProvider!)} statuses for the current project. Empty provider states will appear once issues are synced into them.`
              : "This board uses the current project's configured columns. Edit it to remap raw provider statuses and move targets."}
          </div>
          <div style={{
            display: "flex", alignItems: "center", gap: 4,
            background: "rgba(15,23,42,0.5)", border: "1px solid rgba(51,65,85,0.2)",
            borderRadius: 10, padding: 4, flexShrink: 0,
          }}>
            <button
              onClick={() => setLayoutMode("project")}
              className={layoutMode === "project" ? "ib-layout-active" : undefined}
              style={{
                display: "flex", alignItems: "center", gap: 6, padding: "7px 11px",
                borderRadius: 8, border: "1px solid transparent", background: "transparent",
                color: "#94a3b8", fontSize: 12, fontWeight: 600, cursor: "pointer", fontFamily: "inherit",
              }}
            >
              <BoardIcon /> Project Board
            </button>
            <button
              onClick={() => setLayoutMode("provider")}
              className={layoutMode === "provider" ? "ib-layout-active" : undefined}
              style={{
                display: "flex", alignItems: "center", gap: 6, padding: "7px 11px",
                borderRadius: 8, border: "1px solid transparent", background: "transparent",
                color: "#94a3b8", fontSize: 12, fontWeight: 600, cursor: "pointer", fontFamily: "inherit",
              }}
            >
              <StackIcon /> Provider Statuses
            </button>
          </div>
        </div>

        {/* Per-provider sync row */}
        {syncStates.length > 0 && (
          <div style={{ display: "flex", alignItems: "center", gap: 14, paddingTop: 10, paddingBottom: 4, overflowX: "auto" }}>
            {syncStates.map(s => (
              <div key={s.provider} style={{ display: "flex", alignItems: "center", gap: 5, flexShrink: 0 }}>
                <span style={{ display: "inline-block", width: 6, height: 6, borderRadius: "50%", background: s.last_error ? "#EF4444" : "#31B97B" }} />
                <span style={{ fontSize: 10, color: "#475569" }}>{s.provider}</span>
                {s.last_synced_at && <span style={{ fontSize: 10, color: "#334155" }}>{formatRelative(s.last_synced_at)}</span>}
                {s.last_error && <span style={{ fontSize: 10, color: "#ef4444" }} title={s.last_error}>error</span>}
                <button
                  onClick={() => void handleSync(s.provider)}
                  disabled={!!syncing}
                  style={{
                    fontSize: 9, padding: "1px 5px", borderRadius: 3,
                    border: "1px solid rgba(51,65,85,0.25)", background: "transparent",
                    color: syncing ? "#334155" : "#475569", cursor: syncing ? "default" : "pointer",
                  }}
                >{syncing === s.provider ? "…" : "Sync"}</button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* ─── KANBAN BOARD ─── */}
      <div style={{ flex: 1, overflowX: "auto", padding: "16px 28px 28px", display: "flex", gap: 12 }}>
        {board ? (
          displayColumns.map(col => (
            <KanbanColumn
              key={col.id}
              column={col}
              selectedIssueId={selectedIssue ? compositeId(selectedIssue) : null}
              onSelectIssue={setSelectedIssue}
              onAddClick={() => setShowCreate(true)}
            />
          ))
        ) : (
          <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center", color: "#334155", fontSize: 13 }}>
            Loading…
          </div>
        )}
      </div>

      {/* ─── MODALS ─── */}
      <NewIssueModal
        open={showCreate}
        projectId={projectId}
        onClose={() => setShowCreate(false)}
        onCreated={() => { setShowCreate(false); refetch(); }}
      />

      <BoardEditorModal
        open={showBoardEditor}
        config={editorConfig}
        saving={savingBoard}
        projectId={projectId}
        onClose={() => { setShowBoardEditor(false); setBoardDraft(null); }}
        onChange={setBoardDraft}
        onSave={() => void handleSaveBoard()}
      />

      <IssueDrawer
        issue={selectedIssue}
        open={!!selectedIssue}
        projectId={projectId ?? ""}
        onClose={() => setSelectedIssue(null)}
        onDeleted={() => { setSelectedIssue(null); refetch(); }}
        onReopen={() => { refetch(); setSelectedIssue(null); }}
        onUpdated={(updated) => { setSelectedIssue(updated); refetch(); }}
      />
    </div>
  );
}
