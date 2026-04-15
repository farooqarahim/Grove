import { MainPanel } from "@/components/layout/MainPanel";
import { NavRail } from "@/components/layout/NavRail";
import { ResizableLayout } from "@/components/layout/ResizableLayout";
import { RightPanel } from "@/components/layout/RightPanel";
import { Sidebar } from "@/components/layout/Sidebar";
import { CommitModal } from "@/components/modals/CommitModal";
import { CreateProjectModal } from "@/components/modals/CreateProjectModal";
import { NewCliConversationModal } from "@/components/modals/NewCliConversationModal";
import { NewRunModal } from "@/components/modals/NewRunModal";
import { SessionNameModal } from "@/components/modals/SessionNameModal";
import { ReviewView } from "@/components/review/ReviewView";
import { GroveLogo } from "@/components/ui/GroveLogo";
import type { GitStatusEntry } from "@/lib/api";
import {
  getConversation,
  getFileDiff,
  getProjectPanelData,
  getRightPanelData,
  gitProjectDiff,
  gitProjectStatus,
  gitStatusDetailed,
  issueCountOpen,
  listAutomations,
  listProjects,
  publishChanges,
} from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { C } from "@/lib/theme";
import type { BranchStatus, FileDiffEntry, NavScreen, ReviewContext, RunRecord } from "@/types";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { UpdateBanner } from "@/components/UpdateBanner";
import { lazy, Suspense, useEffect, useState, type CSSProperties } from "react";
const DashboardScreen = lazy(() => import("@/components/screens/DashboardScreen").then(m => ({ default: m.DashboardScreen })));
const IssueBoardScreen = lazy(() => import("@/components/screens/IssueBoardScreen").then(m => ({ default: m.IssueBoardScreen })));
const SettingsScreen = lazy(() => import("@/components/screens/SettingsScreen").then(m => ({ default: m.SettingsScreen })));
const AutomationsScreen = lazy(() => import("@/components/screens/AutomationsScreen").then(m => ({ default: m.AutomationsScreen })));

function isInternalWorkspaceProject(project: { root_path: string }): boolean {
  return project.root_path.includes("/.grove/workspaces/");
}

