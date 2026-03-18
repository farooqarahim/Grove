import {
  Commit,
  Download,
  FileMinus,
  Maximize,
  Plus,
  PullRequest,
  Refresh,
  Trash,
  Undo,
  Upload,
  Worktree,
} from "@/components/ui/icons";
import { createPortal } from "react-dom";
import { HeaderBarButton } from "./HeaderBarButton";
import { openUrl } from "./constants";
import type { BranchStatus, GitLogEntry, PrStatus } from "@/types";

interface HeaderActionsProps {
  host: HTMLElement;
  // capabilities
  canRevert: boolean;
  canStage: boolean;
  canUnstage: boolean;
  canCommit: boolean;
  canReview: boolean;
  // branch/sync
  branchStatus: BranchStatus | null;
  latestCommit: GitLogEntry | null;
  showSyncAction: boolean;
  syncActionLabel: string;
  showCreatePrAction: boolean;
  showPrDetailsAction: boolean;
  prStatus: PrStatus | null;
  commitButtonLabel: string;
  changedFileCount: number;
  // loading states
  pulling: boolean;
  pushing: boolean;
  creatingPr: boolean;
  // actions
  onRefresh: () => void;
  onRevertAll: () => void;
  onStageAll: () => void;
  onUnstageAll: () => void;
  onPull: () => void;
  onPush: () => void;
  onUndo: () => void;
  onCreatePr: () => void;
  onOpenCommit?: () => void;
  onOpenReview?: () => void;
  onWorktreeMenu: () => void;
}

export function HeaderActions({
  host,
  canRevert,
  canStage,
  canUnstage,
  canCommit,
  canReview,
  branchStatus,
  latestCommit,
  showSyncAction,
  syncActionLabel,
  showCreatePrAction,
  showPrDetailsAction,
  prStatus,
  commitButtonLabel,
  changedFileCount,
  pulling,
  pushing,
  creatingPr,
  onRefresh,
  onRevertAll,
  onStageAll,
  onUnstageAll,
  onPull,
  onPush,
  onUndo,
  onCreatePr,
  onOpenCommit,
  onOpenReview,
  onWorktreeMenu,
}: HeaderActionsProps) {
  return createPortal(
    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
      <HeaderBarButton
        icon={<Refresh size={12} />}
        title="Refresh changes"
        compact
        onClick={onRefresh}
      />
      <HeaderBarButton
        icon={<Trash size={12} />}
        title="Revert all changes"
        tone="danger"
        compact
        disabled={!canRevert}
        onClick={onRevertAll}
      />
      <HeaderBarButton
        icon={<Plus size={12} />}
        title="Stage all changes"
        tone="success"
        compact
        disabled={!canStage}
        onClick={onStageAll}
      />
      <HeaderBarButton
        icon={<FileMinus size={12} />}
        title="Unstage all staged files"
        tone="neutral"
        compact
        disabled={!canUnstage}
        onClick={onUnstageAll}
      />
      {branchStatus && branchStatus.behind > 0 && (
        <HeaderBarButton
          icon={<Download size={12} />}
          label={pulling ? "Pulling..." : "Pull"}
          tone="neutral"
          disabled={pulling}
          onClick={onPull}
        />
      )}
      <HeaderBarButton
        icon={<Commit size={12} />}
        label={commitButtonLabel}
        tone="primary"
        disabled={!canCommit}
        onClick={onOpenCommit}
      />
      <HeaderBarButton
        icon={<Maximize size={11} />}
        label={`Review ${changedFileCount > 0 ? changedFileCount : ""}`.trim()}
        disabled={!canReview}
        onClick={canReview ? onOpenReview : undefined}
      />
      {showSyncAction && (
        <HeaderBarButton
          icon={<Upload size={12} />}
          label={pushing ? "Syncing..." : syncActionLabel}
          tone="success"
          disabled={pushing}
          onClick={onPush}
        />
      )}
      {latestCommit && !latestCommit.is_pushed && (
        <HeaderBarButton
          icon={<Undo size={12} />}
          label="Undo Commit"
          tone="neutral"
          onClick={onUndo}
        />
      )}
      {showPrDetailsAction && prStatus && (
        <HeaderBarButton
          icon={<PullRequest size={12} />}
          label="PR Details"
          tone="info"
          onClick={() => { void openUrl(prStatus.url); }}
        />
      )}
      {showCreatePrAction && (
        <HeaderBarButton
          icon={<PullRequest size={12} />}
          label={creatingPr ? "Creating..." : "Create PR"}
          tone="info"
          disabled={creatingPr}
          onClick={onCreatePr}
        />
      )}
      <HeaderBarButton
        icon={<Worktree size={12} />}
        title="Workspace actions"
        compact
        onClick={onWorktreeMenu}
      />
    </div>,
    host,
  );
}
