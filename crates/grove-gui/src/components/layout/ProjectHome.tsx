import {
  gitProjectBranchStatus,
  gitProjectGetPrStatus,
  gitProjectStatus,
  issueCountOpen,
  listAutomations,
  listConversations,
  type GitStatusEntry,
} from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { relativeTime } from "@/lib/hooks";
import { C } from "@/lib/theme";
import type { BranchStatus, ConversationRow, ProjectRow, PrStatus } from "@/types";
import { useQuery } from "@tanstack/react-query";
import { Sparkles, Zap } from "@/components/ui/icons";

interface ProjectHomeProps {
  project: ProjectRow;
  onNewRun: () => void;
  onSelectConversation: (id: string) => void;
}

// ── Palette ──────────────────────────────────────────────────────────────────

const BG = "#0e0f11";
const CARD = "#13151a";
const BORDER = "rgba(255,255,255,0.06)";
const TEXT1 = "rgba(255,255,255,0.88)";
const TEXT2 = "rgba(255,255,255,0.55)";
const TEXT3 = "rgba(255,255,255,0.28)";
const TEXT4 = "rgba(255,255,255,0.14)";
const ACCENT = "#31B97B";
const ACCENT_DIM = "rgba(49,185,123,0.1)";
const BLUE = "#60a5fa";
const BLUE_DIM = "rgba(96,165,250,0.1)";
const AMBER = "#f59e0b";
const AMBER_DIM = "rgba(245,158,11,0.1)";
const RED = "#f87171";
const RED_DIM = "rgba(248,113,113,0.1)";

const ACTIVE_STATES = ["executing", "waiting_for_gate", "planning", "verifying", "publishing", "merging"];

function shortenPath(p: string) {
  return p.replace(/^\/Users\/[^/]+/, "~");
}

function humanState(state: string): string {
  const map: Record<string, string> = {
    executing: "Running",
    planning: "Planning",
    verifying: "Verifying",
    publishing: "Publishing",
    merging: "Merging",
    waiting_for_gate: "Waiting",
    paused: "Paused",
    completed: "Completed",
    failed: "Failed",
    idle: "Idle",
    pending: "Pending",
  };
  return map[state] ?? (state.charAt(0).toUpperCase() + state.slice(1));
}

function stateColor(state: string): { text: string; bg: string; dot: string } {
  if (state === "completed") return { text: ACCENT, bg: ACCENT_DIM, dot: ACCENT };
  if (ACTIVE_STATES.includes(state)) return { text: BLUE, bg: BLUE_DIM, dot: BLUE };
  if (["waiting_for_gate", "paused"].includes(state)) return { text: AMBER, bg: AMBER_DIM, dot: AMBER };
  if (state === "failed") return { text: RED, bg: RED_DIM, dot: RED };
  return { text: TEXT3, bg: "rgba(255,255,255,0.04)", dot: TEXT3 };
}

// ── Skeleton ──────────────────────────────────────────────────────────────────

function Sk({ w, h, r = 4 }: { w?: string | number; h: number; r?: number }) {
  return (
    <div className="skeleton" style={{ width: w ?? "100%", height: h, borderRadius: r, flexShrink: 0 }} />
  );
}

// ── Stat card ─────────────────────────────────────────────────────────────────

