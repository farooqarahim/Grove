import { TaskList } from "@/components/queue/TaskList";
import { PrStatusSection } from "@/components/review/PrStatusSection";
import { Folder, GitBranch, Undo } from "@/components/ui/icons";
import { Menu } from "@/components/ui/Menu";
import { C } from "@/lib/theme";
import { useState } from "react";

import { ChangedFilesPanel } from "./right-panel/ChangedFilesPanel";
import type { RightPanelProps } from "./right-panel/constants";
import { EmptyState } from "./right-panel/EmptyState";
import { HeaderActions } from "./right-panel/HeaderActions";
import { useGitActions } from "./right-panel/useGitActions";
import { useRightPanelData } from "./right-panel/useRightPanelData";
import { WorkspaceInfo } from "./right-panel/WorkspaceInfo";

export type { RightPanelProps };

const SECTION_HEADER: React.CSSProperties = {
  fontSize: 10,
  fontWeight: 600,
  textTransform: "uppercase",
  letterSpacing: "0.06em",
  color: C.text4,
  padding: "10px 14px 6px",
  flexShrink: 0,
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
};

export function RightPanel({
  conversationId,
  projectRoot,
  conversationKind,
  onOpenReview,
  onOpenCommit,
  onLatestRun,
  headerActionsHost,
}: RightPanelProps) {
  const [worktreeMenu, setWorktreeMenu] = useState<{ top: number; left: number } | null>(null);

  const data = useRightPanelData({ conversationId, projectRoot, conversationKind, onLatestRun });
  const isCliConversation = conversationKind === "cli" || conversationKind === "hive_loom";

  const actions = useGitActions({
    latestRun: data.latestRun,
    projectRoot,
    conversationId,
    branchStatus: data.branchStatus,
    stagedPaths: data.stagedPaths,
    canRevert: data.canRevert,
    canStage: data.canStage,
    canUnstage: data.canUnstage,
    changedFileCount: data.changedFileCount,
    workspacePath: data.workspacePath,
    refetchIsRepo: data.refetchIsRepo,
  });

  const shouldShowHeaderActions =
    data.gitSource !== "none"
    && data.isGitRepo !== false
    && data.gitSource !== "loading"
    && data.gitSource !== "conversation-empty";

  return (
    <div className="w-full h-full flex flex-col" style={{ background: "rgb(17, 20, 25)" }}>
      {headerActionsHost && shouldShowHeaderActions && (
        <HeaderActions
          host={headerActionsHost}
          canRevert={data.canRevert}
          canStage={data.canStage}
          canUnstage={data.canUnstage}
          canCommit={data.canCommit}
          canReview={data.canReview}
          branchStatus={data.branchStatus}
          latestCommit={data.latestCommit}
          showSyncAction={data.showSyncAction}
          syncActionLabel={data.syncActionLabel}
          showCreatePrAction={data.showCreatePrAction}
          showPrDetailsAction={data.showPrDetailsAction}
          prStatus={data.prStatus ?? null}
          commitButtonLabel={data.commitButtonLabel}
          changedFileCount={data.changedFileCount}
          pulling={actions.pulling}
          pushing={actions.pushing}
          creatingPr={actions.creatingPr}
          onRefresh={actions.refreshPanel}
          onRevertAll={actions.handleRevertAll}
          onStageAll={actions.handleStageAll}
          onUnstageAll={actions.handleUnstageAll}
          onPull={actions.handlePull}
          onPush={actions.handlePush}
          onUndo={actions.handleUndo}
          onCreatePr={actions.handleCreatePr}
          onOpenCommit={onOpenCommit}
          onOpenReview={onOpenReview}
          onWorktreeMenu={() => setWorktreeMenu({ top: 72, left: window.innerWidth - 240 })}
        />
      )}

      {/* ── Git / Changes section ── */}
      <div style={{ flex: 7, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <div style={SECTION_HEADER}>
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span>Changes</span>
            {data.changedFileCount > 0 && (
              <span
                className="inline-flex items-center justify-center text-2xs font-bold"
                style={{
                  minWidth: 16,
                  height: 16,
                  padding: "0 4px",
                  borderRadius: 2,
                  background: C.blueDim,
                  color: C.blue,
                }}
              >
                {data.changedFileCount}
              </span>
            )}
          </div>
          <div style={{ display: "flex", gap: 8, fontSize: 10, flexShrink: 0 }}>
            {data.totalAdded > 0 && <span style={{ color: C.accent }}>+{data.totalAdded}</span>}
            {data.totalModified > 0 && <span style={{ color: C.blue }}>~{data.totalModified}</span>}
            {data.totalDeleted > 0 && <span style={{ color: C.danger }}>-{data.totalDeleted}</span>}
          </div>
        </div>

        <div style={{ flex: 1, minHeight: 0, display: "flex", flexDirection: "column", overflowY: "auto" }}>
          {data.gitSource === "none" ? (
            <EmptyState
              icon={<Folder size={18} />}
              title="No workspace selected"
              description="Pick a project or conversation to see changed files."
            />
          ) : data.isGitRepo === false ? (
            <EmptyState
              icon={<GitBranch size={18} />}
              title="No Git repository"
              description="This project folder is not tracked by Git yet."
              action={(
                <button
                  onClick={actions.handleGitInit}
                  disabled={actions.gitInitializing}
                  style={{
                    height: 32,
                    padding: "0 12px",
                    borderRadius: 2,
                    border: "none",
                    background: actions.gitInitializing ? C.surfaceHover : C.accent,
                    color: actions.gitInitializing ? C.text4 : C.base,
                    fontSize: 11,
                    fontWeight: 700,
                    cursor: actions.gitInitializing ? "default" : "pointer",
                  }}
                >
                  {actions.gitInitializing ? "Initializing..." : "Initialize Git"}
                </button>
              )}
            />
          ) : data.gitSource === "loading" ? (
            <div style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 6 }}>
              <div className="skeleton" style={{ width: 120, height: 10, borderRadius: 4 }} />
              <div className="skeleton" style={{ width: 90, height: 10, borderRadius: 4 }} />
            </div>
          ) : data.gitSource === "conversation-empty" ? (
            <EmptyState
              icon={<Folder size={18} />}
              title="No sessions yet"
              description="Start a run in this conversation to populate the change panel."
            />
          ) : (
            <>
              <WorkspaceInfo
                branchStatus={data.branchStatus}
                latestCommit={data.latestCommit}
                syncSummary={data.syncSummary}
                branchNeedsSync={data.branchNeedsSync}
                pullError={actions.pullError}
                undoError={actions.undoError}
              />

              {data.prStatus && (data.latestRun || data.gitSource === "project") && (
                <>
                  <div style={{ margin: "0 16px", height: 1, background: `${C.border}66` }} />
                  <div style={{ padding: 16 }}>
                    <PrStatusSection
                      prStatus={data.prStatus}
                      runId={data.latestRun?.id}
                      projectRoot={data.gitSource === "project" ? projectRoot ?? undefined : undefined}
                      onMerged={() => {
                        actions.refreshPanel();
                        actions.showToast("PR merged successfully");
                      }}
                    />
                  </div>
                </>
              )}

              <div style={{ margin: "0 16px", height: 1, background: `${C.border}66` }} />

              <ChangedFilesPanel
                changedTree={data.changedTree}
                changedFileCount={data.changedFileCount}
                fileMetaByPath={data.fileMetaByPath}
                gitSource={data.gitSource}
              />

              {data.activeRun && (
                <div style={{ padding: "0 10px 10px", flexShrink: 0 }}>
                  <button
                    onClick={() => actions.handleAbort(data.activeRun!.id)}
                    disabled={actions.aborting}
                    style={{
                      width: "100%",
                      height: 34,
                      borderRadius: 2,
                      border: "none",
                      background: actions.confirmAbort ? C.danger : C.dangerDim,
                      color: actions.confirmAbort ? "#fff" : C.danger,
                      fontSize: 11,
                      fontWeight: 700,
                      cursor: actions.aborting ? "default" : "pointer",
                      opacity: actions.aborting ? 0.5 : 1,
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      gap: 5,
                    }}
                  >
                    <Undo size={11} />
                    {actions.aborting ? "Aborting..." : actions.confirmAbort ? "Confirm Abort" : "Abort Active Run"}
                  </button>
                  {actions.abortError && (
                    <div style={{ marginTop: 6, fontSize: 10, color: C.danger, textAlign: "center" }}>
                      {actions.abortError}
                    </div>
                  )}
                </div>
              )}
            </>
          )}
        </div>
      </div>

      {!isCliConversation && (
        <>
      {/* ── Divider ── */}
      <div style={{ height: 1, background: C.border, flexShrink: 0 }} />

      {/* ── Queue section ── */}
      <div style={{ flex: 3, minHeight: 0, display: "flex", flexDirection: "column" }}>
        <div style={SECTION_HEADER}>
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span>Queue</span>
            {data.queueCount > 0 && (
              <span
                className="inline-flex items-center justify-center text-2xs font-bold"
                style={{
                  minWidth: 16,
                  height: 16,
                  padding: "0 4px",
                  borderRadius: 2,
                  background: C.warnDim,
                  color: "#F59E0B",
                }}
              >
                {data.queueCount}
              </span>
            )}
          </div>
        </div>

        <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>
          <TaskList conversationId={conversationId} />
        </div>
      </div>
        </>
      )}

      {worktreeMenu && (
        <Menu items={actions.worktreeMenuItems} onClose={() => setWorktreeMenu(null)} position={worktreeMenu} />
      )}

      {actions.toast && (
        <div style={{
          position: "absolute",
          bottom: 48,
          left: 12,
          right: 12,
          padding: "8px 12px",
          borderRadius: 2,
          background: C.accentDim,
          color: C.accent,
          fontSize: 11,
          textAlign: "center",
          zIndex: 100,
        }}>
          {actions.toast}
        </div>
      )}
    </div>
  );
}
