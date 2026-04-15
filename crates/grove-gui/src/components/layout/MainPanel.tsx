import { SessionSettingsModal } from "@/components/conversations/ConversationActions";
import { GraphPanel } from "@/components/grove-graph/GraphPanel";
import { ProjectHome } from "@/components/layout/ProjectHome";
import { RealTerminal } from "@/components/runs/RealTerminal";
import { RunCard } from "@/components/runs/RunCard";
import { ProjectSettingsPanel } from "@/components/settings/ProjectSettingsPanel";
import { TerminalColumn } from "@/components/terminal";
import { ConversationThread } from "@/components/thread/ConversationThread";
import { StatusTag } from "@/components/ui/badge";
import { Gear, Layers, Pulse, Sparkles } from "@/components/ui/icons";
import { getConversation, listProjects, listRunsForConversation } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { C } from "@/lib/theme";
import type { ProjectRow } from "@/types";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

interface MainPanelProps {
  conversationId: string | null;
  selectedProject: ProjectRow | null;
  projectView?: "home" | "settings";
  onNewRun: () => void;
  onSelectConversation?: (id: string) => void;
  // "Continue Task" on a finished run opens modal pre-wired to resume that specific run's thread.
  onContinueTask: (conversationId: string, runId: string) => void;
  onViewDiff?: (runId: string) => void;
}

