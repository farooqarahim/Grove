import {
  abortRun,
  forkRunWorktree,
  gitCreatePr,
  gitGeneratePrContent,
  gitProjectCreatePr,
  gitProjectGeneratePrContent,
  gitProjectInit,
  gitProjectPull,
  gitProjectPush,
  gitProjectRevertAll,
  gitProjectSoftReset,
  gitProjectStageAll,
  gitProjectUnstageFiles,
  gitPull,
  gitPush,
  gitRevertAll,
  gitSoftReset,
  gitStageAll,
  gitUnstageFiles,
  mergeConversation,
} from "@/lib/api";
import type { MenuItem } from "@/components/ui/Menu";
import {
  Copy,
  ForkIcon,
  GitBranch,
  InfoCircle,
  Worktree,
} from "@/components/ui/icons";
import { qk } from "@/lib/queryKeys";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useState } from "react";
import type { BranchStatus, RunRecord } from "@/types";

interface UseGitActionsParams {
  latestRun: RunRecord | null;
  projectRoot: string | null;
  conversationId: string | null;
  branchStatus: BranchStatus | null;
  stagedPaths: string[];
  canRevert: boolean;
  canStage: boolean;
  canUnstage: boolean;
  changedFileCount: number;
  workspacePath: string | null;
  refetchIsRepo: () => void;
}

