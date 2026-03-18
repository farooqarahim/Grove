import {
  Arrow, BarChart, Check, ChevronDown, ChevronR, Clock, Commit, Copy,
  FileText, ForkIcon, HDotsIcon, Plus, PullRequest, Refresh, Shield,
  statusColor, StatusIcon, Terminal, Undo, Zap,
} from "@/components/ui/icons";
import {
  abortRun,
  forkRunWorktree,
  getRunReport,
  listMergeQueue,
  listOwnershipLocks,
  listPhaseCheckpoints,
  listPlanSteps,
  listQaMessages,
  listRunMessages,
  listSessions,
  listSignals,
  listSubtasks,
  markSignalRead,
  readSessionLog,
  resumeRun,
  retryPublishRun,
  sendAgentMessage,
} from "@/lib/api";
import type { LogEntry, PhaseCheckpointDto, QaMessageDto } from "@/lib/api";
import type { StreamOutputEvent } from "@/types/thread";
import { formatDuration, relativeTime } from "@/lib/hooks";
import { qk } from "@/lib/queryKeys";
import { formatRunAgentLabel } from "@/lib/runLabels";
import { C } from "@/lib/theme";
import type { MessageRow, PlanStep, RunRecord, RunReport, SessionRecord } from "@/types";
import { useQueries, useQuery } from "@tanstack/react-query";
import { useState, useRef, useEffect, memo } from "react";
import { AgentDetail } from "./AgentDetail";
import { PhaseGateBlock } from "./PhaseGateBlock";
import { PipelineViz } from "./PipelineViz";
import { QaCard } from "./QaCard";

// ── Constants ────────────────────────────────────────────────────────────────

const ACTIVE_STATES = ["executing", "waiting_for_gate", "planning", "verifying", "publishing", "merging"];
const RESUMABLE_STATES = ["failed", "paused"];
const EASE = "cubic-bezier(0.16, 1, 0.3, 1)";

const STATE_BADGE: Record<string, { label: string; color: string }> = {
  completed:        { label: "Completed",        color: "green" },
  failed:           { label: "Failed",           color: "red" },
  executing:        { label: "Executing",        color: "blue" },
  waiting_for_gate: { label: "Waiting For Gate", color: "amber" },
  planning:         { label: "Planning",         color: "blue" },
  verifying:        { label: "Verifying",        color: "amber" },
  publishing:       { label: "Publishing",       color: "blue" },
  merging:          { label: "Merging",          color: "amber" },
  paused:           { label: "Paused",           color: "gray" },
};

const PUBLISH_BADGE: Record<string, { label: string; color: string }> = {
  published:          { label: "Published",       color: "blue" },
  failed:             { label: "Publish Failed",  color: "red" },
  skipped_no_changes: { label: "No Changes",      color: "gray" },
  pending_retry:      { label: "Pending Publish",  color: "amber" },
};

const BADGE_COLORS: Record<string, { bg: string; border: string; text: string }> = {
  green:  { bg: "rgba(62,207,142,0.08)",  border: "rgba(62,207,142,0.2)",   text: "#3ecf8e" },
  purple: { bg: "rgba(167,139,250,0.1)",  border: "rgba(167,139,250,0.2)",  text: "#a78bfa" },
  amber:  { bg: "rgba(245,158,11,0.1)",   border: "rgba(245,158,11,0.25)",  text: "#f59e0b" },
  blue:   { bg: "rgba(96,165,250,0.08)",  border: "rgba(96,165,250,0.2)",   text: "#60a5fa" },
  red:    { bg: "rgba(248,113,113,0.1)",  border: "rgba(248,113,113,0.3)",  text: "#f87171" },
  gray:   { bg: "rgba(255,255,255,0.04)", border: "rgba(255,255,255,0.08)", text: "#8b8d98" },
};

const AGENT_BADGE_COLOR: Record<string, "purple" | "amber" | "green" | "blue" | "gray"> = {
  builder: "purple", validator: "amber", judge: "amber", result: "green",
};

function agentBadgeColor(agentType?: string): "purple" | "amber" | "green" | "blue" | "gray" {
  if (!agentType) return "gray";
  return AGENT_BADGE_COLOR[agentType.toLowerCase()] ?? "blue";
}

const AGENT_ICONS: Record<string, { icon: (size: number) => React.ReactNode; color: string; bg: string; border: string }> = {
  builder:   { icon: (s) => <Zap size={s} />,    color: "#a78bfa", bg: "rgba(167,139,250,0.1)", border: "rgba(167,139,250,0.2)" },
  validator: { icon: (s) => <Shield size={s} />,  color: "#f59e0b", bg: "rgba(245,158,11,0.1)",  border: "rgba(245,158,11,0.25)" },
  judge:     { icon: (s) => <Shield size={s} />,  color: "#f59e0b", bg: "rgba(245,158,11,0.1)",  border: "rgba(245,158,11,0.25)" },
};

// ── Palette ─────────────────────────────────────────────────────────────────

const P = {
  bg:          "#0e0f11",
  bgCard:      "#16171b",
  bgSurface:   "#1c1d22",
  bgHover:     "#22242a",
  bgElevated:  "#1a1b20",
  border:      "#2a2c33",
  borderSubtle:"#222329",
  text:        "#e2e4e9",
  textMuted:   "#8b8d98",
  textFaint:   "#5c5e6a",
  accent:      "#3ecf8e",
  accentMuted: "#2a9d6a",
  accentBg:    "rgba(62,207,142,0.08)",
  accentBorder:"rgba(62,207,142,0.2)",
  blue:        "#60a5fa",
  blueBorder:  "rgba(96,165,250,0.2)",
  red:         "#f87171",
  coral:       "#fb923c",
} as const;

// ── Sub-components ───────────────────────────────────────────────────────────

function Badge({ children, color = "gray", small = false }: { children: React.ReactNode; color?: string; small?: boolean }) {
  const c = BADGE_COLORS[color] ?? BADGE_COLORS.gray;
  return (
    <span style={{
      fontSize: small ? 10 : 11, fontWeight: 600, letterSpacing: "0.03em",
      padding: small ? "1px 6px" : "2px 8px", borderRadius: 4,
      background: c.bg, border: `1px solid ${c.border}`, color: c.text,
      whiteSpace: "nowrap", lineHeight: "18px",
      display: "inline-flex", alignItems: "center",
    }}>
      {children}
    </span>
  );
}

function IconBtn({ children, onClick, tooltip }: { children: React.ReactNode; onClick?: (e: React.MouseEvent) => void; tooltip?: string }) {
  return (
    <button onClick={onClick} title={tooltip} style={{
      background: "transparent", border: "none", color: P.textMuted, cursor: "pointer",
      padding: 6, borderRadius: 6, display: "flex", alignItems: "center", justifyContent: "center",
      transition: "all 0.15s",
    }}
      onMouseEnter={e => { e.currentTarget.style.background = P.bgHover; e.currentTarget.style.color = P.text; }}
      onMouseLeave={e => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = P.textMuted; }}
    >
      {children}
    </button>
  );
}

function CheckItem({ label, passed = true, failed = false, children }: { label: string; passed?: boolean; failed?: boolean; children?: React.ReactNode }) {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 8,
      padding: "4px 0",
      fontSize: 12.5, color: failed ? P.red : P.textMuted, lineHeight: "22px",
    }}>
      <div style={{
        width: 16, height: 16, borderRadius: 4, flexShrink: 0,
        background: passed ? P.accentBg : failed ? "rgba(248,113,113,0.1)" : "rgba(255,255,255,0.04)",
        border: `1.5px solid ${passed ? P.accent : failed ? P.red : "rgba(255,255,255,0.08)"}`,
        display: "flex", alignItems: "center", justifyContent: "center",
        color: passed ? P.accent : failed ? P.red : P.textFaint,
      }}>
        {passed && <Check size={10} />}
        {failed && <span style={{ fontSize: 10, fontWeight: 700, lineHeight: 1 }}>×</span>}
      </div>
      <span style={{ flex: 1, fontSize: 12.5, fontFamily: C.mono, color: P.textMuted, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
        {label}
      </span>
      {children}
    </div>
  );
}

// ── Main Component ───────────────────────────────────────────────────────────

interface RunCardProps {
  run: RunRecord;
  number: number;
  isExpanded: boolean;
  onToggle: () => void;
  expandedAgentKey: string | null;
  onToggleAgent: (key: string) => void;
  onContinueTask?: (conversationId: string, runId: string) => void;
  onViewDiff?: (runId: string) => void;
}

