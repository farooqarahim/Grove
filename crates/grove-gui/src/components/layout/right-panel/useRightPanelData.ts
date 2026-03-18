import {
  getProjectPanelData,
  getRightPanelData,
  gitGetPrStatus,
  gitProjectGetPrStatus,
  gitProjectIsRepo,
  listRunsForConversation,
  listTasksForConversation,
} from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import type { PrStatus, RightPanelData } from "@/types";
import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";
import { buildFileTree } from "@/components/ui/FileTree";
import { ACTIVE_STATES, type GitSourceKind } from "./constants";

interface UseRightPanelDataParams {
  conversationId: string | null;
  projectRoot: string | null;
  conversationKind: "run" | "cli" | "hive_loom" | null;
  onLatestRun?: (run: import("@/types").RunRecord | null) => void;
}

export function useRightPanelData({ conversationId, projectRoot, conversationKind, onLatestRun }: UseRightPanelDataParams) {
  const isCliConversation = conversationKind === "cli" || conversationKind === "hive_loom";
  const { data: tasks } = useQuery({
    queryKey: qk.tasks(conversationId),
    queryFn: () => listTasksForConversation(conversationId!),
    enabled: !!conversationId && !isCliConversation,
    refetchInterval: 60000,
    staleTime: 30000,
  });

  const { data: runs } = useQuery({
    queryKey: qk.runsForConversation(conversationId),
    queryFn: () => listRunsForConversation(conversationId!),
    enabled: !!conversationId && !isCliConversation,
    refetchInterval: 60000,
    staleTime: 30000,
  });

  const latestRun = runs?.[0] ?? null;

  const gitSource: GitSourceKind = (() => {
    if (conversationId) {
      if (isCliConversation) return projectRoot ? "project" : "none";
      if (runs == null) return "loading";
      if (runs.length === 0) return "conversation-empty";
      return "run";
    }
    return projectRoot ? "project" : "none";
  })();

  useEffect(() => {
    onLatestRun?.(latestRun);
  }, [latestRun?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const filesActive = gitSource === "run" || gitSource === "project";
  const panelKey = gitSource === "run"
    ? qk.panelData(latestRun?.id ?? null)
    : qk.projectPanelData(projectRoot);

  const { data: panelData } = useQuery<RightPanelData | null>({
    queryKey: panelKey,
    queryFn: async (): Promise<RightPanelData | null> => {
      if (gitSource === "run") return getRightPanelData(latestRun!.id);
      if (gitSource === "project") {
        const d = await getProjectPanelData(projectRoot!);
        return { files: d.files, branch: d.branch, latest_commit: d.latest_commit, cwd: d.cwd, diffs: d.diffs };
      }
      return null;
    },
    enabled: filesActive,
    refetchInterval: filesActive ? 60000 : false,
    staleTime: 30000,
  });

  const files = panelData?.files ?? null;
  const branchStatus = panelData?.branch ?? null;
  const latestCommit = panelData?.latest_commit ?? null;

  const prEnabled = gitSource === "run" || gitSource === "project";
  const { data: prStatus } = useQuery<PrStatus | null>({
    queryKey: gitSource === "run"
      ? qk.prStatus(latestRun?.id ?? null)
      : qk.projectPrStatus(projectRoot),
    queryFn: () => {
      if (gitSource === "run" && latestRun) return gitGetPrStatus(latestRun.id);
      if (gitSource === "project" && projectRoot) return gitProjectGetPrStatus(projectRoot);
      return Promise.resolve(null);
    },
    enabled: prEnabled,
    refetchInterval: prEnabled ? 60000 : false,
    staleTime: 30000,
  });

  const { data: isGitRepo, refetch: refetchIsRepo } = useQuery<boolean>({
    queryKey: qk.isGitRepo(projectRoot),
    queryFn: () => gitProjectIsRepo(projectRoot!),
    enabled: !!projectRoot && gitSource !== "run",
    refetchInterval: 60000,
    staleTime: 30000,
  });

  // Computed file state
  const allChangedFiles = files?.filter((file) => !!file.status.trim()) ?? [];
  const changedFiles = prStatus?.state === "MERGED"
    ? allChangedFiles.filter((file) => file.area !== "committed")
    : allChangedFiles;
  const changedTree = buildFileTree(changedFiles);
  const totalAdded = changedFiles.filter(f => f.status.charAt(0) === "A").length;
  const totalModified = changedFiles.filter(f => f.status.charAt(0) === "M").length;
  const totalDeleted = changedFiles.filter(f => f.status.charAt(0) === "D").length;
  const stagedPaths = allChangedFiles.filter((f) => f.area === "staged").map((f) => f.path);
  const uncommittedFileCount = allChangedFiles.filter(f => f.area !== "committed").length;
  const committedFileCount = allChangedFiles.filter(f => f.area === "committed").length;
  const changedFileCount = changedFiles.length;
  const fileMetaByPath = new Map(changedFiles.map((file) => [file.path, file] as const));
  const hasCommittedBranchChanges = committedFileCount > 0;

  const hasGit = gitSource === "run" || gitSource === "project";
  const canCommit = hasGit && uncommittedFileCount > 0;
  const canRevert = hasGit && uncommittedFileCount > 0;
  const canStage = hasGit && (changedFiles.some(f => f.area === "unstaged" || f.area === "untracked") ?? false);
  const canUnstage = hasGit && stagedPaths.length > 0;
  const canReview = hasGit && changedFileCount > 0;

  const branchNeedsSync = gitSource === "run" && !!latestRun && !!branchStatus && (
    !branchStatus.remote_branch_exists
    || branchStatus.remote_registration_state === "failed"
    || branchStatus.ahead > 0
  );
  const projectBranchNeedsSync = gitSource === "project" && !!branchStatus && (
    !branchStatus.remote_branch_exists || branchStatus.ahead > 0
  );

  const workspacePath = panelData?.cwd ?? projectRoot ?? null;
  const activeRun = isCliConversation ? null : runs?.find(r => ACTIVE_STATES.includes(r.state));
  const queueCount = (!isCliConversation && conversationId && tasks)
    ? tasks.filter(t => t.state === "queued").length
    : 0;

  const showSyncAction = branchNeedsSync || projectBranchNeedsSync;
  const syncActionLabel = !branchStatus?.remote_branch_exists ? "Push Branch" : "Sync Branch";
  const showCreatePrAction = hasGit && hasCommittedBranchChanges && !prStatus;
  const showPrDetailsAction = !!prStatus;

  const syncSummary = branchStatus
    ? branchStatus.behind > 0
      ? `Behind ${branchStatus.behind}`
      : (branchNeedsSync || projectBranchNeedsSync)
        ? (!branchStatus.remote_branch_exists ? "Not pushed" : `Ahead ${branchStatus.ahead}`)
        : "Up to date"
    : "Status unavailable";

  const commitButtonLabel = canCommit
    ? "Commit"
    : committedFileCount > 0 && uncommittedFileCount === 0
      ? "Committed"
      : "Commit";

  return {
    gitSource,
    latestRun,
    runs,
    branchStatus,
    latestCommit,
    prStatus,
    isGitRepo,
    refetchIsRepo,
    panelData,
    // file state
    changedFiles,
    changedTree,
    changedFileCount,
    totalAdded,
    totalModified,
    totalDeleted,
    stagedPaths,
    uncommittedFileCount,
    committedFileCount,
    fileMetaByPath,
    // capabilities
    canCommit,
    canRevert,
    canStage,
    canUnstage,
    canReview,
    // branch/sync
    branchNeedsSync: branchNeedsSync || projectBranchNeedsSync,
    workspacePath,
    showSyncAction,
    syncActionLabel,
    showCreatePrAction,
    showPrDetailsAction,
    syncSummary,
    commitButtonLabel,
    // run/queue state
    activeRun,
    queueCount,
  };
}