export function useGitActions({
  latestRun,
  projectRoot,
  conversationId,
  branchStatus,
  stagedPaths,
  canRevert,
  canStage,
  canUnstage,
  changedFileCount,
  workspacePath,
  refetchIsRepo,
}: UseGitActionsParams) {
  const queryClient = useQueryClient();

  const [pulling, setPulling] = useState(false);
  const [pushing, setPushing] = useState(false);
  const [creatingPr, setCreatingPr] = useState(false);
  const [pullError, setPullError] = useState<string | null>(null);
  const [undoError, setUndoError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [gitInitializing, setGitInitializing] = useState(false);
  const [confirmAbort, setConfirmAbort] = useState(false);
  const [aborting, setAborting] = useState(false);
  const [abortError, setAbortError] = useState<string | null>(null);

  useEffect(() => {
    setConfirmAbort(false);
    setAbortError(null);
  }, [conversationId, projectRoot]);

  const showToast = useCallback((msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(null), 3000);
  }, []);

  const refreshPanel = useCallback(() => {
    if (latestRun) {
      queryClient.invalidateQueries({ queryKey: qk.panelData(latestRun.id) }).catch(() => { });
      queryClient.invalidateQueries({ queryKey: qk.prStatus(latestRun.id) }).catch(() => { });
    }
    if (projectRoot) {
      queryClient.invalidateQueries({ queryKey: qk.projectPanelData(projectRoot) }).catch(() => { });
      queryClient.invalidateQueries({ queryKey: qk.projectPrStatus(projectRoot) }).catch(() => { });
      queryClient.invalidateQueries({ queryKey: qk.isGitRepo(projectRoot) }).catch(() => { });
    }
  }, [queryClient, latestRun, projectRoot]);

  const handleGitInit = async () => {
    if (!projectRoot || gitInitializing) return;
    setGitInitializing(true);
    try {
      await gitProjectInit(projectRoot);
      showToast("Git repository initialized");
      refetchIsRepo();
      refreshPanel();
    } catch (e) {
      showToast(`git init failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setGitInitializing(false);
    }
  };

  const handlePull = async () => {
    if (!latestRun && !projectRoot) return;
    setPulling(true);
    setPullError(null);
    try {
      if (latestRun) {
        await gitPull(latestRun.id);
      } else {
        await gitProjectPull(projectRoot!);
      }
      refreshPanel();
      showToast("Pulled successfully");
    } catch (e) {
      setPullError(e instanceof Error ? e.message : String(e));
    } finally {
      setPulling(false);
    }
  };

  const handlePush = async () => {
    if (!latestRun && !projectRoot) return;
    setPushing(true);
    try {
      if (latestRun) {
        await gitPush(latestRun.id);
      } else {
        await gitProjectPush(projectRoot!);
      }
      refreshPanel();
      showToast("Pushed successfully");
    } catch (e) {
      showToast(`Push failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setPushing(false);
    }
  };

  const handleUndo = async () => {
    if (!latestRun && !projectRoot) return;
    setUndoError(null);
    try {
      const result = latestRun
        ? await gitSoftReset(latestRun.id)
        : await gitProjectSoftReset(projectRoot!);
      refreshPanel();
      showToast(`Undid: ${result.subject}`);
    } catch (e) {
      setUndoError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleCreatePr = async () => {
    if (!latestRun && !projectRoot) return;
    setCreatingPr(true);
    try {
      const content = latestRun
        ? await gitGeneratePrContent(latestRun.id)
        : await gitProjectGeneratePrContent(projectRoot!);
      const pr = latestRun
        ? await gitCreatePr(latestRun.id, content.title, content.description)
        : await gitProjectCreatePr(projectRoot!, content.title, content.description);
      refreshPanel();
      if (pr.code === "PR_ALREADY_EXISTS") {
        showToast(`Existing PR ready: #${pr.number}`);
      } else {
        showToast(`PR #${pr.number} created`);
      }
    } catch (e) {
      showToast(`Create PR failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setCreatingPr(false);
    }
  };

  const handleRevertAll = () => {
    if (!canRevert) return;
    if (!window.confirm("Revert all changes? This cannot be undone.")) return;
    const revertPromise = latestRun
      ? gitRevertAll(latestRun.id)
      : projectRoot
        ? gitProjectRevertAll(projectRoot)
        : Promise.reject(new Error("No workspace available"));
    revertPromise
      .then(() => {
        refreshPanel();
        showToast("All changes reverted");
      })
      .catch((e) => showToast(`Revert failed: ${e instanceof Error ? e.message : String(e)}`));
  };

  const handleStageAll = () => {
    if (!canStage) return;
    const stagePromise = latestRun
      ? gitStageAll(latestRun.id)
      : projectRoot
        ? gitProjectStageAll(projectRoot)
        : Promise.reject(new Error("No workspace available"));
    stagePromise
      .then(() => {
        refreshPanel();
        showToast("All files staged");
      })
      .catch((e) => showToast(`Stage failed: ${e instanceof Error ? e.message : String(e)}`));
  };

  const handleUnstageAll = () => {
    if (!canUnstage || stagedPaths.length === 0) return;
    const unstagePromise = latestRun
      ? gitUnstageFiles(latestRun.id, stagedPaths)
      : projectRoot
        ? gitProjectUnstageFiles(projectRoot, stagedPaths)
        : Promise.reject(new Error("No workspace available"));
    unstagePromise
      .then(() => {
        refreshPanel();
        showToast("All staged files moved back to unstaged");
      })
      .catch((e) => showToast(`Unstage failed: ${e instanceof Error ? e.message : String(e)}`));
  };

  const handleAbort = async (activeRunId: string) => {
    if (!confirmAbort) {
      setConfirmAbort(true);
      setTimeout(() => setConfirmAbort(false), 4000);
      return;
    }
    setConfirmAbort(false);
    setAborting(true);
    setAbortError(null);
    try {
      await abortRun(activeRunId);
    } catch (e) {
      setAbortError(e instanceof Error ? e.message : String(e));
    } finally {
      setAborting(false);
    }
  };

  const worktreeMenuItems: MenuItem[] = [
    {
      icon: <Worktree size={12} />, label: "Open workspace in terminal",
      action: () => {
        if (!workspacePath) return;
        navigator.clipboard.writeText(`cd ${workspacePath}`).catch(() => { });
        showToast(`Copied: cd ${workspacePath}`);
      },
    },
    {
      icon: <ForkIcon size={12} />, label: "Fork to new worktree",
      action: async () => {
        if (!latestRun) return;
        try {
          const path = await forkRunWorktree(latestRun.id);
          navigator.clipboard.writeText(path).catch(() => { });
          showToast(`Forked: ${path}`);
        } catch (e) {
          showToast(`Fork failed: ${e instanceof Error ? e.message : String(e)}`);
        }
      },
    },
    {
      icon: <GitBranch size={12} />, label: "Merge conversation branch",
      action: async () => {
        if (!conversationId) return;
        if (!window.confirm("Merge this conversation branch using the project's configured merge strategy? Make sure all changes are committed first.")) return;
        try {
          const result = await mergeConversation(conversationId);
          if (result.outcome === "merged") {
            showToast(`Merged ${result.source_branch} into ${result.target_branch}`);
          } else if (result.outcome === "up_to_date") {
            showToast(`${result.source_branch} is up to date`);
          } else if (result.outcome === "conflict") {
            showToast(`Merge conflict in ${result.conflicting_files.length} file(s)`);
          } else if (result.pr_url) {
            showToast(`PR ready: ${result.pr_url}`);
          } else {
            showToast(`Processed ${result.source_branch} -> ${result.target_branch}`);
          }
        } catch (e) {
          showToast(`Merge failed: ${e instanceof Error ? e.message : String(e)}`);
        }
      },
    },
    { sep: true },
    {
      icon: <InfoCircle size={12} />, label: "View workspace status",
      action: () => {
        const branch = branchStatus?.branch ?? "unknown";
        const ahead = branchStatus?.ahead ?? 0;
        const behind = branchStatus?.behind ?? 0;
        showToast(`${branch} \u2022 ${changedFileCount} files \u2022 \u2191${ahead} \u2193${behind}`);
      },
    },
    {
      icon: <Copy size={12} />, label: "Copy workspace path",
      action: () => {
        if (!workspacePath) return;
        navigator.clipboard.writeText(workspacePath).catch(() => { });
        showToast("Path copied");
      },
    },
  ];

  return {
    // loading states
    pulling,
    pushing,
    creatingPr,
    gitInitializing,
    confirmAbort,
    aborting,
    // errors
    pullError,
    undoError,
    abortError,
    // toast
    toast,
    showToast,
    // actions
    refreshPanel,
    handleGitInit,
    handlePull,
    handlePush,
    handleUndo,
    handleCreatePr,
    handleRevertAll,
    handleStageAll,
    handleUnstageAll,
    handleAbort,
    // menu
    worktreeMenuItems,
  };
}