function StatCard({
  label, value, sub, accent, loading,
}: { label: string; value: string | number; sub?: string; accent?: string; loading?: boolean }) {
  return (
    <div style={{
      background: CARD, border: `1px solid ${BORDER}`,
      borderRadius: 8, padding: "14px 16px", flex: 1, minWidth: 0,
    }}>
      <div style={{ fontSize: 10.5, fontWeight: 600, letterSpacing: "0.06em", textTransform: "uppercase", color: TEXT3, marginBottom: 8 }}>
        {label}
      </div>
      {loading ? (
        <>
          <Sk h={20} w={48} r={4} />
          <div style={{ marginTop: 6 }}><Sk h={10} w={64} r={3} /></div>
        </>
      ) : (
        <>
          <div style={{ fontSize: 20, fontWeight: 700, color: accent ?? TEXT1, lineHeight: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {value}
          </div>
          {sub && <div style={{ fontSize: 11, color: TEXT3, marginTop: 5 }}>{sub}</div>}
        </>
      )}
    </div>
  );
}

// ── Live session row ──────────────────────────────────────────────────────────

function LiveRow({ conv, index, onClick }: { conv: ConversationRow; index: number; onClick: () => void }) {
  const sc = stateColor(conv.state);
  return (
    <button
      onClick={onClick}
      style={{
        width: "100%", textAlign: "left", background: "transparent", border: "none",
        borderTop: index > 0 ? "1px solid rgba(96,165,250,0.08)" : "none",
        padding: "10px 16px", cursor: "pointer", fontFamily: "inherit",
        display: "flex", alignItems: "center", gap: 12, transition: "background 0.1s",
      }}
      onMouseEnter={e => { e.currentTarget.style.background = "rgba(96,165,250,0.05)"; }}
      onMouseLeave={e => { e.currentTarget.style.background = "transparent"; }}
    >
      {/* Pulsing dot */}
      <div style={{ position: "relative", width: 8, height: 8, flexShrink: 0 }}>
        <div
          className="animate-ping"
          style={{ position: "absolute", inset: 0, borderRadius: "50%", background: sc.dot, opacity: 0.4 }}
        />
        <div style={{ position: "absolute", inset: 0, borderRadius: "50%", background: sc.dot }} />
      </div>

      {/* Title */}
      <span style={{ flex: 1, fontSize: 12.5, fontWeight: 500, color: TEXT1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
        {conv.title || `Session ${conv.id.slice(0, 8)}`}
      </span>

      {/* State pill */}
      <span style={{
        fontSize: 10, fontWeight: 600, letterSpacing: "0.04em",
        padding: "2px 8px", borderRadius: 4, background: sc.bg, color: sc.text, flexShrink: 0,
      }}>
        {humanState(conv.state)}
      </span>

      {/* Time */}
      <span style={{ fontSize: 11, color: TEXT3, flexShrink: 0 }}>
        {relativeTime(conv.updated_at)}
      </span>
    </button>
  );
}

// ── Session row ───────────────────────────────────────────────────────────────

function SessionRow({ conv, index, onClick }: { conv: ConversationRow; index: number; onClick: () => void }) {
  const sc = stateColor(conv.state);
  const kindLabel = conv.conversation_kind === "cli" ? "CLI" : conv.conversation_kind === "hive_loom" ? "HIVE" : "RUN";
  const kindColor = conv.conversation_kind === "cli" ? BLUE : conv.conversation_kind === "hive_loom" ? AMBER : ACCENT;

  return (
    <button
      onClick={onClick}
      style={{
        width: "100%", textAlign: "left", background: "transparent", border: "none",
        borderTop: index > 0 ? `1px solid ${BORDER}` : "none",
        padding: "10px 16px", cursor: "pointer", fontFamily: "inherit",
        display: "flex", alignItems: "center", gap: 12, transition: "background 0.1s",
      }}
      onMouseEnter={e => { e.currentTarget.style.background = "rgba(255,255,255,0.025)"; }}
      onMouseLeave={e => { e.currentTarget.style.background = "transparent"; }}
    >
      {/* Status dot */}
      <div style={{ width: 6, height: 6, borderRadius: "50%", background: sc.dot, flexShrink: 0, marginTop: 1 }} />

      {/* Title + sub */}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 12.5, fontWeight: 400, color: TEXT1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
          {conv.title || `Session ${conv.id.slice(0, 8)}`}
        </div>
        <div style={{ fontSize: 11, color: TEXT3, marginTop: 2, display: "flex", alignItems: "center", gap: 5 }}>
          <span style={{ color: kindColor, fontWeight: 600, fontSize: 9.5, letterSpacing: "0.04em", textTransform: "uppercase" }}>
            {kindLabel}
          </span>
          <span style={{ color: TEXT4 }}>·</span>
          <span>{humanState(conv.state)}</span>
          {conv.branch_name && (
            <>
              <span style={{ color: TEXT4 }}>·</span>
              <span style={{ fontFamily: C.mono, fontSize: 10.5, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", maxWidth: 140 }}>
                {conv.branch_name}
              </span>
            </>
          )}
        </div>
      </div>

      {/* Time */}
      <span style={{ fontSize: 11, color: TEXT3, flexShrink: 0 }}>
        {relativeTime(conv.updated_at)}
      </span>
    </button>
  );
}

// ── Main ──────────────────────────────────────────────────────────────────────

export function ProjectHome({ project, onNewRun, onSelectConversation }: ProjectHomeProps) {
  const projectRoot = project.source_kind === "ssh" ? null : project.root_path;

  const { data: conversations, isLoading: convLoading } = useQuery({
    queryKey: qk.conversations(project.id, 20),
    queryFn: () => listConversations(20, project.id),
    staleTime: 30000,
    refetchInterval: 30000,
  });

  const { data: openIssues, isLoading: issuesLoading } = useQuery({
    queryKey: qk.openIssueCount(project.id),
    queryFn: () => issueCountOpen(project.id),
    staleTime: 30000,
    refetchInterval: 60000,
  });

  const { data: automations, isLoading: autoLoading } = useQuery({
    queryKey: qk.automations(project.id),
    queryFn: () => listAutomations(project.id),
    staleTime: 30000,
    refetchInterval: 60000,
  });

  const { data: branchStatus } = useQuery<BranchStatus>({
    queryKey: ["projectBranchStatus", projectRoot],
    queryFn: () => gitProjectBranchStatus(projectRoot!),
    enabled: !!projectRoot,
    staleTime: 30000,
    refetchInterval: 60000,
  });

  const { data: gitStatus } = useQuery<GitStatusEntry[]>({
    queryKey: ["projectGitStatus", projectRoot],
    queryFn: () => gitProjectStatus(projectRoot!),
    enabled: !!projectRoot,
    staleTime: 30000,
    refetchInterval: 60000,
  });

  const { data: prStatus } = useQuery<PrStatus | null>({
    queryKey: ["projectPrStatus", projectRoot],
    queryFn: () => gitProjectGetPrStatus(projectRoot!),
    enabled: !!projectRoot,
    staleTime: 60000,
    refetchInterval: 120000,
  });

  const activeSessions = conversations?.filter(c => ACTIVE_STATES.includes(c.state)) ?? [];
  const recentSessions = conversations?.filter(c => !ACTIVE_STATES.includes(c.state)).slice(0, 8) ?? [];
  const activeCount = activeSessions.length;
  const enabledAutomationCount = automations?.filter(a => a.enabled).length ?? 0;
  const changedFiles = gitStatus?.length ?? 0;
  const lastActive = conversations?.[0]?.updated_at;

  return (
    <div style={{ flex: 1, overflowY: "auto", background: BG, fontFamily: "'Geist', 'DM Sans', -apple-system, sans-serif" }}>
      <div style={{ maxWidth: 780, margin: "0 auto", padding: "32px 28px 48px" }}>

        {/* ── Header ── */}
        <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", marginBottom: 28 }}>
          <div>
            <h1 style={{ fontSize: 22, fontWeight: 700, color: TEXT1, margin: "0 0 5px", letterSpacing: "-0.02em" }}>
              {project.name ?? project.root_path.split("/").pop() ?? "Project"}
            </h1>
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span style={{ fontSize: 11.5, color: TEXT3, fontFamily: C.mono }}>
                {shortenPath(project.root_path)}
              </span>
              {lastActive && (
                <>
                  <span style={{ color: TEXT4, fontSize: 11 }}>·</span>
                  <span style={{ fontSize: 11, color: TEXT3 }}>active {relativeTime(lastActive)}</span>
                </>
              )}
            </div>
          </div>
          <button
            onClick={onNewRun}
            style={{
              display: "flex", alignItems: "center", gap: 7,
              padding: "8px 18px", borderRadius: 7,
              border: "1px solid rgba(62,207,142,0.24)",
              background: "rgba(62,207,142,0.1)", color: ACCENT,
              fontSize: 12.5, fontWeight: 600, cursor: "pointer", fontFamily: "inherit",
              transition: "background 0.14s, border-color 0.14s", flexShrink: 0,
              letterSpacing: "0.01em",
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
            <Sparkles size={12} /> New Session
          </button>
        </div>

        {/* ── Stat cards ── */}
        <div style={{ display: "flex", gap: 10, marginBottom: 20 }}>
          <StatCard
            label="Sessions"
            value={conversations?.length ?? "—"}
            sub={activeCount > 0 ? `${activeCount} running` : "none active"}
            accent={activeCount > 0 ? BLUE : undefined}
            loading={convLoading}
          />
          <StatCard
            label="Open Issues"
            value={openIssues ?? "—"}
            accent={openIssues && openIssues > 0 ? AMBER : undefined}
            loading={issuesLoading}
          />
          <StatCard
            label="Automations"
            value={enabledAutomationCount}
            sub={automations ? `${automations.length} total` : undefined}
            accent={enabledAutomationCount > 0 ? ACCENT : undefined}
            loading={autoLoading}
          />
          <StatCard
            label="Last Run"
            value={lastActive ? relativeTime(lastActive) : "—"}
            sub={conversations && conversations.length > 0 ? `${conversations.length} sessions` : undefined}
            loading={convLoading}
          />
        </div>

        {/* ── Git status strip ── */}
        {projectRoot && (branchStatus || gitStatus) && (
          <div style={{
            background: CARD, border: `1px solid ${BORDER}`, borderRadius: 8,
            padding: "10px 16px", marginBottom: 20,
            display: "flex", alignItems: "center", gap: 14, flexWrap: "wrap",
          }}>
            <div style={{ fontSize: 10, fontWeight: 700, letterSpacing: "0.06em", textTransform: "uppercase", color: TEXT4 }}>
              Git
            </div>

            {branchStatus && (
              <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <span style={{ fontSize: 12, color: BLUE, fontFamily: C.mono, fontWeight: 500 }}>
                  {branchStatus.branch}
                </span>
                {branchStatus.ahead > 0 && (
                  <span style={{ fontSize: 11, color: ACCENT }}>↑{branchStatus.ahead}</span>
                )}
                {branchStatus.behind > 0 && (
                  <span style={{ fontSize: 11, color: AMBER }}>↓{branchStatus.behind}</span>
                )}
              </div>
            )}

            <span style={{ width: 1, height: 12, background: BORDER, flexShrink: 0 }} />

            {changedFiles > 0 ? (
              <span style={{ fontSize: 11.5, color: AMBER }}>
                {changedFiles} uncommitted {changedFiles === 1 ? "file" : "files"}
              </span>
            ) : gitStatus ? (
              <span style={{ fontSize: 11.5, color: ACCENT }}>clean</span>
            ) : null}

            {prStatus && (
              <>
                <span style={{ width: 1, height: 12, background: BORDER, flexShrink: 0 }} />
                <div style={{ display: "flex", alignItems: "center", gap: 6, minWidth: 0 }}>
                  <span style={{
                    fontSize: 10, fontWeight: 700, letterSpacing: "0.04em",
                    padding: "1px 6px", borderRadius: 4, flexShrink: 0,
                    background: prStatus.state === "OPEN" ? BLUE_DIM : ACCENT_DIM,
                    color: prStatus.state === "OPEN" ? BLUE : ACCENT,
                  }}>
                    PR #{prStatus.number}
                  </span>
                  <span style={{ fontSize: 11.5, color: TEXT2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", maxWidth: 200 }}>
                    {prStatus.title}
                  </span>
                  <span style={{ fontSize: 11, color: ACCENT, flexShrink: 0 }}>+{prStatus.additions}</span>
                  <span style={{ fontSize: 11, color: RED, flexShrink: 0 }}>-{prStatus.deletions}</span>
                </div>
              </>
            )}
          </div>
        )}

        {/* ── Live / Running now ── */}
        {activeSessions.length > 0 && (
          <div style={{ marginBottom: 20 }}>
            <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 10 }}>
              <div className="animate-ping" style={{ width: 6, height: 6, borderRadius: "50%", background: BLUE, flexShrink: 0, animationDuration: "1.8s" }} />
              <span style={{ fontSize: 11, fontWeight: 700, letterSpacing: "0.06em", textTransform: "uppercase", color: BLUE }}>
                Running now
              </span>
              <span style={{ fontSize: 11, color: TEXT3 }}>{activeSessions.length}</span>
            </div>
            <div style={{
              background: "rgba(96,165,250,0.03)",
              border: "1px solid rgba(96,165,250,0.12)",
              borderRadius: 8, overflow: "hidden",
            }}>
              {activeSessions.map((conv, i) => (
                <LiveRow key={conv.id} conv={conv} index={i} onClick={() => onSelectConversation(conv.id)} />
              ))}
            </div>
          </div>
        )}

        {/* ── Recent sessions ── */}
        <div style={{ marginBottom: 8 }}>
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 10 }}>
            <span style={{ fontSize: 11, fontWeight: 700, letterSpacing: "0.06em", textTransform: "uppercase", color: TEXT3 }}>
              {activeSessions.length > 0 ? "Recent" : "Recent Sessions"}
            </span>
            {!convLoading && conversations && conversations.length > 8 && (
              <span style={{ fontSize: 11, color: TEXT3 }}>{conversations.length} total</span>
            )}
          </div>

          {convLoading ? (
            /* Skeleton list */
            <div style={{ background: CARD, border: `1px solid ${BORDER}`, borderRadius: 8, overflow: "hidden" }}>
              {[1, 2, 3, 4].map(i => (
                <div key={i} style={{ display: "flex", alignItems: "center", gap: 12, padding: "12px 16px", borderTop: i > 1 ? `1px solid ${BORDER}` : "none" }}>
                  <Sk w={6} h={6} r={3} />
                  <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 5 }}>
                    <Sk h={12} w="55%" />
                    <Sk h={10} w="35%" />
                  </div>
                  <Sk w={36} h={10} />
                </div>
              ))}
            </div>
          ) : recentSessions.length === 0 && activeSessions.length === 0 ? (
            /* Empty state */
            <div style={{
              background: CARD, border: `1px solid ${BORDER}`,
              borderRadius: 8, padding: "40px 20px",
              display: "flex", flexDirection: "column", alignItems: "center", gap: 12,
            }}>
              <div style={{ fontSize: 26, opacity: 0.35 }}>🌱</div>
              <div style={{ fontSize: 13, color: TEXT3 }}>No sessions yet</div>
              <button
                onClick={onNewRun}
                style={{
                  display: "inline-flex", alignItems: "center", gap: 7,
                  padding: "8px 20px", borderRadius: 7,
                  border: "1px solid rgba(62,207,142,0.24)",
                  background: "rgba(62,207,142,0.1)", color: ACCENT,
                  fontSize: 12, fontWeight: 600, cursor: "pointer", fontFamily: "inherit",
                  letterSpacing: "0.01em",
                }}
              >
                <Sparkles size={12} /> New Session
              </button>
            </div>
          ) : recentSessions.length > 0 ? (
            <div style={{ background: CARD, border: `1px solid ${BORDER}`, borderRadius: 8, overflow: "hidden" }}>
              {recentSessions.map((conv, i) => (
                <SessionRow key={conv.id} conv={conv} index={i} onClick={() => onSelectConversation(conv.id)} />
              ))}
            </div>
          ) : null}
        </div>

        {/* ── Automations quick-view ── */}
        {automations && automations.length > 0 && (
          <div style={{ marginTop: 20 }}>
            <div style={{ fontSize: 11, fontWeight: 700, letterSpacing: "0.06em", textTransform: "uppercase", color: TEXT3, marginBottom: 10 }}>
              Automations
            </div>
            <div style={{ background: CARD, border: `1px solid ${BORDER}`, borderRadius: 8, overflow: "hidden" }}>
              {automations.slice(0, 5).map((a, i) => (
                <div
                  key={a.id}
                  style={{
                    display: "flex", alignItems: "center", gap: 12,
                    padding: "10px 16px",
                    borderTop: i > 0 ? `1px solid ${BORDER}` : "none",
                  }}
                >
                  <span style={{ color: a.enabled ? ACCENT : TEXT3, flexShrink: 0, display: "flex" }}>
                    <Zap size={12} />
                  </span>
                  <span style={{ flex: 1, fontSize: 12.5, color: a.enabled ? TEXT1 : TEXT3, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {a.name}
                  </span>
                  <span style={{ fontSize: 10, fontWeight: 600, letterSpacing: "0.04em", textTransform: "uppercase", color: a.enabled ? ACCENT : TEXT3 }}>
                    {a.enabled ? "on" : "off"}
                  </span>
                </div>
              ))}
              {automations.length > 5 && (
                <div style={{ padding: "8px 16px", borderTop: `1px solid ${BORDER}` }}>
                  <span style={{ fontSize: 11, color: TEXT3 }}>+{automations.length - 5} more</span>
                </div>
              )}
            </div>
          </div>
        )}

      </div>
    </div>
  );
}