export function MainPanel({ conversationId, selectedProject, projectView = "home", onNewRun, onSelectConversation, onContinueTask, onViewDiff }: MainPanelProps) {
  const [expandedRunId, setExpandedRunId] = useState<string | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [viewMode, setViewMode] = useState<"thread" | "cards">("cards");

  // Fade-in on conversation switch
  const [opacity, setOpacity] = useState(1);
  const prevConvRef = useRef(conversationId);
  useEffect(() => {
    if (prevConvRef.current !== conversationId && conversationId) {
      setOpacity(0);
      const frame = requestAnimationFrame(() => setOpacity(1));
      return () => cancelAnimationFrame(frame);
    }
    prevConvRef.current = conversationId;
  }, [conversationId]);

  const { data: conversation, refetch: refetchConversation } = useQuery({
    queryKey: qk.conversation(conversationId),
    queryFn: () => getConversation(conversationId!),
    enabled: !!conversationId,
    refetchInterval: 60000,
    staleTime: 30000,
  });
  const { data: runs } = useQuery({
    queryKey: qk.runsForConversation(conversationId),
    queryFn: () => listRunsForConversation(conversationId!),
    enabled: !!conversationId,
    refetchInterval: 60000,
    staleTime: 30000,
  });
  const { data: projects } = useQuery({
    queryKey: qk.projects(),
    queryFn: listProjects,
    refetchInterval: 60000,
    staleTime: 30000,
  });

  const projectRoot = projects?.find(p => p.id === conversation?.project_id)?.root_path;

  // Track visited CLI conversations so their terminals stay mounted (never destroyed).
  const [visitedCli, setVisitedCli] = useState<Map<string, string | undefined>>(new Map());
  const isCliConversation = conversation?.conversation_kind === "cli";

  useEffect(() => {
    if (conversationId && isCliConversation) {
      setVisitedCli((prev) => {
        if (prev.has(conversationId)) return prev;
        const next = new Map(prev);
        next.set(conversationId, conversation?.worktree_path ?? projectRoot ?? undefined);
        return next;
      });
    }
  }, [conversationId, isCliConversation, conversation?.worktree_path, projectRoot]);

  const latestRun = runs?.[0];

  const activeStates = ["executing", "waiting_for_gate", "planning", "verifying", "publishing", "merging"];
  const activeRun = runs?.find(r => activeStates.includes(r.state));
  const hasRunning = !!activeRun;


  if (!conversationId) {
    if (selectedProject?.source_kind === "ssh") {
      const host = selectedProject.source_details?.ssh_host ?? "";
      const user = selectedProject.source_details?.ssh_user ?? "";
      const port = selectedProject.source_details?.ssh_port ?? 22;
      const remotePath = selectedProject.source_details?.ssh_remote_path ?? "";
      const target = user ? `${user}@${host}` : host;

      if (!host || !remotePath) {
        return (
          <div className="h-full flex flex-col items-center justify-center gap-3" style={{ color: C.text4 }}>
            <div className="text-lg font-medium" style={{ color: C.text3 }}>SSH project is missing connection details</div>
            <div className="text-sm">Recreate the project with a host and remote path.</div>
          </div>
        );
      }

      return (
        <div className="flex-1 min-h-0 flex flex-col" style={{ background: C.base }}>
          <div
            className="flex items-center justify-between shrink-0"
            style={{
              padding: "14px 24px",
              background: C.surface,
              borderBottom: `1px solid ${C.border}`,
            }}
          >
            <div>
              <div className="text-lg font-bold" style={{ color: C.text1 }}>
                {selectedProject.name ?? remotePath.split("/").pop() ?? "SSH Project"}
              </div>
              <div style={{ fontSize: 11, color: C.text4 }}>
                SSH shell: {target}:{port} | {remotePath}
              </div>
            </div>
            <div style={{ fontSize: 11, color: C.text4 }}>
              Agent runs are disabled for SSH projects.
            </div>
          </div>
          <div className="flex-1 min-h-0">
            <RealTerminal
              conversationId={`project:${selectedProject.id}`}
              ssh={{ target, port, remotePath }}
            />
          </div>
        </div>
      );
    }

    if (selectedProject) {
      if (projectView === "settings") {
        return (
          <div style={{ flex: 1, overflowY: "auto", background: "#0e0f11", fontFamily: "'Geist', 'DM Sans', -apple-system, sans-serif" }}>
            <div style={{ maxWidth: 640, margin: "0 auto", padding: "32px 28px 48px" }}>
              <div style={{ marginBottom: 28 }}>
                <h1 style={{ fontSize: 22, fontWeight: 700, color: "rgba(255,255,255,0.88)", margin: "0 0 4px", letterSpacing: "-0.02em" }}>
                  Project Settings
                </h1>
                <div style={{ fontSize: 11.5, color: "rgba(255,255,255,0.28)", fontFamily: C.mono }}>
                  {selectedProject.name ?? selectedProject.root_path.split("/").pop()}
                </div>
              </div>
              <ProjectSettingsPanel
                projectId={selectedProject.id}
                projectName={selectedProject.name}
                rootPath={selectedProject.root_path}
              />
            </div>
          </div>
        );
      }

      return (
        <ProjectHome
          project={selectedProject}
          onNewRun={onNewRun}
          onSelectConversation={onSelectConversation ?? (() => {})}
        />
      );
    }

    return (
      <div
        className="h-full flex flex-col items-center justify-center gap-5"
        style={{ color: C.text4 }}
      >
        <div
          className="flex items-center justify-center rounded"
          style={{ width: 56, height: 56, background: C.accentDim }}
        >
          <span className="text-xl font-black" style={{ color: `${C.accent}50` }}>G</span>
        </div>
        <div className="text-center">
          <div className="text-lg font-medium mb-1" style={{ color: C.text3 }}>Select a project</div>
          <div className="text-sm" style={{ color: C.text4 }}>Choose a project from the sidebar to get started</div>
        </div>
      </div>
    );
  }

  if (isCliConversation) {
    return (
      <div
        className="flex-1 flex flex-col min-w-0 min-h-0 overflow-hidden"
        style={{ background: C.base, opacity, transition: "opacity 120ms ease-in" }}
      >
        <div
          className="flex items-center justify-between shrink-0"
          style={{ padding: "12px 16px", borderBottom: `1px solid ${C.border}` }}
        >
          <div>
            <div className="flex items-center gap-2.5">
              <h1 className="text-lg font-semibold" style={{ color: C.text1, margin: 0 }}>
                {conversation.title || `Session ${conversationId.slice(0, 8)}`}
              </h1>
              <StatusTag status={conversation.state ?? "active"} />
              <span
                style={{
                  fontSize: 10,
                  fontWeight: 700,
                  letterSpacing: "0.06em",
                  textTransform: "uppercase",
                  color: C.blue,
                  background: C.blueDim,
                  padding: "2px 8px",
                  borderRadius: 2,
                }}
              >
                CLI
              </span>
            </div>
            <p
              style={{
                fontSize: 12,
                color: C.text4,
                marginTop: 4,
                marginBottom: 0,
                fontFamily: C.mono,
                opacity: 0.7,
              }}
            >
              {(conversation.cli_provider ?? "cli").replaceAll("_", " ")}
              {conversation.cli_model ? ` · ${conversation.cli_model}` : ""}
              {conversation.branch_name ? ` · ${conversation.branch_name}` : ""}
            </p>
          </div>
          <div className="flex items-center gap-1">
            <button
              onClick={() => setShowSettings(true)}
              className="btn-ghost flex items-center justify-center cursor-pointer"
              style={{
                width: 30,
                height: 30,
                borderRadius: 2,
                background: "transparent",
                border: "none",
                color: C.text3,
              }}
              title="Session settings"
            >
              <Gear size={14} />
            </button>
          </div>
        </div>

        {/* All visited CLI terminals stay mounted; only active one is visible */}
        {Array.from(visitedCli.entries()).map(([convId, cwd]) => (
          <TerminalColumn
            key={convId}
            conversationId={convId}
            cwd={cwd}
            visible={convId === conversationId}
          />
        ))}

        {showSettings && conversation && (
          <SessionSettingsModal
            conversation={conversation}
            onClose={() => setShowSettings(false)}
            onUpdated={refetchConversation}
            onDeleted={() => setShowSettings(false)}
          />
        )}
      </div>
    );
  }

  if (conversation?.conversation_kind === "hive_loom") {
    return (
      <div
        className="flex-1 flex flex-col min-w-0 min-h-0 overflow-hidden"
        style={{ background: C.base, opacity, transition: "opacity 120ms ease-in" }}
      >
        <div
          className="flex items-center justify-between shrink-0"
          style={{ padding: "12px 16px", borderBottom: `1px solid ${C.border}` }}
        >
          <div>
            <div className="flex items-center gap-2.5">
              <h1 className="text-lg font-semibold" style={{ color: C.text1, margin: 0 }}>
                {conversation.title || `Session ${conversationId!.slice(0, 8)}`}
              </h1>
              <StatusTag status={conversation.state ?? "active"} />
              <span
                style={{
                  fontSize: 10,
                  fontWeight: 700,
                  letterSpacing: "0.06em",
                  textTransform: "uppercase",
                  color: "#F59E0B",
                  background: "rgba(245,158,11,0.12)",
                  padding: "2px 8px",
                  borderRadius: 2,
                }}
              >
                HIVE
              </span>
            </div>
            {conversation.branch_name && (
              <p
                style={{
                  fontSize: 12,
                  color: C.text4,
                  marginTop: 4,
                  marginBottom: 0,
                  fontFamily: C.mono,
                  opacity: 0.7,
                }}
              >
                {conversation.branch_name}
              </p>
            )}
          </div>
          <div className="flex items-center gap-1">
            <button
              onClick={() => setShowSettings(true)}
              className="btn-ghost flex items-center justify-center cursor-pointer"
              style={{
                width: 30,
                height: 30,
                borderRadius: 2,
                background: "transparent",
                border: "none",
                color: C.text3,
              }}
              title="Session settings"
            >
              <Gear size={14} />
            </button>
          </div>
        </div>

        <GraphPanel conversationId={conversationId!} />

        {showSettings && conversation && (
          <SessionSettingsModal
            conversation={conversation}
            onClose={() => setShowSettings(false)}
            onUpdated={refetchConversation}
            onDeleted={() => setShowSettings(false)}
          />
        )}
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col min-w-0 min-h-0 overflow-hidden" style={{ background: C.base, opacity, transition: "opacity 120ms ease-in" }}>

      {/* Session header */}
      <div
        className="flex items-center justify-between shrink-0"
        style={{ padding: "16px 24px" }}
      >
        <div>
          <div className="flex items-center gap-2.5">
            <h1 className="text-lg font-semibold" style={{ color: C.text1, margin: 0 }}>
              {conversation?.title || `Session ${conversationId.slice(0, 8)}`}
            </h1>
            {hasRunning ? (
              <span
                className="inline-flex items-center gap-1.5"
                style={{
                  fontSize: 11, fontWeight: 500,
                  background: `${C.accent}1A`, color: C.accent,
                  padding: "2px 10px", borderRadius: 2,
                }}
              >
                <span className="relative flex" style={{ width: 6, height: 6 }}>
                  <span
                    className="animate-ping absolute inline-flex rounded-full"
                    style={{
                      width: "100%", height: "100%",
                      background: C.accent, opacity: 0.75,
                    }}
                  />
                  <span
                    className="relative inline-flex rounded-full"
                    style={{ width: 6, height: 6, background: C.accent }}
                  />
                </span>
                Active
              </span>
            ) : (
              <StatusTag status={conversation?.state ?? "active"} />
            )}
          </div>
          <p
            style={{
              fontSize: 12, color: C.text4, marginTop: 4, marginBottom: 0,
              fontFamily: C.mono, opacity: 0.6,
            }}
          >
            {runs?.length ?? 0} runs{latestRun ? ` · ${conversation?.branch_name ?? "—"}` : ""}
          </p>
        </div>
        <div className="flex items-center gap-1">
          {/* Thread / Cards view toggle */}
          <button
            onClick={() => setViewMode(viewMode === "thread" ? "cards" : "thread")}
            className="btn-ghost flex items-center justify-center cursor-pointer"
            title={viewMode === "thread" ? "Switch to card view" : "Switch to thread view"}
            style={{
              padding: 6, borderRadius: 2,
              background: viewMode === "thread" ? "rgba(255,255,255,0.04)" : "transparent",
              color: viewMode === "thread" ? C.text1 : C.text3,
              border: "none",
            }}
          >
            <Layers size={14} />
          </button>
          {conversation && (
            <button
              onClick={() => setShowSettings(true)}
              className="btn-ghost flex items-center justify-center cursor-pointer"
              style={{
                width: 30, height: 30, borderRadius: 2,
                background: "transparent", border: "none",
                color: C.text3,
              }}
            >
              <Gear size={14} />
            </button>
          )}
          <button
            onClick={onNewRun}
            style={{
              display: "flex", alignItems: "center", gap: 7,
              padding: "6px 14px", borderRadius: 6, cursor: "pointer",
              border: "1px solid rgba(62,207,142,0.24)",
              background: "rgba(62,207,142,0.1)", color: C.accent,
              fontSize: 12, fontWeight: 600, fontFamily: "inherit",
              transition: "background 0.14s, border-color 0.14s",
            }}
            onMouseEnter={e => {
              e.currentTarget.style.background = "rgba(62,207,142,0.17)";
              e.currentTarget.style.borderColor = "rgba(62,207,142,0.38)";
            }}
            onMouseLeave={e => {
              e.currentTarget.style.background = "rgba(62,207,142,0.1)";
              e.currentTarget.style.borderColor = "rgba(62,207,142,0.24)";
            }}
          >
            <Sparkles size={12} /> New Run
          </button>
        </div>
      </div>

      {/* Active run banner */}
      {activeRun && (
        <div
          className="flex items-center gap-2.5"
          style={{
            padding: "8px 24px",
            borderBottom: `1px solid ${C.blueDim}`,
            background: C.blueMuted,
          }}
        >
          <Pulse color={C.blue} size={5} />
          <span style={{ fontSize: 11, fontWeight: 600, color: C.blue, flexShrink: 0 }}>
            {activeRun.state.charAt(0).toUpperCase() + activeRun.state.slice(1)}
          </span>
          <span
            style={{ fontSize: 11, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", color: C.text2 }}
          >
            {activeRun.objective}
          </span>
        </div>
      )}

      {/* Main content: Thread or Cards view */}
      {viewMode === "thread" ? (
        <div className="flex-1 min-h-0 overflow-hidden">
          <ConversationThread
            conversationId={conversationId}
          />
        </div>
      ) : (
        <div className="smooth-scroll flex-1 overflow-y-auto" style={{ padding: "0 18px" }}>
          {/* Loading skeleton */}
          {!runs && (
            <div>
              {[1, 2, 3].map(i => (
                <div key={i} style={{ display: "flex", alignItems: "center", gap: 10, padding: "7px 14px", borderBottom: `1px solid ${C.border}` }}>
                  <div className="skeleton" style={{ width: 6, height: 6, borderRadius: "50%", flexShrink: 0 }} />
                  <div className="skeleton" style={{ width: 20, height: 10, borderRadius: 2 }} />
                  <div className="skeleton" style={{ height: 10, flex: 1, borderRadius: 2 }} />
                  <div className="skeleton" style={{ width: 60, height: 10, borderRadius: 2 }} />
                </div>
              ))}
            </div>
          )}
          {runs && runs.length === 0 && (
            <div className="text-center" style={{ padding: "56px 12px" }}>
              <div style={{ fontSize: 13, color: C.text4, marginBottom: 12 }}>
                No runs in this session yet
              </div>
              <button
                onClick={onNewRun}
                style={{
                  display: "inline-flex", alignItems: "center", gap: 7,
                  padding: "8px 20px", borderRadius: 7, cursor: "pointer",
                  border: "1px solid rgba(62,207,142,0.24)",
                  background: "rgba(62,207,142,0.1)", color: C.accent,
                  fontSize: 12, fontWeight: 600, fontFamily: "inherit",
                  letterSpacing: "0.01em",
                }}
              >
                <Sparkles size={12} /> New Run
              </button>
            </div>
          )}
          {runs?.map((run, ri) => (
            <RunCard
              key={run.id}
              run={run}
              number={runs.length - ri}
              isExpanded={expandedRunId === run.id}
              onToggle={() => {
                setExpandedRunId(expandedRunId === run.id ? null : run.id);
              }}
              onContinueTask={onContinueTask}
              onViewDiff={onViewDiff}
            />
          ))}
        </div>
      )}

      {/* Session settings modal */}
      {showSettings && conversation && (
        <SessionSettingsModal
          conversation={conversation}
          onClose={() => setShowSettings(false)}
          onUpdated={refetchConversation}
          onDeleted={() => setShowSettings(false)}
        />
      )}

      {/* Keep CLI terminals alive (hidden) while viewing non-CLI conversations */}
      {visitedCli.size > 0 && Array.from(visitedCli.entries()).map(([convId, cwd]) => (
        <TerminalColumn
          key={convId}
          conversationId={convId}
          cwd={cwd}
          visible={false}
        />
      ))}
    </div>
  );
}