export const RunCard = memo(function RunCard({ run, number, isExpanded, onToggle, expandedAgentKey, onToggleAgent, onContinueTask, onViewDiff }: RunCardProps) {
  const isActive = ACTIVE_STATES.includes(run.state);
  const isDone = !isActive;
  const fallback = isDone ? false : 60000;
  const [runTab, setRunTab] = useState<"activity" | "agents" | "tasks" | "logs">("activity");
  const sessionRefetchInterval =
    isExpanded && isActive && runTab === "logs" ? 3000
      : isExpanded ? fallback
        : false;

  // ── Data hooks ──────────────────────────────────────────────────────────

  const { data: sessions } = useQuery({
    queryKey: qk.sessions(run.id),
    queryFn: () => listSessions(run.id),
    enabled: isExpanded,
    refetchInterval: sessionRefetchInterval,
    staleTime: runTab === "logs" ? 0 : 30000,
  });
  const { data: planSteps } = useQuery({ queryKey: qk.planSteps(run.id), queryFn: () => listPlanSteps(run.id), enabled: isExpanded, refetchInterval: isExpanded ? fallback : false, staleTime: 30000 });
  const { data: subtasks } = useQuery({ queryKey: qk.subtasks(run.id), queryFn: () => listSubtasks(run.id), enabled: isExpanded, refetchInterval: isExpanded ? fallback : false, staleTime: 30000 });
  const { data: ownershipLocks } = useQuery({ queryKey: qk.locks(run.id), queryFn: () => listOwnershipLocks(run.id), enabled: isExpanded, refetchInterval: isExpanded ? fallback : false, staleTime: 30000 });
  const { data: mergeQueue } = useQuery({ queryKey: qk.mergeQueue(run.id), queryFn: () => listMergeQueue(run.id), enabled: isExpanded, refetchInterval: isExpanded ? fallback : false, staleTime: 30000 });
  const { data: signals } = useQuery({ queryKey: qk.signals(run.id), queryFn: () => listSignals(run.id), enabled: isExpanded, refetchInterval: isExpanded ? fallback : false, staleTime: 30000 });
  const { data: runMessages } = useQuery({ queryKey: qk.runMessages(run.id), queryFn: () => listRunMessages(run.id), enabled: isExpanded, refetchInterval: isExpanded ? fallback : false, staleTime: 30000 });

  const shouldLoadCheckpoints = isExpanded || run.state === "waiting_for_gate" || run.disable_phase_gates;
  const { data: checkpoints } = useQuery({
    queryKey: qk.checkpoints(run.id),
    queryFn: () => listPhaseCheckpoints(run.id),
    enabled: shouldLoadCheckpoints,
    refetchInterval: shouldLoadCheckpoints && isActive ? 5000 : false,
    staleTime: 5000,
  });

  const pendingCheckpoints = (checkpoints ?? []).filter((cp: PhaseCheckpointDto) => cp.status === "pending");
  const visibleGateBlocks =
    pendingCheckpoints.length > 0
      ? pendingCheckpoints
      : run.disable_phase_gates
        ? (checkpoints ?? []).filter((cp: PhaseCheckpointDto) => cp.status !== "pending").slice(-1)
        : [];
  const gateHistoryBlocks = (checkpoints ?? []).filter(
    (cp: PhaseCheckpointDto) =>
      cp.status !== "pending" && !visibleGateBlocks.some((visible) => visible.id === cp.id),
  );

  const sessionLogQueries = useQueries({
    queries: (sessions ?? []).map((session) => ({
      queryKey: ["sessionLogs", run.id, session.id],
      queryFn: () => readSessionLog(run.id, session.id),
      enabled: isExpanded && runTab === "logs",
      refetchInterval:
        isExpanded && runTab === "logs" && isActive && session.state === "running" ? 3000 : false,
      staleTime: 0,
      gcTime: 0,
    })),
  });
  const sessionLogs = (sessions ?? []).map((session, index) => ({
    session,
    entries: (sessionLogQueries[index]?.data ?? []).map((entry) => ({
      ...entry,
      agentType: session.agent_type,
    })),
  }));

  // ── State ───────────────────────────────────────────────────────────────

  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [confirmAbort, setConfirmAbort] = useState(false);
  const [expandedSection, setExpandedSection] = useState<string | null>(null);
  const [reportData, setReportData] = useState<RunReport | null>(null);
  const [reportLoading, setReportLoading] = useState(false);
  const [eventCatFilter, setEventCatFilter] = useState<string | null>(null);
  const [eventsShowAll, setEventsShowAll] = useState(false);
  const [animReady, setAnimReady] = useState(false);
  const [liveStreamEntries, setLiveStreamEntries] = useState<(LogEntry & { agentType?: string })[]>([]);

  // ── Stagger entrance animation ──────────────────────────────────────────

  useEffect(() => {
    if (isExpanded) {
      const t = setTimeout(() => setAnimReady(true), 50);
      return () => clearTimeout(t);
    } else {
      setAnimReady(false);
    }
  }, [isExpanded]);

  // ── Live stream event listener ──────────────────────────────────────────

  useEffect(() => {
    if (!isActive || !isExpanded) return;
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail as { run_id: string; event: StreamOutputEvent } | undefined;
      if (!detail || detail.run_id !== run.id) return;
      const ev = detail.event;
      let entry: (LogEntry & { agentType?: string }) | null = null;
      const base = { line_no: null, event_type: null, subtype: null, detail: null, metadata_json: null };
      switch (ev.kind) {
        case "system":
          entry = { ...base, role: "system", content: ev.message, tool_name: null, session_id: ev.session_id ?? null, cost_usd: null, is_error: false };
          break;
        case "assistant_text":
          entry = { ...base, role: "assistant", content: ev.text, tool_name: null, session_id: null, cost_usd: null, is_error: false };
          break;
        case "tool_use":
          entry = { ...base, role: "tool_use", content: "", tool_name: ev.tool, session_id: null, cost_usd: null, is_error: false };
          break;
        case "tool_result":
          entry = { ...base, role: "tool_result", content: "", tool_name: ev.tool, session_id: null, cost_usd: null, is_error: false };
          break;
        case "result":
          entry = { ...base, role: "result", content: ev.text, tool_name: null, session_id: ev.session_id ?? null, cost_usd: ev.cost_usd ?? null, is_error: ev.is_error };
          break;
        case "raw_line":
          entry = { ...base, role: "raw", content: ev.line, tool_name: null, session_id: null, cost_usd: null, is_error: false };
          break;
      }
      if (entry) setLiveStreamEntries(prev => [...prev, entry!]);
    };
    window.addEventListener("grove-agent-output", handler);
    return () => window.removeEventListener("grove-agent-output", handler);
  }, [isActive, isExpanded, run.id]);

  useEffect(() => {
    if (!isActive) setLiveStreamEntries([]);
  }, [isActive]);

  // ── Derived values ──────────────────────────────────────────────────────

  const rs = statusColor(run.state);
  const duration = formatDuration(run.created_at, run.state === "completed" || run.state === "failed" ? run.updated_at : null);
  const branch = `grove/r_${run.id.slice(0, 8)}`;
  const isResumable = RESUMABLE_STATES.includes(run.state);
  const badge = STATE_BADGE[run.state] ?? { label: run.state, color: "gray" };
  const publishBadge = run.publish_status ? (PUBLISH_BADGE[run.publish_status] ?? { label: run.publish_status, color: "gray" }) : null;
  const canRetryPublish = run.state === "completed" && (run.publish_status === "failed" || run.publish_status === "pending_retry");

  // ── Event handlers ──────────────────────────────────────────────────────

  const handleResume = async () => {
    setActionLoading("resume"); setActionError(null);
    try { await resumeRun(run.id); } catch (e) { setActionError(e instanceof Error ? e.message : String(e)); } finally { setActionLoading(null); }
  };

  const handleReport = async () => {
    if (reportData) { setExpandedSection(expandedSection === "report" ? null : "report"); return; }
    setReportLoading(true);
    try { const data = await getRunReport(run.id); setReportData(data); setExpandedSection("report"); } catch (e) { setActionError(e instanceof Error ? e.message : String(e)); } finally { setReportLoading(false); }
  };

  const toggleSection = (name: string) => setExpandedSection(expandedSection === name ? null : name);

  const handleAbort = async () => {
    if (!confirmAbort) { setConfirmAbort(true); setTimeout(() => setConfirmAbort(false), 4000); return; }
    setConfirmAbort(false); setActionLoading("abort"); setActionError(null);
    try { await abortRun(run.id); } catch (e) { setActionError(e instanceof Error ? e.message : String(e)); } finally { setActionLoading(null); }
  };

  // ── Session grouping ───────────────────────────────────────────────────

  const groupedSessions = (() => {
    if (!sessions || !planSteps) return sessions ? [sessions] : [];
    const groups: SessionRecord[][] = [];
    const sessionWaveMap = new Map<string, number>();
    for (const step of planSteps) { if (step.session_id) sessionWaveMap.set(step.session_id, step.wave); }
    const byWave = new Map<number, SessionRecord[]>();
    let maxWave = 0;
    for (const s of sessions) {
      const wave = sessionWaveMap.get(s.id) ?? maxWave++;
      const existing = byWave.get(wave) ?? [];
      existing.push(s);
      byWave.set(wave, existing);
    }
    const sortedWaves = Array.from(byWave.keys()).sort((a, b) => a - b);
    for (const w of sortedWaves) groups.push(byWave.get(w)!);
    return groups.length > 0 ? groups : sessions ? [sessions] : [];
  })();

  const stepForSession = (sessionId: string): PlanStep | undefined =>
    planSteps?.find(s => s.session_id === sessionId);

  // ── Container styling ──────────────────────────────────────────────────

  const containerStyle: React.CSSProperties = isExpanded
    ? {
      borderRadius: 12,
      background: P.bgCard,
      boxShadow: "0 10px 24px rgba(0,0,0,0.25)",
      border: `1px solid ${P.accentBorder}`,
      overflow: "hidden",
      marginBottom: 8,
      transition: "all 0.2s ease",
    }
    : isActive
      ? {
        borderRadius: 12,
        background: P.bgCard,
        border: `1px solid ${P.blueBorder}`,
        boxShadow: "0 4px 12px rgba(96,165,250,0.05)",
        overflow: "hidden",
        marginBottom: 8,
        transition: "all 0.2s ease",
      }
      : {
        borderRadius: 12,
        background: "rgba(22,23,27,0.5)",
        border: "1px solid transparent",
        overflow: "hidden",
        marginBottom: 8,
        transition: "all 0.2s ease",
      };

  // ── Tab definitions ────────────────────────────────────────────────────

  const tabs: { id: "activity" | "agents" | "tasks" | "logs"; label: string; count?: number }[] = [
    { id: "activity", label: "Activity" },
    { id: "agents", label: "Agents", count: sessions?.length },
    { id: "tasks", label: "Tasks", count: subtasks?.length ?? 0 },
    { id: "logs", label: "Logs", count: sessions?.length },
  ];

  // ── Render ─────────────────────────────────────────────────────────────

  return (
    <div className={isExpanded ? "" : "run-card"} style={containerStyle}>

      {/* ── Streaming progress bar (collapsed active) ── */}
      {isActive && !isExpanded && (
        <div style={{ height: 2, background: P.bgHover, overflow: "hidden", borderRadius: "12px 12px 0 0" }}>
          <div style={{
            height: "100%", width: "33%",
            background: `linear-gradient(90deg, transparent, ${P.blue}, transparent)`,
            animation: "stream-bar 1.8s infinite linear",
          }} />
        </div>
      )}

      {/* ── HEADER ── */}
      <div
        onClick={onToggle}
        style={{
          padding: "20px 16px 16px", cursor: "pointer",
          display: "flex", alignItems: "flex-start", gap: 12,
          opacity: isExpanded ? (animReady ? 1 : 0) : 1,
          transform: isExpanded ? (animReady ? "translateY(0)" : "translateY(8px)") : undefined,
          transition: isExpanded ? `all 0.4s ${EASE}` : undefined,
        }}
      >
        {/* Status circle */}
        <div style={{
          width: 28, height: 28, borderRadius: "50%", flexShrink: 0, marginTop: 1,
          background: isActive ? BADGE_COLORS.blue.bg : rs.bg,
          border: `1.5px solid ${isActive ? BADGE_COLORS.blue.border : rs.border}`,
          display: "flex", alignItems: "center", justifyContent: "center",
          color: isActive ? P.blue : rs.text,
        }}>
          {isActive ? (
            <span style={{ display: "inline-block", animation: "spin 1s linear infinite" }}>
              <StatusIcon status={run.state} size={14} />
            </span>
          ) : (
            run.state === "completed" ? <Check size={14} /> : <StatusIcon status={run.state} size={14} />
          )}
        </div>

        <div style={{ flex: 1, minWidth: 0 }}>
          {/* Title line */}
          <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
            <span style={{ fontSize: 13, color: P.textFaint, fontWeight: 500, flexShrink: 0 }}>#{number}</span>
            <h2 style={{
              fontSize: 15, fontWeight: 500, color: P.text, lineHeight: 1.4,
              overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
              flex: 1, minWidth: 0, margin: 0,
            }}>
              {run.objective}
            </h2>
          </div>

          {/* Badges + meta */}
          <div style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>
            <Badge color={badge.color}>
              {isActive && (
                <span style={{
                  width: 5, height: 5, borderRadius: "50%", background: "currentColor",
                  animation: "pulse 1.5s infinite", display: "inline-block", marginRight: 4,
                }} />
              )}
              {badge.label}
            </Badge>
            {publishBadge && (
              <Badge color={publishBadge.color}>
                {run.state === "publishing" && (
                  <span style={{
                    width: 5, height: 5, borderRadius: "50%", background: "currentColor",
                    animation: "pulse 1.5s infinite", display: "inline-block", marginRight: 4,
                  }} />
                )}
                {publishBadge.label}
              </Badge>
            )}
            <Badge color="gray">AUTO</Badge>
            <span style={{ width: 1, height: 12, background: P.borderSubtle, margin: "0 4px" }} />
            <span style={{ fontSize: 12, color: P.textFaint, display: "flex", alignItems: "center", gap: 4 }}>
              <Clock size={11} /> {duration}
            </span>
            <span style={{ fontSize: 12, color: P.textFaint }}>·</span>
            <span style={{ fontSize: 12, color: P.textFaint, display: "flex", alignItems: "center", gap: 4 }}>
              <Zap size={11} /> {sessions?.length ?? 0} agent{(sessions?.length ?? 0) !== 1 ? "s" : ""}
            </span>
            <span style={{ fontSize: 12, color: P.textFaint }}>·</span>
            <span style={{ fontSize: 12, color: P.textFaint }}>
              {new Date(run.created_at).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}
            </span>
          </div>

          {/* Pipeline viz (collapsed) */}
          {!isExpanded && sessions && sessions.length > 0 && (
            <div style={{ marginTop: 8 }}><PipelineViz sessions={sessions} /></div>
          )}
        </div>

        {/* Right controls */}
        <div style={{ display: "flex", alignItems: "center", gap: 4, flexShrink: 0 }}>
          <IconBtn onClick={(e) => e.stopPropagation()} tooltip="More options">
            <HDotsIcon size={14} />
          </IconBtn>
          <span className="transition-transform" style={{
            color: P.textFaint,
            transform: isExpanded ? "rotate(90deg)" : "",
            display: "inline-flex",
          }}>
            <ChevronR size={12} />
          </span>
        </div>
      </div>

      {/* ── EXPANDED BODY ── */}
      {isExpanded && sessions && (
        <div onClick={(e) => e.stopPropagation()}>

          {/* ── Commit bar ── */}
          <div style={{
            display: "flex", alignItems: "center", gap: 10,
            padding: "8px 12px", margin: "0 16px", borderRadius: 8,
            background: P.bgSurface,
            border: `1px solid ${P.borderSubtle}`,
            marginBottom: 4,
            opacity: animReady ? 1 : 0,
            transform: animReady ? "translateY(0)" : "translateY(6px)",
            transition: `all 0.4s ${EASE} 0.08s`,
          }}>
            <span style={{ color: P.textFaint, display: "flex", alignItems: "center" }}>
              <Commit size={14} />
            </span>
            {run.final_commit_sha ? (
              <code style={{ fontFamily: C.mono, fontSize: 12, color: P.accent, fontWeight: 500 }}>
                {run.final_commit_sha.slice(0, 8)}
              </code>
            ) : (
              <code style={{ fontFamily: C.mono, fontSize: 12, color: P.textFaint, fontWeight: 500 }}>pending</code>
            )}
            <span style={{ fontSize: 12, color: P.textFaint }}>·</span>
            <code style={{ fontFamily: C.mono, fontSize: 11.5, color: P.textMuted, flex: 1 }}>{branch}</code>
            {run.publish_error && <span style={{ fontSize: 11, color: P.red }}>{run.publish_error}</span>}
            <IconBtn
              onClick={(e) => { e.stopPropagation(); navigator.clipboard.writeText(run.final_commit_sha ?? branch).catch(() => {}); }}
              tooltip="Copy hash"
            >
              <Copy size={13} />
            </IconBtn>
          </div>

          {/* ── Phase gates ── */}
          {visibleGateBlocks.length > 0 && (
            <div style={{ padding: "8px 16px", display: "flex", flexDirection: "column", gap: 8 }}>
              {visibleGateBlocks.map(cp => <PhaseGateBlock key={cp.id} checkpoint={cp} runId={run.id} pipeline={run.pipeline} />)}
            </div>
          )}

          {/* ── TABS ── */}
          <div style={{
            display: "flex", gap: 0,
            borderBottom: `1px solid ${P.borderSubtle}`,
            marginTop: 8, padding: "0 16px",
            opacity: animReady ? 1 : 0,
            transition: "opacity 0.4s 0.15s",
          }}>
            {tabs.map(tab => {
              const active = runTab === tab.id;
              return (
                <button
                  key={tab.id}
                  onClick={() => setRunTab(tab.id)}
                  style={{
                    background: "none", border: "none", cursor: "pointer",
                    padding: "10px 16px", fontSize: 13,
                    fontWeight: active ? 500 : 400,
                    color: active ? P.text : P.textFaint,
                    display: "flex", alignItems: "center", gap: 6,
                    transition: "color 0.15s",
                    position: "relative",
                    borderBottom: active ? `2px solid ${P.accent}` : "2px solid transparent",
                    marginBottom: -1,
                  }}
                >
                  {tab.label}
                  {tab.count !== undefined && (
                    <span style={{
                      fontSize: 10, fontWeight: 600, fontFamily: C.mono,
                      padding: "1px 6px", borderRadius: 99,
                      background: active ? "rgba(62,207,142,0.1)" : "rgba(255,255,255,0.04)",
                      color: active ? P.accent : P.textFaint,
                      border: `1px solid ${active ? "rgba(62,207,142,0.15)" : "transparent"}`,
                    }}>
                      {tab.count}
                    </span>
                  )}
                </button>
              );
            })}
          </div>

          {/* ── TAB CONTENT ── */}
          <div style={{ maxHeight: 480, overflowY: "auto" }}>

            {/* ════ Activity tab ════ */}
            {runTab === "activity" && (
              <div style={{ flex: 1, overflow: "auto", paddingTop: 4, paddingBottom: 16 }}>
                <RunThread
                  runId={run.id}
                  sessions={sessions}
                  dbMessages={runMessages}
                  runState={run.state}
                  liveEntries={liveStreamEntries}
                  animReady={animReady}
                />

                {gateHistoryBlocks.length > 0 && (
                  <div style={{ padding: "8px 16px 0", display: "flex", flexDirection: "column", gap: 8 }}>
                    {gateHistoryBlocks.map(cp => <PhaseGateBlock key={`history-${cp.id}`} checkpoint={cp} runId={run.id} />)}
                  </div>
                )}

                {sessions && sessions.length > 0 && (
                  <InlineLogsSection sessionLogs={sessionLogs} animReady={animReady} />
                )}
              </div>
            )}

            {/* ════ Agents tab ════ */}
            {runTab === "agents" && (
              <div style={{ flex: 1, padding: "16px 16px 12px" }}>
                {groupedSessions.length > 0 && (
                  <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                    {groupedSessions.map((group, gi) => (
                      <div key={gi} style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                        {group.map((session, si) => {
                          const aKey = `${run.id}-${gi}-${si}`;
                          const agentIcon = AGENT_ICONS[session.agent_type.toLowerCase()] ?? { icon: (s: number) => <Zap size={s} />, color: P.blue, bg: BADGE_COLORS.blue.bg, border: BADGE_COLORS.blue.border };
                          return (
                            <div key={session.id}>
                              <div
                                onClick={() => onToggleAgent(expandedAgentKey === aKey ? "" : aKey)}
                                style={{
                                  display: "flex", alignItems: "center", gap: 12,
                                  padding: "12px 14px",
                                  background: P.bgCard, border: `1px solid ${P.borderSubtle}`,
                                  borderRadius: 10, cursor: "pointer",
                                }}
                              >
                                <div style={{
                                  width: 32, height: 32, borderRadius: 8,
                                  background: agentIcon.bg, border: `1px solid ${agentIcon.border}`,
                                  display: "flex", alignItems: "center", justifyContent: "center",
                                  color: agentIcon.color,
                                }}>
                                  {agentIcon.icon(14)}
                                </div>
                                <div style={{ flex: 1, minWidth: 0 }}>
                                  <div style={{ fontSize: 13, fontWeight: 500, color: P.text }}>{session.agent_type}</div>
                                  <div style={{ fontSize: 11.5, color: P.textFaint, fontFamily: C.mono }}>{session.id.slice(0, 12)}</div>
                                </div>
                                <Badge color="gray" small>{session.state}</Badge>
                              </div>
                              {expandedAgentKey === aKey && (
                                <div style={{ marginTop: 4 }}>
                                  <AgentDetail session={session} planStep={stepForSession(session.id)} isExpanded={true} onToggle={() => onToggleAgent("")} />
                                </div>
                              )}
                            </div>
                          );
                        })}
                      </div>
                    ))}
                  </div>
                )}

                {/* File Locks */}
                {ownershipLocks && ownershipLocks.length > 0 && (
                  <CollapsibleSection title="File Locks" count={ownershipLocks.length} isOpen={expandedSection === "locks"} onToggle={() => toggleSection("locks")}>
                    {ownershipLocks.map(lock => (
                      <div key={lock.id} style={{ display: "flex", alignItems: "center", gap: 8, padding: "5px 6px" }}>
                        <span style={{ width: 6, height: 6, borderRadius: "50%", background: "#f59e0b", flexShrink: 0 }} />
                        <span style={{ fontSize: 13, color: P.text, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{lock.path}</span>
                        <span style={{ fontSize: 12, color: P.textFaint }}>{lock.owner_session_id.slice(0, 8)}</span>
                      </div>
                    ))}
                  </CollapsibleSection>
                )}

                {/* Merge Queue */}
                {mergeQueue && mergeQueue.length > 0 && (
                  <CollapsibleSection title="Merge Queue" count={mergeQueue.length} isOpen={expandedSection === "merge"} onToggle={() => toggleSection("merge")}>
                    {mergeQueue.map(mq => (
                      <div key={mq.id} style={{ display: "flex", alignItems: "center", gap: 8, padding: "5px 6px" }}>
                        <StepStatusDot status={mq.status} />
                        <span style={{ fontSize: 13, color: P.text, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{mq.branch_name}</span>
                        <span style={{
                          fontSize: 11, padding: "1px 6px", borderRadius: 4, fontWeight: 500,
                          background: mq.status === "merged" ? BADGE_COLORS.green.bg : mq.status === "failed" ? BADGE_COLORS.red.bg : BADGE_COLORS.amber.bg,
                          color: mq.status === "merged" ? P.accent : mq.status === "failed" ? P.red : "#f59e0b",
                        }}>{mq.status}</span>
                      </div>
                    ))}
                  </CollapsibleSection>
                )}
              </div>
            )}

            {/* ════ Tasks tab ════ */}
            {runTab === "tasks" && (
              <div style={{ padding: "12px 16px" }}>
                {/* Plan Steps */}
                {planSteps && planSteps.length > 0 && (
                  <div style={{
                    padding: 14, borderRadius: 10, marginBottom: 12,
                    background: P.bgSurface, border: `1px solid ${P.borderSubtle}`,
                  }}>
                    <div style={{
                      display: "flex", alignItems: "center", gap: 8, marginBottom: 10,
                      paddingBottom: 8, borderBottom: `1px solid ${P.borderSubtle}`,
                    }}>
                      <Badge color="blue" small>Plan</Badge>
                      <span style={{ fontSize: 12, fontWeight: 500, color: P.textMuted }}>Steps</span>
                      <span style={{ fontSize: 10, fontFamily: C.mono, color: P.textFaint, marginLeft: "auto" }}>
                        {planSteps.filter(s => s.status === "completed" || s.status === "done").length}/{planSteps.length}
                      </span>
                    </div>
                    {(() => {
                      const waveMap = new Map<number, PlanStep[]>();
                      for (const s of planSteps) { const arr = waveMap.get(s.wave) ?? []; arr.push(s); waveMap.set(s.wave, arr); }
                      const waves = Array.from(waveMap.keys()).sort((a, b) => a - b);
                      return waves.map(w => (
                        <div key={w} style={{ marginBottom: 4 }}>
                          {waves.length > 1 && (
                            <div style={{ fontSize: 10, color: P.textFaint, fontWeight: 600, letterSpacing: "0.05em", marginBottom: 4, paddingLeft: 4 }}>
                              Wave {w + 1}{waveMap.get(w)!.length > 1 ? " · parallel" : ""}
                            </div>
                          )}
                          {waveMap.get(w)!.map(step => {
                            const stepDone = step.status === "completed" || step.status === "done";
                            return (
                              <CheckItem key={step.id} label={step.title} passed={stepDone} failed={step.status === "failed"}>
                                <Badge color={agentBadgeColor(step.agent_type)} small>{step.agent_type}</Badge>
                              </CheckItem>
                            );
                          })}
                        </div>
                      ));
                    })()}
                  </div>
                )}

                {/* Subtasks */}
                {subtasks && subtasks.length > 0 && (
                  <div style={{
                    padding: 14, borderRadius: 10,
                    background: P.bgSurface, border: `1px solid ${P.borderSubtle}`,
                  }}>
                    <div style={{
                      display: "flex", alignItems: "center", gap: 8, marginBottom: 10,
                      paddingBottom: 8, borderBottom: `1px solid ${P.borderSubtle}`,
                    }}>
                      <Badge color="purple" small>Subtasks</Badge>
                      <span style={{ fontSize: 10, fontFamily: C.mono, color: P.textFaint, marginLeft: "auto" }}>
                        {subtasks.filter(s => s.status === "completed" || s.status === "done").length}/{subtasks.length}
                      </span>
                    </div>
                    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                      {subtasks.map(st => {
                        const stDone = st.status === "completed" || st.status === "done";
                        return (
                          <div key={st.id} style={{ display: "flex", alignItems: "flex-start", gap: 8, fontSize: 12, color: P.textMuted, padding: "4px 0" }}>
                            <div style={{
                              width: 16, height: 16, borderRadius: 4, flexShrink: 0, marginTop: 2,
                              background: stDone ? P.accentBg : st.status === "failed" ? "rgba(248,113,113,0.1)" : "rgba(255,255,255,0.04)",
                              border: `1px solid ${stDone ? P.accentBorder : st.status === "failed" ? "rgba(248,113,113,0.3)" : "rgba(255,255,255,0.08)"}`,
                              display: "flex", alignItems: "center", justifyContent: "center",
                              color: stDone ? P.accent : st.status === "failed" ? P.red : P.textFaint,
                            }}>
                              {stDone && <Check size={9} />}
                            </div>
                            <div style={{ flex: 1, minWidth: 0 }}>
                              <span style={{ color: P.text, fontWeight: 500, fontSize: 13 }}>{st.title}</span>
                              {st.description && (
                                <p style={{ fontSize: 12, color: P.textMuted, marginTop: 2, lineHeight: 1.5, margin: "2px 0 0" }}>
                                  {st.description.length > 140 ? st.description.slice(0, 140) + "…" : st.description}
                                </p>
                              )}
                              {st.files_hint.length > 0 && (
                                <div style={{ display: "flex", gap: 4, flexWrap: "wrap", marginTop: 4 }}>
                                  {st.files_hint.slice(0, 4).map(f => (
                                    <code key={f} style={{ fontSize: 10, color: "#a78bfa", fontFamily: C.mono, background: BADGE_COLORS.purple.bg, padding: "1px 5px", borderRadius: 3 }}>
                                      {f.split("/").pop()}
                                    </code>
                                  ))}
                                  {st.files_hint.length > 4 && <span style={{ fontSize: 10, color: P.textFaint }}>+{st.files_hint.length - 4}</span>}
                                </div>
                              )}
                            </div>
                            {st.assigned_agent && <Badge color={agentBadgeColor(st.assigned_agent)} small>{st.assigned_agent}</Badge>}
                          </div>
                        );
                      })}
                    </div>
                  </div>
                )}

                {/* Empty state */}
                {(!planSteps || planSteps.length === 0) && (!subtasks || subtasks.length === 0) && (
                  <div style={{ display: "flex", alignItems: "center", justifyContent: "center", padding: "60px 0" }}>
                    <div style={{ textAlign: "center" }}>
                      <div style={{
                        width: 40, height: 40, borderRadius: 10, margin: "0 auto 12px",
                        background: "rgba(255,255,255,0.03)", border: `1px dashed ${P.border}`,
                        display: "flex", alignItems: "center", justifyContent: "center",
                        color: P.textFaint,
                      }}>
                        <FileText size={18} />
                      </div>
                      <p style={{ fontSize: 13, color: P.textFaint, margin: 0 }}>No tasks in this run</p>
                    </div>
                  </div>
                )}
              </div>
            )}

            {/* ════ Logs tab ════ */}
            {runTab === "logs" && (
              <SessionLogsTab sessionLogs={sessionLogs} isActive={isActive} pipeline={run.pipeline} />
            )}
          </div>

          {/* ── Report Panel ── */}
          {expandedSection === "report" && reportData && (
            <div style={{ borderRadius: 10, border: `1px solid ${P.borderSubtle}`, margin: "0 16px 12px", overflow: "hidden" }}>
              <div style={{
                padding: "10px 14px", display: "flex", alignItems: "center", justifyContent: "space-between",
                borderBottom: `1px solid ${P.borderSubtle}`, background: P.bgSurface,
              }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <Badge color="blue" small>Report</Badge>
                  <Badge color={badge.color} small>{badge.label}</Badge>
                </div>
                <IconBtn onClick={(e) => { e.stopPropagation(); setExpandedSection(null); }} tooltip="Close">
                  <span style={{ fontSize: 14, lineHeight: 1 }}>×</span>
                </IconBtn>
              </div>

              <div style={{ maxHeight: 480, overflowY: "auto" }}>
                {/* Overview */}
                <div style={{ padding: "10px 12px", borderBottom: `1px solid ${P.borderSubtle}` }}>
                  <div style={{ fontSize: 10, fontWeight: 700, textTransform: "uppercase", letterSpacing: "0.06em", color: P.textFaint, marginBottom: 8 }}>Overview</div>
                  <p style={{ margin: "0 0 8px", fontSize: 13, color: P.text, lineHeight: 1.5 }}>{reportData.objective}</p>
                  <div style={{ display: "flex", gap: 16, fontSize: 11, color: P.textFaint }}>
                    <span>Started <b style={{ color: P.textMuted, fontWeight: 500 }}>{new Date(reportData.created_at).toLocaleString([], { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })}</b></span>
                    <span style={{ fontFamily: C.mono }}>{reportData.run_id.slice(0, 8)}</span>
                  </div>
                </div>

                {/* Sessions */}
                {reportData.sessions.length > 0 && (
                  <div style={{ padding: "10px 12px", borderBottom: `1px solid ${P.borderSubtle}` }}>
                    <div style={{ fontSize: 10, fontWeight: 700, textTransform: "uppercase", letterSpacing: "0.06em", color: P.textFaint, marginBottom: 8 }}>
                      Agents <span style={{ fontSize: 10, background: P.bgSurface, padding: "1px 5px", borderRadius: 4, marginLeft: 4 }}>{reportData.sessions.length}</span>
                    </div>
                    <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                      {reportData.sessions.map((s, si) => {
                        const sc2 = statusColor(s.state);
                        const durStr = s.started_at ? formatDuration(s.started_at, s.ended_at) : null;
                        return (
                          <div key={s.id} style={{
                            display: "flex", alignItems: "center", gap: 8, padding: "5px 0",
                            borderTop: si > 0 ? `1px solid ${P.borderSubtle}` : undefined,
                          }}>
                            <div style={{
                              width: 20, height: 20, borderRadius: 4, flexShrink: 0,
                              background: sc2.bg, display: "flex", alignItems: "center", justifyContent: "center",
                            }}>
                              <StatusIcon status={s.state} size={9} />
                            </div>
                            <span style={{ fontSize: 12, color: P.text, fontWeight: 500, flex: 1 }}>{formatRunAgentLabel(s.agent_type, run.pipeline)}</span>
                            {durStr && <span style={{ fontSize: 11, color: P.textMuted }}>{durStr}</span>}
                            {s.started_at && (
                              <span style={{ fontSize: 10, color: P.textFaint, fontFamily: C.mono }}>
                                {new Date(s.started_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
                                {s.ended_at && ` – ${new Date(s.ended_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`}
                              </span>
                            )}
                          </div>
                        );
                      })}
                    </div>
                  </div>
                )}

                {/* Events */}
                {reportData.events.length > 0 && (() => {
                  const allCats = Array.from(new Set(reportData.events.map(e => evCat(e.event_type).label)));
                  const filtered = eventCatFilter
                    ? reportData.events.filter(e => evCat(e.event_type).label === eventCatFilter)
                    : reportData.events;
                  const LIMIT = 30;
                  const visible = eventsShowAll ? filtered : filtered.slice(0, LIMIT);
                  const hidden = filtered.length - visible.length;
                  return (
                    <div style={{ padding: "10px 12px" }}>
                      <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 8, flexWrap: "wrap" }}>
                        <span style={{ fontSize: 10, fontWeight: 700, textTransform: "uppercase", letterSpacing: "0.06em", color: P.textFaint }}>Events</span>
                        <span style={{ fontSize: 10, background: P.bgSurface, padding: "1px 5px", borderRadius: 4, color: P.textFaint }}>
                          {filtered.length}{eventCatFilter ? ` / ${reportData.events.length}` : ""}
                        </span>
                        <div style={{ flex: 1 }} />
                        {allCats.map(cat => {
                          const active = eventCatFilter === cat;
                          return (
                            <button key={cat}
                              onClick={(e) => { e.stopPropagation(); setEventCatFilter(active ? null : cat); setEventsShowAll(false); }}
                              style={{
                                fontSize: 9, fontWeight: 700, textTransform: "uppercase", letterSpacing: "0.05em",
                                padding: "2px 6px", borderRadius: 4, cursor: "pointer",
                                background: active ? P.bgHover : P.bgSurface,
                                color: active ? P.text : P.textFaint,
                                border: active ? `1px solid ${P.border}` : "1px solid transparent",
                              }}
                            >{cat}</button>
                          );
                        })}
                      </div>
                      <div style={{ display: "flex", flexDirection: "column" }}>
                        {visible.map((ev, i) => {
                          const cat = evCat(ev.event_type);
                          const summary = evSummary(ev.event_type, ev.payload, run.pipeline);
                          return (
                            <div key={i} className="activity-row" style={{
                              display: "flex", alignItems: "flex-start", gap: 8,
                              padding: "6px 4px",
                              borderBottom: i < visible.length - 1 ? `1px solid ${P.borderSubtle}` : undefined,
                            }}>
                              <div style={{ flex: 1, minWidth: 0 }}>
                                <div style={{ display: "flex", alignItems: "center", gap: 5, flexWrap: "wrap" }}>
                                  <Badge color={
                                    cat.label === "gate" ? "amber" : cat.label === "tool" ? "purple"
                                      : cat.label === "commit" ? "green" : cat.label === "error" ? "red" : "gray"
                                  } small>{cat.label}</Badge>
                                  <span style={{ fontSize: 12, color: summary.verdictColor ?? P.text }}>{summary.title}</span>
                                  {summary.chips.map((chip, ci) => (
                                    <span key={ci} style={{ fontSize: 10, padding: "1px 6px", borderRadius: 4, background: P.bgSurface, color: P.textMuted, fontFamily: C.mono }}>{chip}</span>
                                  ))}
                                </div>
                                {summary.detail && (
                                  <p style={{ margin: "3px 0 0", fontSize: 11, color: P.textMuted, lineHeight: 1.5, overflow: "hidden", display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical" }}>
                                    {summary.detail}
                                  </p>
                                )}
                              </div>
                              <span style={{ fontSize: 10, color: P.textFaint, fontFamily: C.mono, flexShrink: 0, paddingTop: 2 }}>
                                {new Date(ev.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                              </span>
                            </div>
                          );
                        })}
                      </div>
                      {hidden > 0 && (
                        <button onClick={(e) => { e.stopPropagation(); setEventsShowAll(true); }}
                          style={{ marginTop: 8, width: "100%", padding: "5px 0", borderRadius: 4, background: P.bgSurface, border: "none", cursor: "pointer", fontSize: 11, color: P.textMuted }}>
                          Show {hidden} more event{hidden !== 1 ? "s" : ""}
                        </button>
                      )}
                      {eventsShowAll && filtered.length > LIMIT && (
                        <button onClick={(e) => { e.stopPropagation(); setEventsShowAll(false); }}
                          style={{ marginTop: 8, width: "100%", padding: "5px 0", borderRadius: 4, background: "transparent", border: "none", cursor: "pointer", fontSize: 11, color: P.textFaint }}>
                          Show less
                        </button>
                      )}
                    </div>
                  );
                })()}

                {/* Signals */}
                {signals && signals.length > 0 && (
                  <div style={{ padding: "10px 12px", borderTop: `1px solid ${P.borderSubtle}` }}>
                    <div style={{ fontSize: 10, fontWeight: 700, textTransform: "uppercase", letterSpacing: "0.06em", color: P.textFaint, marginBottom: 8 }}>
                      Signals <span style={{ fontSize: 10, background: P.bgSurface, padding: "1px 5px", borderRadius: 4, marginLeft: 4 }}>{signals.length}</span>
                    </div>
                    <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
                      {signals.map(sig => {
                        const pc = sig.priority === "critical" ? P.red : sig.priority === "high" ? "#f59e0b" : P.textMuted;
                        const bgc = sig.priority === "critical" ? BADGE_COLORS.red.bg : sig.priority === "high" ? BADGE_COLORS.amber.bg : "rgba(255,255,255,0.04)";
                        return (
                          <span key={sig.id} style={{
                            display: "inline-flex", alignItems: "center", gap: 5,
                            padding: "3px 10px", borderRadius: 4, fontSize: 11, fontWeight: 500,
                            background: bgc, color: pc, opacity: sig.read ? 0.4 : 1,
                            border: `1px solid ${bgc}`,
                          }}>
                            {sig.signal_type}
                            {!sig.read && (
                              <button onClick={(e) => { e.stopPropagation(); markSignalRead(sig.id); }}
                                style={{ background: "transparent", border: "none", color: pc, cursor: "pointer", padding: 0, lineHeight: 1 }}>×</button>
                            )}
                          </span>
                        );
                      })}
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}

          {/* ── FOOTER ACTIONS ── */}
          <div style={{
            display: "flex", alignItems: "center", gap: 8,
            padding: "12px 16px",
            borderTop: `1px solid ${P.borderSubtle}`,
            opacity: animReady ? 1 : 0,
            transition: "opacity 0.4s 0.5s",
          }}>
            {/* Left cluster */}
            <button className="action-btn" onClick={(e) => { e.stopPropagation(); onViewDiff?.(run.id); }}
              style={{
                display: "flex", alignItems: "center", gap: 6,
                fontWeight: 500, color: P.textMuted, fontSize: 12,
                background: "transparent", border: `1px solid ${P.borderSubtle}`,
                borderRadius: 7, padding: "7px 12px", cursor: "pointer",
              }}>
              <BarChart size={13} /> Diff
            </button>
            <button className="action-btn"
              onClick={async (e) => {
                e.stopPropagation();
                setActionLoading("fork"); setActionError(null);
                try { const path = await forkRunWorktree(run.id); navigator.clipboard.writeText(path).catch(() => {}); }
                catch (err) { setActionError(err instanceof Error ? err.message : String(err)); }
                finally { setActionLoading(null); }
              }}
              disabled={actionLoading === "fork"}
              style={{
                display: "flex", alignItems: "center", gap: 6,
                fontWeight: 500, color: P.textMuted, fontSize: 12,
                background: "transparent", border: `1px solid ${P.borderSubtle}`,
                borderRadius: 7, padding: "7px 12px", cursor: "pointer",
                opacity: actionLoading === "fork" ? 0.5 : 1,
              }}>
              <ForkIcon size={13} /> {actionLoading === "fork" ? "Forking…" : "Fork"}
            </button>
            <button className="action-btn" onClick={(e) => { e.stopPropagation(); handleReport(); }}
              disabled={reportLoading}
              style={{
                display: "flex", alignItems: "center", gap: 6,
                fontWeight: 500, color: P.textMuted, fontSize: 12,
                background: "transparent", border: `1px solid ${P.borderSubtle}`,
                borderRadius: 7, padding: "7px 12px", cursor: "pointer",
                opacity: reportLoading ? 0.5 : 1,
              }}>
              <FileText size={13} /> {reportLoading ? "Loading…" : "Report"}
            </button>

            <div style={{ flex: 1 }} />

            {/* Right cluster */}
            {run.pr_url && (
              <button className="action-btn" onClick={(e) => { e.stopPropagation(); window.open(run.pr_url!, "_blank"); }}
                style={{
                  display: "flex", alignItems: "center", gap: 6,
                  fontSize: 12.5, fontWeight: 500,
                  color: P.blue, background: "transparent",
                  border: `1px solid ${P.blueBorder}`,
                  borderRadius: 7, padding: "7px 14px", cursor: "pointer",
                }}>
                <PullRequest size={13} /> Open PR
              </button>
            )}
            {canRetryPublish && (
              <button className="action-btn"
                onClick={async (e) => {
                  e.stopPropagation();
                  setActionLoading("publish"); setActionError(null);
                  try { const result = await retryPublishRun(run.id); if (result.pr_url) window.open(result.pr_url, "_blank"); }
                  catch (err) { setActionError(err instanceof Error ? err.message : String(err)); }
                  finally { setActionLoading(null); }
                }}
                disabled={actionLoading === "publish"}
                style={{
                  display: "flex", alignItems: "center", gap: 6,
                  fontSize: 12.5, fontWeight: 500,
                  color: run.publish_status === "failed" ? P.red : "#f59e0b",
                  background: "transparent",
                  border: `1px solid ${run.publish_status === "failed" ? "rgba(248,113,113,0.3)" : "rgba(245,158,11,0.25)"}`,
                  borderRadius: 7, padding: "7px 14px", cursor: "pointer",
                  opacity: actionLoading === "publish" ? 0.6 : 1,
                }}>
                <Refresh size={13} /> {actionLoading === "publish" ? "Publishing…" : "Retry Publish"}
              </button>
            )}
            {isResumable && (
              <button className="action-btn" onClick={(e) => { e.stopPropagation(); handleResume(); }}
                disabled={actionLoading === "resume"}
                style={{
                  display: "flex", alignItems: "center", gap: 6,
                  fontSize: 12.5, fontWeight: 600,
                  color: "#fff", background: P.accentMuted,
                  border: `1px solid ${P.accent}`,
                  borderRadius: 7, padding: "7px 16px", cursor: "pointer",
                  opacity: actionLoading === "resume" ? 0.6 : 1,
                }}>
                <Arrow size={13} /> {actionLoading === "resume" ? "Resuming…" : "Continue"}
              </button>
            )}
            {!isActive && run.conversation_id && onContinueTask && (
              <button className="action-btn" onClick={(e) => { e.stopPropagation(); onContinueTask(run.conversation_id!, run.id); }}
                style={{
                  display: "flex", alignItems: "center", gap: 6,
                  fontSize: 12.5, fontWeight: 600,
                  color: "#fff", background: P.accentMuted,
                  border: `1px solid ${P.accent}`,
                  borderRadius: 7, padding: "7px 16px", cursor: "pointer",
                }}>
                <Plus size={13} /> Continue task
              </button>
            )}
            {isActive && (
              <button className="action-btn" onClick={(e) => { e.stopPropagation(); handleAbort(); }}
                disabled={actionLoading === "abort"}
                style={{
                  display: "flex", alignItems: "center", gap: 6,
                  fontSize: 12.5, fontWeight: 600,
                  background: confirmAbort ? P.red : "transparent",
                  color: confirmAbort ? "#fff" : P.red,
                  border: `1px solid ${confirmAbort ? P.red : "rgba(248,113,113,0.3)"}`,
                  borderRadius: 7, padding: "7px 14px", cursor: "pointer",
                  opacity: actionLoading === "abort" ? 0.5 : 1,
                }}>
                <Undo size={13} /> {actionLoading === "abort" ? "Aborting…" : confirmAbort ? "Confirm abort" : "Abort"}
              </button>
            )}
          </div>

          {actionError && (
            <div style={{
              fontSize: 12, color: P.red, padding: "7px 12px", margin: "0 16px 12px",
              background: BADGE_COLORS.red.bg, borderRadius: 4, border: "1px solid rgba(248,113,113,0.15)",
            }}>
              {actionError}
            </div>
          )}
        </div>
      )}
    </div>
  );
});

// ── InlineLogsSection ────────────────────────────────────────────────────────

function InlineLogsSection({ sessionLogs, animReady }: { sessionLogs: { session: SessionRecord; entries: (LogEntry & { agentType?: string })[] }[]; animReady: boolean }) {
  const [expanded, setExpanded] = useState(true);
  if (sessionLogs.length === 0) return null;

  const agentTypes = sessionLogs.map(sl => sl.session.agent_type);
  const label = agentTypes.length <= 2 ? agentTypes.join(" + ") : `${agentTypes.length} agents`;
  const firstSession = sessionLogs[0]?.session;
  const lastSession = sessionLogs[sessionLogs.length - 1]?.session;
  const allDone = sessionLogs.every(sl => sl.session.state === "done" || sl.session.state === "completed");

  return (
    <div style={{
      margin: "8px 16px 0",
      borderTop: `1px solid ${P.borderSubtle}`,
      paddingTop: 16,
      opacity: animReady ? 1 : 0,
      transition: "opacity 0.4s 0.7s",
    }}>
      <button
        onClick={() => setExpanded(!expanded)}
        style={{
          display: "flex", alignItems: "center", gap: 8, width: "100%",
          background: "none", border: "none", cursor: "pointer",
          padding: "4px 0", marginBottom: expanded ? 10 : 0,
        }}
      >
        <span style={{ color: P.textFaint, display: "inline-flex" }}>
          {expanded ? <ChevronDown size={14} /> : <ChevronR size={14} />}
        </span>
        <span style={{ color: P.textFaint, display: "flex", alignItems: "center" }}><Terminal size={13} /></span>
        <span style={{ fontSize: 13, fontWeight: 500, color: P.text }}>Logs</span>
        <Badge color="gray" small>{label}</Badge>
        <span style={{
          marginLeft: "auto", fontSize: 10, fontWeight: 600, letterSpacing: "0.05em",
          color: allDone ? P.accent : P.blue,
        }}>
          {allDone ? "COMPLETED" : "RUNNING"}
        </span>
      </button>

      {expanded && (
        <div style={{
          background: P.bgSurface,
          border: `1px solid ${P.borderSubtle}`,
          borderRadius: 8,
          padding: "8px 14px",
          animation: "fadeIn 0.2s ease",
        }}>
          {firstSession && (
            <div style={{
              fontSize: 11, color: P.textFaint, fontFamily: C.mono, padding: "4px 0",
              borderBottom: `1px solid ${P.borderSubtle}`, marginBottom: 4,
            }}>
              {firstSession.id.slice(0, 12)}
              {firstSession.provider_session_id && ` · provider ${firstSession.provider_session_id.slice(0, 12)}`}
              {firstSession.started_at && ` · ${new Date(firstSession.started_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}`}
              {lastSession?.ended_at && ` → ${new Date(lastSession.ended_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}`}
            </div>
          )}

          {sessionLogs.flatMap(sl => sl.entries).slice(0, 20).map((entry, i) => {
            const levelColor: Record<string, string> = {
              system: P.blue, assistant: P.accent, result: P.accent,
              tool_use: "#a78bfa", tool_result: P.textMuted, raw: P.textFaint,
            };
            const level = entry.role === "system" ? "INIT" : entry.role === "result" ? "RUN" : entry.role === "tool_use" ? "TOOL" : entry.role === "assistant" ? "RUN" : "OUT";
            const color = levelColor[entry.role] ?? P.textFaint;
            const content = sanitizeLogContent(entry.content ?? "");
            if (!content && !entry.tool_name) return null;
            return (
              <div key={i} style={{ display: "flex", gap: 10, padding: "5px 0", alignItems: "flex-start" }}>
                <span style={{ fontFamily: C.mono, fontSize: 10, fontWeight: 600, letterSpacing: "0.05em", color, minWidth: 32, paddingTop: 2 }}>
                  {level}
                </span>
                <span style={{ fontFamily: C.mono, fontSize: 12, color: P.textMuted, wordBreak: "break-all", flex: 1, minWidth: 0 }}>
                  {entry.tool_name ? `${entry.tool_name}${content ? ` · ${content.slice(0, 80)}` : ""}` : content.slice(0, 120)}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ── RunThread ────────────────────────────────────────────────────────────────

function RunThread({ runId, sessions, dbMessages, runState, permissionMode, liveEntries, animReady }: {
  runId: string;
  sessions: SessionRecord[] | undefined;
  dbMessages: MessageRow[] | undefined;
  runState: string;
  permissionMode?: string;
  liveEntries?: (LogEntry & { agentType?: string })[];
  animReady?: boolean;
}) {
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const isActive = ACTIVE_STATES.includes(runState);
  const mode = (permissionMode ?? "skip_all") as "skip_all" | "human_gate" | "autonomous_gate";

  const sessionIds = sessions?.map(s => s.id) ?? [];
  const { data: logEntries } = useQuery({
    queryKey: ["sessionLogs", runId, ...sessionIds],
    queryFn: async () => {
      if (!sessions || sessions.length === 0) return [];
      const results = await Promise.all(
        sessions.map(async (s) => {
          const entries = await readSessionLog(runId, s.id);
          return entries.map(e => ({ ...e, agentType: s.agent_type }));
        })
      );
      return results.flat();
    },
    enabled: !!sessions && sessions.length > 0 && !isActive,
    staleTime: 30000,
  });

  const { data: qaMessages } = useQuery({
    queryKey: ["qaMessages", runId],
    queryFn: () => listQaMessages(runId),
    refetchInterval: isActive ? 3000 : false,
    staleTime: 5000,
  });

  const curatedLiveEntries = curateThreadEntries(liveEntries ?? []);
  const curatedReplayEntries = curateThreadEntries(logEntries ?? []);
  const entries = isActive ? curatedLiveEntries : curatedReplayEntries;
  const pendingQuestions = (qaMessages ?? []).filter(
    (m: QaMessageDto) => m.direction === "question" && !(qaMessages ?? []).some(
      (a: QaMessageDto) => a.direction === "answer" && a.created_at > m.created_at
    )
  );

  useEffect(() => {
    const el = scrollContainerRef.current;
    if (!el) return;
    const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
    if (nearBottom) el.scrollTop = el.scrollHeight;
  }, [entries.length, qaMessages?.length]);

  const hasContent = entries.length > 0 || (dbMessages && dbMessages.length > 0);

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* Activity header */}
      <div style={{
        padding: "14px 16px 10px", display: "flex", alignItems: "flex-start", gap: 8,
      }}>
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: 14, fontWeight: 600, color: P.text, marginBottom: 2 }}>Activity</div>
          <div style={{ fontSize: 12, color: P.textFaint }}>Live thread and gate history for this run</div>
        </div>
        <span style={{
          fontSize: 10, fontWeight: 600, letterSpacing: "0.05em",
          padding: "3px 10px", borderRadius: 4,
          color: isActive ? P.blue : runState === "completed" ? P.accent : runState === "failed" ? P.red : P.textFaint,
          background: isActive ? BADGE_COLORS.blue.bg : runState === "completed" ? BADGE_COLORS.green.bg : runState === "failed" ? BADGE_COLORS.red.bg : "rgba(255,255,255,0.04)",
          border: `1px solid ${isActive ? BADGE_COLORS.blue.border : runState === "completed" ? BADGE_COLORS.green.border : runState === "failed" ? BADGE_COLORS.red.border : "rgba(255,255,255,0.08)"}`,
        }}>
          {isActive ? "RUNNING" : runState === "completed" ? "COMPLETED" : runState === "failed" ? "FAILED" : runState.toUpperCase()}
        </span>
      </div>

      {/* Status bar */}
      <div style={{
        padding: "4px 16px 8px", display: "flex", alignItems: "center", gap: 8,
        borderBottom: `1px solid ${P.borderSubtle}`,
      }}>
        <span style={{
          width: 6, height: 6, borderRadius: "50%", flexShrink: 0,
          background: isActive ? P.accent : P.textFaint,
        }} />
        <span style={{ fontSize: 11, color: P.textMuted, fontWeight: 500 }}>
          {isActive ? "Live" : runState}
        </span>
        <span style={{ marginLeft: "auto" }}>
          <Badge color="gray" small>
            {mode === "human_gate" ? "HUMAN" : mode === "autonomous_gate" ? "GATEKEEPER" : "AUTO"}
          </Badge>
        </span>
      </div>

      {/* Message stream */}
      <div ref={scrollContainerRef} style={{ flex: 1, overflowY: "auto", maxHeight: 400 }}>
        {!hasContent && (
          <div style={{ padding: "32px 0", textAlign: "center", color: P.textFaint, fontSize: 12 }}>
            {isActive ? "Waiting for agent output..." : "No conversation log for this run"}
          </div>
        )}

        {entries.length > 0 && (
          <div style={{ paddingTop: 4 }}>
            {entries.map((entry, i) => (
              <LogEntryRow key={i} entry={entry} agentType={entry.agentType} staggerDelay={animReady ? 0.3 + i * 0.05 : undefined} animReady={animReady} />
            ))}
          </div>
        )}

        {/* Fallback: DB messages */}
        {entries.length === 0 && dbMessages && dbMessages.length > 0 && (
          <div style={{ paddingTop: 4 }}>
            {dbMessages.map(msg => {
              const isUser = msg.role === "user";
              const isSystem = msg.role === "system";
              if (isSystem) {
                return (
                  <div key={msg.id} style={{ padding: "6px 12px", display: "flex", alignItems: "center", gap: 10 }}>
                    <div style={{ flex: 1, height: 1, background: "rgba(255,255,255,0.04)" }} />
                    <span style={{ fontSize: 10, color: P.textFaint, fontStyle: "italic", flexShrink: 0 }}>
                      {msg.content.length > 80 ? msg.content.slice(0, 80) + "\u2026" : msg.content}
                    </span>
                    <div style={{ flex: 1, height: 1, background: "rgba(255,255,255,0.04)" }} />
                  </div>
                );
              }
              const badgeC = isUser ? "green" : "blue";
              const borderC = isUser ? BADGE_COLORS.green.text : BADGE_COLORS.blue.text;
              return (
                <div key={msg.id} style={{
                  padding: "12px 14px", margin: "8px 12px", borderRadius: 8,
                  background: P.bgSurface,
                  border: `1px solid ${P.borderSubtle}`,
                  borderLeft: `3px solid ${borderC}`,
                }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
                    <Badge color={badgeC} small>{msg.role}</Badge>
                    <span style={{ fontSize: 10, color: P.textFaint }}>{relativeTime(msg.created_at)}</span>
                  </div>
                  <p style={{ fontSize: 13, color: P.textMuted, lineHeight: 1.55, paddingLeft: 2, margin: 0, whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
                    {msg.content}
                  </p>
                </div>
              );
            })}
          </div>
        )}

        {pendingQuestions.length > 0 && (
          <div style={{ padding: "0 16px" }}>
            {pendingQuestions.map(q => (
              <QaCard key={q.id} runId={runId} question={q.content}
                options={q.options_json ? JSON.parse(q.options_json) : []}
                blocking={true} permissionMode={mode} isRunActive={isActive}
              />
            ))}
          </div>
        )}
      </div>

      {isActive && mode !== "skip_all" && (
        <ThreadInputBar runId={runId} disabled={pendingQuestions.length === 0 && mode === "human_gate"} />
      )}
    </div>
  );
}

// ── SessionLogsTab ───────────────────────────────────────────────────────────

function SessionLogsTab({
  sessionLogs,
  isActive,
  pipeline,
}: {
  sessionLogs: { session: SessionRecord; entries: (LogEntry & { agentType?: string })[] }[];
  isActive: boolean;
  pipeline?: string | null;
}) {
  const activeSessionId = sessionLogs.find(({ session }) => session.state === "running")?.session.id ?? null;
  const [expandedSessionId, setExpandedSessionId] = useState<string | null>(activeSessionId);

  useEffect(() => {
    if (activeSessionId) setExpandedSessionId(current => current ?? activeSessionId);
  }, [activeSessionId]);

  if (sessionLogs.length === 0) {
    return (
      <div style={{ padding: "28px 16px", color: P.textFaint, fontSize: 12, textAlign: "center" }}>
        No session logs available yet.
      </div>
    );
  }

  return (
    <div style={{ padding: "12px 16px", display: "flex", flexDirection: "column", gap: 12 }}>
      {sessionLogs.map(({ session, entries }) => {
        const stateTone = statusColor(session.state);
        const isRunningSession = session.state === "running";
        const isSessionExpanded = isRunningSession || expandedSessionId === session.id;
        const previewEntry = [...entries].reverse().find(entry => {
          const content = sanitizeLogContent(entry.content ?? "");
          return Boolean(content) || Boolean(entry.tool_name);
        });
        const previewText = previewEntry
          ? previewEntry.tool_name
            ? `${previewEntry.tool_name}${previewEntry.content ? ` · ${sanitizeLogContent(previewEntry.content).slice(0, 80)}` : ""}`
            : sanitizeLogContent(previewEntry.content || previewEntry.detail || "").slice(0, 100)
          : null;

        return (
          <div key={session.id} style={{
            borderRadius: 10, border: `1px solid ${P.borderSubtle}`,
            background: P.bgSurface, overflow: "hidden",
          }}>
            <div
              style={{
                display: "flex", alignItems: "center", gap: 10,
                padding: "10px 14px",
                borderBottom: isSessionExpanded ? `1px solid ${P.borderSubtle}` : "none",
                cursor: "pointer",
              }}
              onClick={() => setExpandedSessionId(current => current === session.id ? null : session.id)}
            >
              <span style={{ color: P.textFaint, display: "flex", alignItems: "center" }}><Terminal size={12} /></span>
              <div style={{ minWidth: 0, flex: 1 }}>
                <div style={{ fontSize: 12, fontWeight: 500, color: P.textMuted }}>{formatRunAgentLabel(session.agent_type, pipeline)}</div>
                <div style={{ fontSize: 10, color: P.textFaint, fontFamily: C.mono, display: "flex", gap: 8, flexWrap: "wrap" }}>
                  <span>{session.id.slice(0, 12)}</span>
                  {session.provider_session_id && <span>provider {session.provider_session_id.slice(0, 12)}</span>}
                  {session.started_at && (
                    <span>
                      {new Date(session.started_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                      {session.ended_at && ` → ${new Date(session.ended_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}`}
                    </span>
                  )}
                </div>
                {!isSessionExpanded && previewText && (
                  <div style={{ marginTop: 4, fontSize: 11, color: P.textFaint, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                    {previewText}
                  </div>
                )}
              </div>
              {isRunningSession && (
                <span style={{ fontSize: 9, fontWeight: 700, letterSpacing: "0.06em", textTransform: "uppercase", padding: "4px 8px", borderRadius: 999, background: BADGE_COLORS.blue.bg, color: P.blue }}>
                  Live
                </span>
              )}
              <span style={{ fontSize: 10, fontWeight: 700, letterSpacing: "0.06em", textTransform: "uppercase", padding: "4px 8px", borderRadius: 999, background: stateTone.bg, color: stateTone.text }}>
                {session.state}
              </span>
              <span style={{ color: P.textFaint, transform: isSessionExpanded ? "rotate(90deg)" : "rotate(0deg)", transition: "transform 0.15s ease", display: "inline-flex" }}>
                <ChevronR size={12} />
              </span>
            </div>

            {isSessionExpanded && (
              <div style={{ padding: "12px" }}>
                {entries.length > 0 ? (
                  entries.map((entry, index) => (
                    <RichLogEntryRow key={`${session.id}-${index}`} entry={entry} agentType={formatRunAgentLabel(session.agent_type, pipeline)} />
                  ))
                ) : (
                  <div style={{ fontSize: 12, color: P.textFaint }}>
                    {isActive ? "Waiting for session log events..." : "No parsed session log entries."}
                  </div>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

// ── RichLogEntryRow ──────────────────────────────────────────────────────────

function RichLogEntryRow({ entry, agentType }: { entry: LogEntry; agentType?: string }) {
  const sanitizedContent = sanitizeLogContent(entry.content ?? "");
  const metadata = parseMetadataJson(entry.metadata_json);
  const label = entry.subtype ?? entry.event_type ?? entry.role;
  const badgeColorName =
    entry.role === "system" ? "amber" :
      entry.role === "assistant" ? "blue" :
        entry.role === "tool_use" ? "purple" :
          entry.role === "tool_result" ? "gray" :
            entry.role === "result" ? "green" :
              "gray";

  return (
    <div className="activity-row" style={{
      padding: "12px 12px", borderBottom: `1px solid ${P.borderSubtle}`, borderRadius: 6,
    }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap", marginBottom: sanitizedContent || entry.tool_name || entry.detail ? 6 : 0 }}>
        <Badge color={badgeColorName} small>{label}</Badge>
        {entry.line_no != null && <span style={{ fontSize: 10, color: P.textFaint, fontFamily: C.mono }}>line {entry.line_no}</span>}
        {agentType && <span style={{ fontSize: 12, color: P.textFaint, fontWeight: 500 }}>{agentType}</span>}
        {entry.cost_usd != null && <span style={{ fontSize: 10, color: P.textFaint, fontFamily: C.mono, marginLeft: "auto" }}>${entry.cost_usd.toFixed(4)}</span>}
        {entry.is_error && (
          <span style={{
            fontSize: 10, fontWeight: 600, letterSpacing: "0.06em",
            padding: "2px 8px", borderRadius: 4, marginLeft: entry.cost_usd != null ? 0 : "auto",
            background: BADGE_COLORS.red.bg, border: `1px solid ${BADGE_COLORS.red.border}`, color: P.red,
          }}>ERROR</span>
        )}
      </div>
      {entry.tool_name && (
        <div style={{ fontSize: 11, color: P.textMuted, marginBottom: 4, fontFamily: C.mono }}>
          <code style={{ fontSize: 12, color: "#a78bfa", background: BADGE_COLORS.purple.bg, padding: "1px 5px", borderRadius: 3 }}>{entry.tool_name}</code>
        </div>
      )}
      {sanitizedContent && (
        <p style={{ fontSize: 13, color: P.textMuted, lineHeight: 1.55, paddingLeft: 2, margin: 0, whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
          {sanitizedContent}
        </p>
      )}
      {entry.detail && (
        <p style={{ margin: "4px 0 0", paddingLeft: 2, fontSize: 12, color: P.textFaint, whiteSpace: "pre-wrap", lineHeight: 1.5 }}>
          {entry.detail}
        </p>
      )}
      {metadata && Object.keys(metadata).length > 0 && (
        <details style={{ marginTop: 8 }}>
          <summary style={{ cursor: "pointer", fontSize: 11, color: P.textFaint, fontFamily: C.mono }}>details</summary>
          <pre style={{
            margin: "8px 0 0", padding: "10px 12px", borderRadius: 6,
            background: "rgba(255,255,255,0.03)", border: `1px solid ${P.borderSubtle}`,
            fontSize: 11, color: P.textMuted, whiteSpace: "pre-wrap", wordBreak: "break-word",
            fontFamily: C.mono, lineHeight: 1.5,
          }}>
            {JSON.stringify(metadata, null, 2)}
          </pre>
        </details>
      )}
    </div>
  );
}

// ── LogEntryRow ──────────────────────────────────────────────────────────────

function LogEntryRow({ entry, agentType, staggerDelay, animReady }: { entry: LogEntry; agentType?: string; staggerDelay?: number; animReady?: boolean }) {
  const sanitizedContent = sanitizeLogContent(entry.content ?? "");
  const stagger: React.CSSProperties = staggerDelay != null ? {
    opacity: animReady ? 1 : 0,
    transform: animReady ? "translateY(0)" : "translateY(6px)",
    transition: `all 0.4s ${EASE} ${staggerDelay}s`,
  } : {};

  // System messages as subtle dividers
  if (entry.role === "system") {
    return (
      <div style={{ padding: "6px 0", display: "flex", alignItems: "center", gap: 10, ...stagger }}>
        <div style={{ flex: 1, height: 1, background: "rgba(255,255,255,0.04)" }} />
        <span style={{ fontSize: 10, color: P.textFaint, fontStyle: "italic", flexShrink: 0 }}>
          {entry.content.length > 100 ? entry.content.slice(0, 100) + "\u2026" : entry.content}
        </span>
        <div style={{ flex: 1, height: 1, background: "rgba(255,255,255,0.04)" }} />
      </div>
    );
  }

  const cardStyle = (borderColor: string): React.CSSProperties => ({
    padding: "12px 14px", margin: "8px 12px", borderRadius: 8,
    background: P.bgSurface,
    border: `1px solid ${P.borderSubtle}`,
    borderLeft: `3px solid ${borderColor}`,
    ...stagger,
  });

  if (entry.role === "tool_use") {
    return (
      <div style={cardStyle("#a78bfa")}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: sanitizedContent ? 6 : 0 }}>
          <Badge color="purple" small>tool</Badge>
          <span style={{ fontSize: 12, color: P.textFaint, fontWeight: 500, fontFamily: C.mono }}>{entry.tool_name ?? "tool"}</span>
        </div>
        {sanitizedContent && (
          <p style={{ fontSize: 13, color: P.textMuted, lineHeight: 1.55, paddingLeft: 2, margin: 0 }}>
            {sanitizedContent.length > 200 ? sanitizedContent.slice(0, 200) + "\u2026" : sanitizedContent}
          </p>
        )}
      </div>
    );
  }

  if (entry.role === "tool_result") {
    if (!sanitizedContent) return null;
    return (
      <div style={cardStyle(P.textFaint)}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
          <Badge color="gray" small>result</Badge>
          {entry.tool_name && <span style={{ fontSize: 12, color: P.textFaint, fontWeight: 500, fontFamily: C.mono }}>{entry.tool_name}</span>}
        </div>
        <p style={{ fontSize: 13, color: P.textMuted, lineHeight: 1.55, paddingLeft: 2, margin: 0 }}>
          {sanitizedContent.length > 200 ? sanitizedContent.slice(0, 200) + "\u2026" : sanitizedContent}
        </p>
      </div>
    );
  }

  if (entry.role === "result") {
    if (!sanitizedContent) return null;
    const isError = entry.is_error;
    return (
      <div style={cardStyle(isError ? P.red : P.accent)}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
          <Badge color={isError ? "amber" : "green"} small>{isError ? "Error" : "Result"}</Badge>
          {agentType && <span style={{ fontSize: 12, color: P.textFaint, fontWeight: 500 }}>· {agentType}</span>}
          {entry.cost_usd != null && <span style={{ fontSize: 10, color: P.textFaint, fontFamily: C.mono, marginLeft: "auto" }}>${entry.cost_usd.toFixed(4)}</span>}
        </div>
        <p style={{ fontSize: 13, color: P.textMuted, lineHeight: 1.55, paddingLeft: 2, margin: 0 }}>{sanitizedContent}</p>
      </div>
    );
  }

  // Assistant messages
  if (!sanitizedContent) return null;
  const badgeColor = agentBadgeColor(agentType);
  const displayLabel = agentType ?? "Assistant";
  const borderAccent = BADGE_COLORS[badgeColor]?.text ?? P.textFaint;

  return (
    <div style={cardStyle(borderAccent)}>
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
        <Badge color={badgeColor} small>{displayLabel}</Badge>
        {entry.is_error && (
          <span style={{
            marginLeft: "auto", fontSize: 10, fontWeight: 600, letterSpacing: "0.06em",
            padding: "2px 8px", borderRadius: 4,
            background: BADGE_COLORS.red.bg, border: `1px solid ${BADGE_COLORS.red.border}`, color: P.red,
          }}>ERROR</span>
        )}
        {entry.cost_usd != null && <span style={{ fontSize: 10, color: P.textFaint, fontFamily: C.mono, marginLeft: entry.is_error ? 0 : "auto" }}>${entry.cost_usd.toFixed(4)}</span>}
      </div>
      <p style={{ fontSize: 13, color: P.textMuted, lineHeight: 1.55, paddingLeft: 2, margin: 0 }}>{sanitizedContent}</p>
    </div>
  );
}

// ── ThreadInputBar ───────────────────────────────────────────────────────────

function ThreadInputBar({ runId, disabled }: { runId: string; disabled: boolean }) {
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);

  const handleSend = async () => {
    if (!text.trim() || sending) return;
    setSending(true);
    try { await sendAgentMessage(runId, text.trim()); setText(""); } finally { setSending(false); }
  };

  return (
    <div style={{ padding: "8px 16px", borderTop: `1px solid ${P.border}`, display: "flex", gap: 6 }}>
      <input
        type="text" value={text}
        onChange={e => setText(e.target.value)}
        onKeyDown={e => { if (e.key === "Enter") void handleSend(); }}
        placeholder={disabled ? "Waiting for agent question..." : "Type your answer..."}
        disabled={disabled || sending}
        style={{
          flex: 1, padding: "6px 10px", borderRadius: 6, fontSize: 12,
          background: P.bgCard, border: `1px solid ${P.border}`, color: P.text,
          outline: "none", opacity: disabled ? 0.5 : 1,
        }}
      />
      <button onClick={() => void handleSend()}
        disabled={disabled || sending || !text.trim()}
        style={{
          padding: "6px 14px", borderRadius: 6, fontSize: 12, fontWeight: 600,
          background: text.trim() && !disabled ? P.accent : P.bgSurface,
          border: "none", color: text.trim() && !disabled ? "#fff" : P.textFaint,
          cursor: text.trim() && !disabled && !sending ? "pointer" : "not-allowed",
        }}>
        {sending ? "..." : "Send"}
      </button>
    </div>
  );
}

// ── Helper functions ─────────────────────────────────────────────────────────

function sanitizeLogContent(content: string): string {
  const lines = content
    .split("\n")
    .filter((line) => {
      const trimmed = line.trim();
      if (trimmed === "```" || trimmed === "```json" || trimmed === "```jsonc") return false;
      return !(trimmed.startsWith("{") && trimmed.includes("\"grove_control\""));
    });
  return lines.join("\n").trim();
}

function curateThreadEntries(entries: (LogEntry & { agentType?: string })[]): (LogEntry & { agentType?: string })[] {
  return entries.filter((entry) => {
    if (entry.role === "assistant" || entry.role === "result") return Boolean(sanitizeLogContent(entry.content ?? ""));
    if (entry.role === "system") return isNarrativeSystemMessage(entry.content ?? "");
    return false;
  });
}

function isNarrativeSystemMessage(content: string): boolean {
  const text = content.trim().toLowerCase();
  if (!text) return false;
  if (text.startsWith("session initialized") || text.startsWith("claude code ") || text.startsWith("integrations need auth")) return false;
  return true;
}

function parseMetadataJson(metadataJson?: string | null): Record<string, unknown> | null {
  if (!metadataJson) return null;
  try {
    const parsed = JSON.parse(metadataJson);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) return parsed as Record<string, unknown>;
    return { value: parsed };
  } catch { return { raw: metadataJson }; }
}

// ── Helper components ────────────────────────────────────────────────────────

const STEP_STATUS_COLORS: Record<string, string> = {
  pending: P.textFaint, running: P.blue, completed: P.accent, done: P.accent,
  failed: P.red, queued: "#f59e0b", merged: P.accent,
};

function StepStatusDot({ status }: { status: string }) {
  const color = STEP_STATUS_COLORS[status] ?? P.textFaint;
  return <span style={{ width: 6, height: 6, borderRadius: "50%", background: color, flexShrink: 0, display: "inline-block" }} />;
}

function CollapsibleSection({ title, count, isOpen, onToggle, children }: {
  title: string; count: number; isOpen: boolean; onToggle: () => void; children: React.ReactNode;
}) {
  return (
    <div style={{ marginTop: 12 }}>
      <button
        onClick={(e) => { e.stopPropagation(); onToggle(); }}
        style={{ width: "100%", background: "transparent", border: "none", display: "flex", alignItems: "center", gap: 8, cursor: "pointer", padding: "4px 0" }}
      >
        <span style={{ transform: isOpen ? "rotate(90deg)" : "", transition: "transform 0.15s", display: "inline-flex", color: P.textFaint }}>
          <ChevronR size={10} />
        </span>
        <span style={{ fontSize: 12, fontWeight: 600, color: P.textMuted }}>{title}</span>
        <span style={{
          fontSize: 10, fontWeight: 600, color: P.textFaint, fontFamily: C.mono,
          background: "rgba(255,255,255,0.04)", padding: "1px 6px", borderRadius: 4,
          border: "1px solid rgba(255,255,255,0.08)",
        }}>{count}</span>
        <div style={{ flex: 1, height: 1, background: isOpen ? "transparent" : P.borderSubtle }} />
      </button>
      {isOpen && <div style={{ paddingBottom: 4 }}>{children}</div>}
    </div>
  );
}

// ── Event rendering helpers ──────────────────────────────────────────────────

interface EvCatConfig { color: string; bg: string; label: string }

const EV_CAT: Record<string, EvCatConfig> = {
  run_created:               { color: P.accent,    bg: BADGE_COLORS.green.bg,  label: "run" },
  run_completed:             { color: P.accent,    bg: BADGE_COLORS.green.bg,  label: "run" },
  run_failed:                { color: P.red,       bg: BADGE_COLORS.red.bg,    label: "run" },
  issue_linked:              { color: P.accent,    bg: BADGE_COLORS.green.bg,  label: "run" },
  plan_generated:            { color: P.accent,    bg: BADGE_COLORS.green.bg,  label: "run" },
  run_state_changed:         { color: P.textMuted, bg: "rgba(255,255,255,0.06)",label: "run" },
  run_publish_state_changed: { color: P.blue,      bg: BADGE_COLORS.blue.bg,   label: "publish" },
  session_spawned:           { color: "#a78bfa",   bg: BADGE_COLORS.purple.bg, label: "agent" },
  session_state_changed:     { color: "#a78bfa",   bg: BADGE_COLORS.purple.bg, label: "agent" },
  conv_branch_stale:         { color: "#f59e0b",   bg: BADGE_COLORS.amber.bg,  label: "git" },
  pre_run_merge_clean:       { color: P.accent,    bg: BADGE_COLORS.green.bg,  label: "git" },
  pre_run_merge_conflict:    { color: "#f59e0b",   bg: BADGE_COLORS.amber.bg,  label: "git" },
  pre_run_conflict_resolved: { color: P.accent,    bg: BADGE_COLORS.green.bg,  label: "git" },
  pre_run_conflict_failed:   { color: P.red,       bg: BADGE_COLORS.red.bg,    label: "git" },
  conv_merged:               { color: P.accent,    bg: BADGE_COLORS.green.bg,  label: "git" },
  conv_rebased:              { color: P.blue,      bg: BADGE_COLORS.blue.bg,   label: "git" },
  pre_publish_pull_clean:    { color: P.accent,    bg: BADGE_COLORS.green.bg,  label: "git" },
  pre_publish_pull_conflict: { color: "#f59e0b",   bg: BADGE_COLORS.amber.bg,  label: "git" },
  pre_publish_pull_resolved: { color: P.accent,    bg: BADGE_COLORS.green.bg,  label: "git" },
  pre_publish_pull_failed:   { color: P.red,       bg: BADGE_COLORS.red.bg,    label: "git" },
  pre_publish_pull_skipped:  { color: P.textMuted, bg: "rgba(255,255,255,0.06)",label: "git" },
  merge_queued:              { color: P.blue,      bg: BADGE_COLORS.blue.bg,   label: "merge" },
  merge_started:             { color: P.blue,      bg: BADGE_COLORS.blue.bg,   label: "merge" },
  merge_completed:           { color: P.accent,    bg: BADGE_COLORS.green.bg,  label: "merge" },
  merge_failed:              { color: P.red,       bg: BADGE_COLORS.red.bg,    label: "merge" },
  merge_conflict:            { color: "#f59e0b",   bg: BADGE_COLORS.amber.bg,  label: "merge" },
  git_push_recovery_started:   { color: "#f59e0b", bg: BADGE_COLORS.amber.bg,  label: "git" },
  git_push_recovery_completed: { color: P.accent,  bg: BADGE_COLORS.green.bg,  label: "git" },
  git_push_recovery_failed:    { color: P.red,     bg: BADGE_COLORS.red.bg,    label: "git" },
  watchdog_stalled:               { color: "#f59e0b", bg: BADGE_COLORS.amber.bg,  label: "watchdog" },
  watchdog_zombie:                { color: "#f59e0b", bg: BADGE_COLORS.amber.bg,  label: "watchdog" },
  watchdog_boot_timeout:          { color: P.red,     bg: BADGE_COLORS.red.bg,    label: "watchdog" },
  watchdog_lifetime_exceeded:     { color: "#f59e0b", bg: BADGE_COLORS.amber.bg,  label: "watchdog" },
  watchdog_run_lifetime_exceeded: { color: P.red,     bg: BADGE_COLORS.red.bg,    label: "watchdog" },
  checkpoint_saved: { color: P.textMuted, bg: "rgba(255,255,255,0.06)", label: "system" },
  crash_recovery:   { color: P.red,       bg: BADGE_COLORS.red.bg,     label: "system" },
  lock_acquired:    { color: P.textFaint, bg: "rgba(255,255,255,0.04)", label: "system" },
  lock_released:    { color: P.textFaint, bg: "rgba(255,255,255,0.04)", label: "system" },
  guard_violation:  { color: P.red,       bg: BADGE_COLORS.red.bg,     label: "security" },
  signal_sent:      { color: "#a78bfa",   bg: BADGE_COLORS.purple.bg,  label: "signal" },
  signal_broadcast: { color: "#a78bfa",   bg: BADGE_COLORS.purple.bg,  label: "signal" },
};

function evCat(event_type: string): EvCatConfig {
  return EV_CAT[event_type] ?? { color: P.textMuted, bg: "rgba(255,255,255,0.06)", label: "other" };
}

interface EvSummary { title: string; chips: string[]; detail?: string; verdictColor?: string }

function evSummary(event_type: string, p: Record<string, unknown>, pipeline?: string | null): EvSummary {
  const s = (k: string) => p[k] != null ? String(p[k]) : "";

  switch (event_type) {
    case "run_created":
      return { title: "Run created", chips: [] };
    case "run_completed":
      return { title: "Run completed", chips: p.provider ? [s("provider")] : [] };
    case "run_failed":
      return { title: "Run failed", chips: [] };
    case "issue_linked":
      return { title: s("title") || "Issue linked", chips: [`#${s("issue_id")}`, s("provider")].filter(Boolean) };
    case "plan_generated": {
      const plan = Array.isArray(p.plan) ? (p.plan as string[]) : [];
      return { title: `Plan: ${plan.length} agent${plan.length !== 1 ? "s" : ""}`, chips: plan };
    }
    case "run_state_changed":
      return { title: `${s("from")} → ${s("to")}`, chips: [] };
    case "run_publish_state_changed":
      return { title: `Publish: ${s("publish_status")}`, chips: p.pr_url ? ["PR created"] : [] };
    case "session_spawned":
      return { title: `Spawned: ${formatRunAgentLabel(s("agent_type"), pipeline)}`, chips: [] };
    case "session_state_changed": {
      if (p.agent) {
        const agent = formatRunAgentLabel(s("agent"), pipeline);
        const verdict = s("verdict");
        const isPass = ["PASS", "APPROVED", "COMPLIANT", "COMPLETE"].includes(verdict);
        const isFail = ["FAIL", "FAILED", "REJECTED", "BLOCK", "NON_COMPLIANT"].includes(verdict);
        const verdictColor = isPass ? P.accent : isFail ? P.red : "#f59e0b";
        const detail =
          s("feedback") ||
          (Array.isArray(p.notes) ? (p.notes as string[]).join(" · ") : "") ||
          (Array.isArray(p.gaps) ? `Gaps: ${(p.gaps as string[]).join(", ")}` : "") ||
          (Array.isArray(p.findings) ? `Findings: ${(p.findings as string[]).join(", ")}` : "") ||
          s("note");
        return { title: `${agent} → ${verdict}`, chips: [], detail: detail || undefined, verdictColor };
      }
      return { title: `State → ${s("state")}`, chips: [] };
    }
    case "conv_branch_stale":
      return { title: `Branch stale · ${s("commits_behind")} commits behind ${s("default_branch")}`, chips: [] };
    case "pre_run_merge_clean":
      return { title: `Merged ${s("default_branch")} cleanly`, chips: p.merge_commit_sha ? [s("merge_commit_sha").slice(0, 8)] : [] };
    case "pre_run_merge_conflict": {
      const files = Array.isArray(p.conflicting_files) ? (p.conflicting_files as string[]) : [];
      const count = s("file_count") || String(files.length);
      return { title: `Merge conflict · ${count} file(s)`, chips: files.slice(0, 3).map(f => f.split("/").pop() ?? f) };
    }
    case "pre_run_conflict_resolved":   return { title: "Merge conflict resolved", chips: [] };
    case "pre_run_conflict_failed":     return { title: "Conflict resolution failed", chips: [] };
    case "conv_merged":                 return { title: "Branch merged", chips: [] };
    case "conv_rebased":                return { title: "Branch rebased", chips: [] };
    case "pre_publish_pull_clean":      return { title: "Pre-publish sync clean", chips: [] };
    case "pre_publish_pull_conflict":   return { title: "Pre-publish sync conflict", chips: [] };
    case "pre_publish_pull_resolved":   return { title: "Pre-publish conflict resolved", chips: [] };
    case "pre_publish_pull_failed":     return { title: "Pre-publish sync failed", chips: [] };
    case "pre_publish_pull_skipped":    return { title: "Pre-publish sync skipped", chips: [] };
    case "merge_queued":
      return { title: `Merge queued: ${s("branch_name") || s("branch")}`, chips: [] };
    case "merge_started":
      return { title: `Merge started: ${s("branch_name") || s("branch")}`, chips: [] };
    case "merge_completed":
      return { title: `Merged: ${s("branch_name") || s("branch")}`, chips: [] };
    case "merge_failed":
      return { title: `Merge failed: ${s("branch_name") || s("branch")}`, chips: [] };
    case "merge_conflict":
      return { title: "Merge conflict", chips: [] };
    case "git_push_recovery_started":   return { title: "Push recovery started", chips: [] };
    case "git_push_recovery_completed": return { title: "Push recovery completed", chips: [] };
    case "git_push_recovery_failed":    return { title: "Push recovery failed", chips: [] };
    case "watchdog_stalled":
      return { title: "Agent stalled", chips: [`${s("idle_secs")}s idle`] };
    case "watchdog_zombie":
      return { title: "Zombie agent detected", chips: [`${s("idle_secs")}s idle`] };
    case "watchdog_boot_timeout":
      return { title: "Boot timeout", chips: [] };
    case "watchdog_lifetime_exceeded":
      return { title: "Agent lifetime exceeded", chips: [`${s("elapsed_secs")}s`] };
    case "watchdog_run_lifetime_exceeded":
      return { title: "Run lifetime exceeded", chips: [`${s("elapsed_secs")}s`] };
    case "checkpoint_saved":
      return { title: `Checkpoint: ${s("stage") || s("checkpoint_id") || "saved"}`, chips: [] };
    case "crash_recovery":
      return { title: "Crash recovery", chips: [] };
    case "lock_acquired":
      return { title: `Lock: ${s("path") || s("file")}`, chips: [] };
    case "lock_released":
      return { title: `Released: ${s("path") || s("file")}`, chips: [] };
    case "guard_violation":
      return { title: "Guard violation", chips: [], detail: s("rule") || s("reason") };
    case "signal_sent":
      return { title: `Signal: ${s("signal_type") || s("type")}`, chips: [] };
    case "signal_broadcast":
      return { title: `Broadcast: ${s("signal_type") || s("type")}`, chips: [] };
    default:
      return { title: event_type.replace(/_/g, " "), chips: [] };
  }
}