export default function App() {
  const [screen, setScreen] = useState<NavScreen>("sessions");
  const [selectedConversationId, setSelectedConversationId] = useState<string | null>(null);
  const [showSessionName, setShowSessionName] = useState(false);
  const [pendingSessionName, setPendingSessionName] = useState<string | null>(null);
  const [showNewCliConversation, setShowNewCliConversation] = useState(false);
  const [showNewRun, setShowNewRun] = useState(false);
  // newRunConversationId: null = new conversation, string = run within existing conversation.
  const [newRunConversationId, setNewRunConversationId] = useState<string | null>(null);
  // newRunResumeRunId: set ONLY when "Continue Task" is clicked on a specific completed run.
  // When set, NewRunModal locks provider/model and resumes that run's provider thread.
  // "New Run" buttons always leave this null (fresh run, no thread resumption).
  const [newRunResumeRunId, setNewRunResumeRunId] = useState<string | null>(null);
  const [showCreateProject, setShowCreateProject] = useState(false);
  const [projectView, setProjectView] = useState<"home" | "settings">("home");
  const [showCommit, setShowCommit] = useState(false);
  const [showReview, setShowReview] = useState(false);
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [reviewFiles, setReviewFiles] = useState<FileDiffEntry[]>([]);
  const [reviewDiffs, setReviewDiffs] = useState<Record<string, string>>({});
  const [reviewSelectedFile, setReviewSelectedFile] = useState<string | null>(null);
  const [reviewGitStatus, setReviewGitStatus] = useState<GitStatusEntry[]>([]);
  const [reviewBranchStatus, setReviewBranchStatus] = useState<BranchStatus | null>(null);
  const [appToast, setAppToast] = useState<{ message: string; type: "success" | "error" } | null>(null);
  const [headerActionsHost, setHeaderActionsHost] = useState<HTMLDivElement | null>(null);
  const reviewCommitFiles = reviewFiles.filter(f => f.area !== "committed");

  // Opens the new-session modal, which now captures both name and conversation kind.
  const handleOpenNewRun = () => {
    if (selectedProject?.source_kind === "ssh") {
      showAppToast("SSH projects support shell access only. Agent runs still require a local checkout.", "error");
      return;
    }
    setNewRunConversationId(null);
    setShowSessionName(true);
  };

  // "Continue Task": opens modal pre-wired to resume that specific run's provider thread.
  // Provider/model are locked to the completed run's values.
  const handleContinueTask = (conversationId: string, runId: string) => {
    if (selectedProject?.source_kind === "ssh") {
      showAppToast("SSH projects support shell access only. Agent runs still require a local checkout.", "error");
      return;
    }
    setNewRunConversationId(conversationId);
    setNewRunResumeRunId(runId);
    setShowNewRun(true);
  };

  const showAppToast = (message: string, type: "success" | "error" = "success") => {
    setAppToast({ message, type });
    // Success toasts auto-dismiss; errors stay until user clicks to dismiss
    if (type !== "error") {
      setTimeout(() => setAppToast(null), 4000);
    }
  };

  // ── Global keyboard shortcuts ──────────────────────────────────────────
  useEffect(() => {
    const SCREEN_MAP: Record<string, NavScreen> = {
      "1": "dashboard",
      "2": "sessions",
      "3": "issues",
      "4": "automations",
      "5": "settings",
    };

    const handler = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;
      const tag = (e.target as HTMLElement)?.tagName;
      const isInput = tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";

      // Escape — close modals/dropdowns
      if (e.key === "Escape") {
        if (showCommit) { setShowCommit(false); e.preventDefault(); return; }
        if (showReview) { setShowReview(false); e.preventDefault(); return; }
        if (showSessionName) { setShowSessionName(false); e.preventDefault(); return; }
        if (showNewCliConversation) { setShowNewCliConversation(false); e.preventDefault(); return; }
        if (showNewRun) { setShowNewRun(false); e.preventDefault(); return; }
        if (showCreateProject) { setShowCreateProject(false); e.preventDefault(); return; }
        return;
      }

      if (!meta) return;

      // Cmd+K — focus search
      if (e.key === "k") {
        e.preventDefault();
        setScreen("sessions");
        // Focus the search input on next tick
        setTimeout(() => {
          const input = document.querySelector<HTMLInputElement>('[placeholder="Search..."]');
          input?.focus();
        }, 50);
        return;
      }

      // Cmd+N — open New Run modal (always new conversation)
      if (e.key === "n" && !isInput) {
        e.preventDefault();
        handleOpenNewRun();
        return;
      }

      // Cmd+1 through Cmd+5 — navigate screens
      const screenKey = SCREEN_MAP[e.key];
      if (screenKey && !isInput) {
        e.preventDefault();
        setScreen(screenKey);
        return;
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [showSessionName, showNewCliConversation, showNewRun, showCreateProject, showCommit, showReview]);

  const queryClient = useQueryClient();

  const { data: projects } = useQuery({
    queryKey: qk.projects(),
    queryFn: listProjects,
    refetchInterval: 60000,
    staleTime: 30000,
  });
  const { data: openIssueCount } = useQuery({
    queryKey: qk.openIssueCount(selectedProjectId),
    queryFn: () => selectedProjectId ? issueCountOpen(selectedProjectId) : Promise.resolve(0),
    refetchInterval: 60000,
    staleTime: 30000,
  });
  const { data: automationsList } = useQuery({
    queryKey: qk.automations(selectedProjectId),
    queryFn: () => selectedProjectId ? listAutomations(selectedProjectId) : Promise.resolve([]),
    refetchInterval: 60000,
    staleTime: 30000,
  });
  const automationCount = automationsList?.filter(a => a.enabled).length ?? 0;
  const { data: selectedConversation } = useQuery({
    queryKey: qk.conversation(selectedConversationId),
    queryFn: () => getConversation(selectedConversationId!),
    enabled: !!selectedConversationId,
    refetchInterval: 60000,
    staleTime: 30000,
  });

  useEffect(() => {
    const conversationProjectId = selectedConversation?.project_id ?? null;
    if (!conversationProjectId || conversationProjectId === selectedProjectId) return;
    setSelectedProjectId(conversationProjectId);
    localStorage.setItem("grove:last-project-id", conversationProjectId);
  }, [selectedConversation?.project_id, selectedProjectId]);

  useEffect(() => {
    if (!projects || projects.length === 0) return;

    const active = projects.filter(p => p.state === "active");
    if (active.length === 0) return;
    const selectable = active.filter(p => !isInternalWorkspaceProject(p));
    const preferred = selectable.length > 0 ? selectable : active;

    // Current selection is still valid — leave it alone.
    if (selectedProjectId && preferred.some(p => p.id === selectedProjectId)) return;

    // Prefer the last project the user explicitly opened.
    const stored = localStorage.getItem("grove:last-project-id");
    if (stored && preferred.some(p => p.id === stored)) {
      setSelectedProjectId(stored);
      return;
    }

    // Fallback: most recently touched (updated_at) active project.
    const recent = [...preferred].sort(
      (a, b) => new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
    );
    setSelectedProjectId(recent[0].id);
  }, [projects, selectedProjectId]);

  const selectedProject = projects?.find(p => p.id === selectedProjectId) ?? null;

  // Shared latest run — set by RightPanel via callback to avoid duplicate polling
  const [latestRun, setLatestRun] = useState<RunRecord | null>(null);

  const handleSelectProject = (id: string) => {
    setSelectedProjectId(id);
    localStorage.setItem("grove:last-project-id", id);
    setSelectedConversationId(null);
    setProjectView("home");
    setLatestRun(null); // clear stale run so RightPanel switches to project-based files
  };

  const handleProjectCreated = () => {
    void queryClient.invalidateQueries({ queryKey: qk.projects() });
  };

  const loadReviewContext = async (): Promise<void> => {
    if (!latestRun && selectedProject?.source_kind === "ssh") {
      throw new Error("SSH projects do not expose local git diffs.");
    }
    const projectRoot = selectedConversation?.conversation_kind === "cli" || selectedConversation?.conversation_kind === "hive_loom"
      ? selectedConversation.worktree_path
      : selectedProject?.root_path;
    if (latestRun) {
      const [data, status] = await Promise.all([
        getRightPanelData(latestRun.id),
        gitStatusDetailed(latestRun.id).catch(() => [] as GitStatusEntry[]),
      ]);
      setReviewFiles(data.files);
      setReviewDiffs(data.diffs);
      setReviewBranchStatus(data.branch);
      setReviewGitStatus(status);
      setReviewSelectedFile(null);
    } else if (projectRoot) {
      const [data, status] = await Promise.all([
        getProjectPanelData(projectRoot),
        gitProjectStatus(projectRoot).catch(() => [] as GitStatusEntry[]),
      ]);
      setReviewFiles(data.files);
      setReviewDiffs(data.diffs);
      setReviewBranchStatus(data.branch);
      setReviewGitStatus(status);
      setReviewSelectedFile(null);
    } else {
      setReviewFiles([]);
      setReviewGitStatus([]);
      setReviewDiffs({});
      setReviewBranchStatus(null);
      setReviewSelectedFile(null);
    }
  };

  const handleViewDiff = async (runId: string) => {
    try {
      const [data, status] = await Promise.all([
        getRightPanelData(runId),
        gitStatusDetailed(runId).catch(() => [] as GitStatusEntry[]),
      ]);
      setReviewFiles(data.files);
      setReviewDiffs(data.diffs);
      setReviewBranchStatus(data.branch);
      setReviewGitStatus(status);
      setReviewSelectedFile(null);
      setShowReview(true);
    } catch {
      setReviewFiles([]);
      setReviewGitStatus([]);
      setReviewDiffs({});
      setReviewBranchStatus(null);
      setReviewSelectedFile(null);
      setShowReview(true);
    }
  };

  const handleOpenReview = async () => {
    try {
      await loadReviewContext();
      setShowReview(true);
    } catch {
      setReviewFiles([]);
      setReviewGitStatus([]);
      setReviewDiffs({});
      setReviewBranchStatus(null);
      setReviewSelectedFile(null);
      setShowReview(true);
    }
  };

  const handleOpenCommit = async () => {
    try {
      await loadReviewContext();
    } catch {
      setReviewFiles([]);
      setReviewGitStatus([]);
      setReviewDiffs({});
      setReviewBranchStatus(null);
      setReviewSelectedFile(null);
    }
    setShowCommit(true);
  };

  const handleReviewSelectFile = async (path: string) => {
    setReviewSelectedFile(path);
    if (reviewDiffs[path]) return; // already cached
    if (!latestRun && selectedProject?.source_kind === "ssh") return;
    const workspaceRoot = selectedConversation?.conversation_kind === "cli" || selectedConversation?.conversation_kind === "hive_loom"
      ? selectedConversation.worktree_path
      : selectedProject?.root_path;
    if (!latestRun && !workspaceRoot) return;
    try {
      const fileEntry = reviewFiles.find(f => f.path === path);
      const diff = latestRun
        ? await getFileDiff(latestRun.id, path, fileEntry?.area)
        : await gitProjectDiff(workspaceRoot!, path);
      setReviewDiffs(prev => ({ ...prev, [path]: diff }));
    } catch {
      // leave cache entry absent so the file shows as empty diff
    }
  };

  const reviewBranch = selectedConversation?.branch_name ?? (selectedProject?.name ?? "main");

  const handleCommit = async (message: string, nextStep: string, includeUnstaged: boolean, prTitle?: string, prBody?: string) => {
    try {
      if (!latestRun && selectedProject?.source_kind === "ssh") {
        throw new Error("SSH projects do not support local commits.");
      }
      const workspaceRoot = selectedConversation?.conversation_kind === "cli" || selectedConversation?.conversation_kind === "hive_loom"
        ? selectedConversation.worktree_path
        : selectedProject?.root_path;
      if (!latestRun && !workspaceRoot) {
        throw new Error("No project selected");
      }

      const result = await publishChanges({
        runId: latestRun?.id,
        projectRoot: latestRun ? undefined : workspaceRoot!,
        step: nextStep as "commit" | "push" | "pr",
        message,
        includeUnstaged,
        prTitle: prTitle || undefined,
        prBody: prBody || (latestRun
          ? `Changes from Grove run ${latestRun.id.slice(0, 8)}\n\nObjective: ${latestRun.objective}`
          : undefined),
      });

      // Show appropriate toast based on what was achieved
      if (result.pr) {
        if (result.pr.code === "PR_ALREADY_EXISTS") {
          showAppToast(`Pushed to existing PR #${result.pr.number}`);
        } else {
          showAppToast(`PR #${result.pr.number} created`);
        }
      } else if (result.pushed) {
        showAppToast(`Pushed ${result.sha.slice(0, 7)} to ${result.branch}`);
      } else {
        showAppToast(`Committed: ${result.sha.slice(0, 7)}`);
      }
    } catch (e) {
      showAppToast(e instanceof Error ? e.message : String(e), "error");
    }
  };

  const isRunMode = !!latestRun;
  const activeProjectRoot = selectedConversation?.conversation_kind === "cli" || selectedConversation?.conversation_kind === "hive_loom"
    ? selectedConversation.worktree_path ?? null
    : selectedProject?.root_path ?? null;
  const hasUncommitted = reviewFiles.some(f => f.area !== "committed");
  const reviewContext: ReviewContext = {
    mode: isRunMode ? 'run' : 'project',
    runId: latestRun?.id ?? null,
    projectRoot: activeProjectRoot,
    files: reviewFiles,
    diffs: reviewDiffs,
    branch: reviewBranchStatus,
    capabilities: {
      canStage: hasUncommitted,
      canUnstage: reviewFiles.some(f => f.area === "staged"),
      canRevert: hasUncommitted,
      canCommit: hasUncommitted,
      canAiReview: isRunMode,
    },
  };

  const renderScreen = () => {
    switch (screen) {
      case "dashboard":
        return (
          <DashboardScreen
            onNavigate={setScreen}
            onNewRun={handleOpenNewRun}
            onCreateProject={() => setShowCreateProject(true)}
            selectedProjectId={selectedProjectId}
            onSelectConversation={(id) => {
              setSelectedConversationId(id);
              setScreen("sessions");
            }}
            onSelectProject={(id) => {
              handleSelectProject(id);
              setScreen("sessions");
            }}
          />
        );
      case "issues":
        return (
          <IssueBoardScreen
            projectId={selectedProjectId}
            projects={projects ?? []}
            onProjectChange={setSelectedProjectId}
          />
        );
      case "automations":
        return (
          <AutomationsScreen
            projectId={selectedProjectId}
            projects={projects ?? []}
          />
        );
      case "settings":
        return (
          <SettingsScreen
            onNavigate={setScreen}
            onCreateProject={() => setShowCreateProject(true)}
          />
        );
      case "sessions":
      default:
        return (
          <ResizableLayout
            sidebar={
              <Sidebar
                selectedConversationId={selectedConversationId}
                onSelectConversation={setSelectedConversationId}
                onNewRun={handleOpenNewRun}
                projects={projects ?? []}
                selectedProjectId={selectedProjectId}
                onSelectProject={handleSelectProject}
                onCreateProject={() => setShowCreateProject(true)}
                projectView={projectView}
                onSetProjectView={setProjectView}
              />
            }
            main={
              <MainPanel
                conversationId={selectedConversationId}
                selectedProject={selectedProject}
                projectView={projectView}
                onNewRun={() => {
                  if (selectedProject?.source_kind === "ssh") {
                    showAppToast("SSH projects support shell access only. Agent runs still require a local checkout.", "error");
                    return;
                  }
                  // "New Run" inside a conversation header → stays in same conversation.
                  // If no conversation is open yet, falls back to new session flow.
                  if (selectedConversationId) {
                    setNewRunConversationId(selectedConversationId);
                    setShowNewRun(true);
                  } else {
                    handleOpenNewRun();
                  }
                }}
                onSelectConversation={setSelectedConversationId}
                onContinueTask={handleContinueTask}
                onViewDiff={handleViewDiff}
              />
            }
            right={
              <RightPanel
                conversationId={selectedConversationId}
                projectRoot={
                  selectedProject?.source_kind === "ssh"
                    ? null
                    : selectedConversation?.conversation_kind === "cli" || selectedConversation?.conversation_kind === "hive_loom"
                      ? selectedConversation.worktree_path ?? null
                      : selectedProject?.root_path ?? null
                }
                conversationKind={selectedConversation?.conversation_kind ?? null}
                onOpenReview={handleOpenReview}
                onOpenCommit={handleOpenCommit}
                onLatestRun={setLatestRun}
                headerActionsHost={headerActionsHost}
              />
            }
          />
        );
    }
  };

  return (
    <div className="flex flex-col h-screen w-full overflow-hidden select-none"
      style={{ background: C.base, color: C.text2, fontSize: 12 }}>

      {/* Title bar */}
      <div
        className="flex items-center relative shrink-0"
        data-tauri-drag-region
        style={{
          height: 44,
          background: "rgb(17, 20, 25)",
          backdropFilter: "blur(8px)",
          WebkitBackdropFilter: "blur(8px)",
          paddingLeft: 80,
          paddingRight: 16,
          borderBottom: `1px solid rgba(0, 0, 0, 0.21)`,
        }}
      >
        <div style={{ width: 80, flexShrink: 0 }} />

        {/* Center — Grove logo */}
        <div
          className="absolute top-1/2 left-1/2"
          style={{ transform: "translate(-50%, -50%)", pointerEvents: "none" }}
        >
          <div className="flex items-center gap-2.5">
            <GroveLogo size={24} color={C.accent} />
            <span className="text-md font-semibold tracking-wide" style={{ color: C.text1 }}>
              Grove
            </span>
          </div>
        </div>

        <div
          ref={setHeaderActionsHost}
          className="ml-auto flex items-center gap-1.5"
          style={{
            position: "relative",
            zIndex: 2,
            maxWidth: "48%",
            overflowX: "auto",
            scrollbarWidth: "none",
            WebkitAppRegion: "no-drag",
          } as CSSProperties & { WebkitAppRegion: string }}
        />
      </div>

      <UpdateBanner />

      {/* Main layout: NavRail + Screen */}
      <div className="flex-1 flex overflow-hidden">
        <NavRail screen={screen} onNavigate={setScreen} openIssueCount={openIssueCount ?? 0} automationCount={automationCount} />
        <Suspense fallback={
          <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", color: C.text4, fontSize: 12 }}>
            Loading...
          </div>
        }>
          {renderScreen()}
        </Suspense>
      </div>

      <SessionNameModal
        open={showSessionName}
        onClose={() => {
          setShowSessionName(false);
          setPendingSessionName(null);
        }}
        onContinue={async (name, kind) => {
          setShowSessionName(false);
          setPendingSessionName(name);
          if (kind === "cli") {
            setShowNewCliConversation(true);
            return;
          }
          if (kind === "hive_loom") {
            const projectId = selectedConversation?.project_id ?? selectedProjectId;
            if (!projectId) {
              showAppToast("No project selected", "error");
              return;
            }
            try {
              const { createHiveLoomConversation } = await import("@/lib/api");
              const row = await createHiveLoomConversation(projectId, name);
              setSelectedConversationId(row.id);
              setPendingSessionName(null);
              setScreen("sessions");
              void queryClient.invalidateQueries({ queryKey: ["conversations"] });
            } catch (e) {
              showAppToast(e instanceof Error ? e.message : String(e), "error");
            }
            return;
          }
          setNewRunConversationId(null);
          setShowNewRun(true);
        }}
      />

      <NewCliConversationModal
        open={showNewCliConversation}
        onClose={() => {
          setShowNewCliConversation(false);
          setPendingSessionName(null);
        }}
        projectId={selectedConversation?.project_id ?? selectedProjectId}
        projects={projects ?? []}
        sessionName={pendingSessionName}
        onProjectChange={setSelectedProjectId}
        onCreated={(convId) => {
          setSelectedConversationId(convId);
          setPendingSessionName(null);
          setShowNewCliConversation(false);
          setScreen("sessions");
        }}
      />

      <NewRunModal
        open={showNewRun}
        onClose={() => {
          setShowNewRun(false);
          setNewRunConversationId(null);
          setNewRunResumeRunId(null);
          setPendingSessionName(null);
        }}
        conversationId={newRunConversationId}
        resumeFromRunId={newRunResumeRunId}
        projectId={selectedConversation?.project_id ?? selectedProjectId}
        projects={projects ?? []}
        onProjectChange={setSelectedProjectId}
        sessionName={pendingSessionName}
        onStarted={(convId) => {
          setSelectedConversationId(convId);
          setNewRunConversationId(null);
          setNewRunResumeRunId(null);
          setPendingSessionName(null);
          setScreen("sessions");
        }}
      />

      <CreateProjectModal
        open={showCreateProject}
        onClose={() => setShowCreateProject(false)}
        onCreated={handleProjectCreated}
      />

      <CommitModal
        open={showCommit}
        onClose={() => setShowCommit(false)}
        branch={reviewBranch}
        fileCount={reviewCommitFiles.length}
        additions={reviewCommitFiles.filter(f => f.status.charAt(0) === "A").length}
        removals={reviewCommitFiles.filter(f => f.status.charAt(0) === "D").length}
        runId={latestRun?.id ?? null}
        projectRoot={activeProjectRoot}
        onCommit={handleCommit}
      />

      <ReviewView
        open={showReview}
        onClose={() => setShowReview(false)}
        context={reviewContext}
        gitStatus={reviewGitStatus}
        insights={null}
        selectedFile={reviewSelectedFile}
        onSelectFile={handleReviewSelectFile}
        onCommit={() => { setShowReview(false); setShowCommit(true); }}
        onRefresh={handleOpenReview}
      />

      {/* Toast notification */}
      {appToast && (
        <div
          onClick={appToast.type === "error" ? () => setAppToast(null) : undefined}
          style={{
            position: "fixed", bottom: 24, left: "50%", transform: "translateX(-50%)",
            padding: "10px 20px", borderRadius: 8, zIndex: 300,
            maxWidth: 480,
            background: appToast.type === "error" ? "rgba(239,68,68,0.15)" : "rgba(49,185,123,0.15)",
            color: appToast.type === "error" ? "#EF4444" : "#31B97B",
            fontSize: 12, fontWeight: 500,
            cursor: appToast.type === "error" ? "pointer" : "default",
          }}
        >
          {appToast.message}
          {appToast.type === "error" && (
            <span style={{ marginLeft: 8, opacity: 0.6, fontSize: 10 }}>click to dismiss</span>
          )}
        </div>
      )}
    </div>
  );
}
