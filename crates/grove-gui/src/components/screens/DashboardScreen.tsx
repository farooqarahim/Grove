import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { NewIssueModal } from "@/components/modals/NewIssueModal";
import { GroveLogo } from "@/components/ui/GroveLogo";
import { getWorkspace, listProjects } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import type { NavScreen, ProjectRow } from "@/types";

interface DashboardScreenProps {
  onNavigate: (screen: NavScreen) => void;
  onNewRun: () => void;
  onCreateProject: () => void;
  selectedProjectId?: string | null;
  onSelectConversation?: (id: string) => void;
  onSelectProject?: (id: string) => void;
}

function projectDisplayName(p: ProjectRow): string {
  return p.name || p.root_path.split("/").pop() || p.id;
}

function shortenPath(p: string): string {
  return p.replace(/^\/Users\/[^/]+/, "~");
}

export function DashboardScreen({ onNewRun, onCreateProject, selectedProjectId, onNavigate, onSelectProject }: DashboardScreenProps) {
  const [showCreateIssue, setShowCreateIssue] = useState(false);

  const { data: workspace } = useQuery({
    queryKey: qk.workspace(),
    queryFn: getWorkspace,
    staleTime: 30000,
    refetchInterval: 60000,
  });
  const { data: projects } = useQuery({
    queryKey: qk.projects(),
    queryFn: listProjects,
    staleTime: 30000,
    refetchInterval: 60000,
  });

  const activeProjects = (projects ?? []).filter((p) => p.state === "active");
  const activeProject = selectedProjectId
    ? activeProjects.find((p) => p.id === selectedProjectId) ?? activeProjects[0]
    : activeProjects[0];

  return (
    <div style={s.root}>
      <style>{css}</style>

      {/* ── Logo mark ─────────────────────────────────────── */}
      <div style={s.logoWrap}>
        <div style={s.logoGlow} />
        <GroveLogo size={44} color="#31B97B" />
      </div>

      {/* ── Product + workspace name ──────────────────────── */}
      <div style={s.productLabel}>Grove</div>
      <div style={s.workspaceName}>
        {workspace?.name ?? "Workspace"}
      </div>
      {activeProjects.length > 0 && (
        <div style={s.workspaceSub}>
          {activeProjects.length} active project{activeProjects.length !== 1 ? "s" : ""}
        </div>
      )}

      {/* ── Projects panel ────────────────────────────────── */}
      <div style={s.panel}>
        <div style={s.panelHeader}>Projects</div>

        {activeProjects.length === 0 ? (
          <div style={s.empty}>No active projects yet.</div>
        ) : (
          activeProjects.map((p, i) => {
            const isActive = p.id === activeProject?.id;
            return (
              <div key={p.id}>
                {i > 0 && <div style={s.divider} />}
                <button
                  className={isActive ? "proj-row proj-row--active" : "proj-row"}
                  onClick={() => { onSelectProject?.(p.id); onNavigate("sessions"); }}
                  style={{
                    ...s.projectRow,
                    ...(isActive ? s.projectRowActive : {}),
                    width: "100%", textAlign: "left", background: isActive ? "rgba(49,185,123,0.045)" : "transparent",
                    border: "none", cursor: "pointer", fontFamily: "inherit",
                  }}
                >
                  {isActive && <div style={s.activeBar} />}
                  <div style={s.projectMeta}>
                    <span style={{ ...s.projectName, ...(isActive ? s.projectNameActive : {}) }}>
                      {projectDisplayName(p)}
                    </span>
                    {p.source_kind === "ssh" && (
                      <span style={s.badge}>ssh</span>
                    )}
                  </div>
                  <span style={s.projectPath}>{shortenPath(p.root_path)}</span>
                </button>
              </div>
            );
          })
        )}
      </div>

      {/* ── Action buttons ────────────────────────────────── */}
      <div style={s.actions}>
        <button
          className="action-btn action-btn--primary"
          style={{ ...s.btn, ...s.btnPrimary }}
          onClick={onNewRun}
        >
          <PlusIcon />
          New Session
        </button>
        <button
          className="action-btn"
          style={s.btn}
          onClick={onCreateProject}
        >
          <PlusIcon />
          New Project
        </button>
        <button
          className="action-btn"
          style={s.btn}
          onClick={() => setShowCreateIssue(true)}
        >
          <PlusIcon />
          New Issue
        </button>
      </div>

      <NewIssueModal
        open={showCreateIssue}
        projectId={activeProject?.id ?? null}
        projects={activeProjects}
        onClose={() => setShowCreateIssue(false)}
        onCreated={() => setShowCreateIssue(false)}
      />
    </div>
  );
}

/* ── Inline icon ────────────────────────────────────────── */
function PlusIcon() {
  return (
    <svg width={12} height={12} viewBox="0 0 24 24" fill="none"
      stroke="currentColor" strokeWidth={2.2} strokeLinecap="round">
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}

/* ── Styles ─────────────────────────────────────────────── */
const BG = "#0D0F14";
const PANEL_BG = "rgba(18,21,28,0.92)";
const BORDER = "rgba(38,42,52,0.70)";
const TEXT1 = "#DDE0E7";
const TEXT3 = "#71767F";
const TEXT4 = "#3E434D";
const GREEN = "#31B97B";

const s: Record<string, React.CSSProperties> = {
  root: {
    flex: 1,
    overflowY: "auto",
    background: BG,
    display: "flex",
    flexDirection: "column",
    alignItems: "center",
    padding: "56px 28px 48px",
    fontFamily: "'Geist', 'DM Sans', -apple-system, sans-serif",
  },

  logoWrap: {
    position: "relative",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    marginBottom: 20,
  },
  logoGlow: {
    position: "absolute",
    width: 120,
    height: 120,
    borderRadius: "50%",
    background: "radial-gradient(circle, rgba(49,185,123,0.13) 0%, transparent 70%)",
    pointerEvents: "none",
  },

  productLabel: {
    fontSize: 10,
    fontWeight: 700,
    letterSpacing: "0.12em",
    textTransform: "uppercase" as const,
    color: TEXT4,
    marginBottom: 6,
  },
  workspaceName: {
    fontSize: 22,
    fontWeight: 600,
    letterSpacing: "-0.025em",
    color: TEXT1,
    marginBottom: 6,
  },
  workspaceSub: {
    fontSize: 11.5,
    color: TEXT4,
    letterSpacing: "0.02em",
    marginBottom: 36,
  },

  panel: {
    width: "100%",
    maxWidth: 520,
    background: PANEL_BG,
    border: `1px solid ${BORDER}`,
    borderRadius: 6,
    overflow: "hidden",
    marginBottom: 16,
  },
  panelHeader: {
    fontSize: 10,
    fontWeight: 700,
    letterSpacing: "0.10em",
    textTransform: "uppercase" as const,
    color: TEXT4,
    padding: "14px 18px 10px",
    borderBottom: `1px solid ${BORDER}`,
  },
  divider: {
    height: 1,
    background: BORDER,
    margin: "0 18px",
  },

  projectRow: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    padding: "13px 18px",
    position: "relative",
    gap: 12,
  },
  projectRowActive: {
    background: "rgba(49,185,123,0.045)",
  },
  activeBar: {
    position: "absolute",
    left: 0,
    top: "50%",
    transform: "translateY(-50%)",
    width: 2,
    height: 20,
    background: GREEN,
    borderRadius: "0 2px 2px 0",
  },
  projectMeta: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    minWidth: 0,
  },
  projectName: {
    fontSize: 13,
    fontWeight: 500,
    color: TEXT3,
    whiteSpace: "nowrap" as const,
    overflow: "hidden",
    textOverflow: "ellipsis",
  },
  projectNameActive: {
    color: TEXT1,
    fontWeight: 600,
  },
  badge: {
    fontSize: 9.5,
    fontWeight: 600,
    letterSpacing: "0.06em",
    textTransform: "uppercase" as const,
    color: TEXT4,
    background: "rgba(38,42,52,0.6)",
    border: `1px solid ${BORDER}`,
    borderRadius: 3,
    padding: "1px 5px",
    flexShrink: 0,
  },
  projectPath: {
    fontSize: 11,
    color: TEXT4,
    fontFamily: "'JetBrains Mono', 'SF Mono', 'Menlo', monospace",
    whiteSpace: "nowrap" as const,
    overflow: "hidden",
    textOverflow: "ellipsis",
    maxWidth: 220,
    flexShrink: 0,
  },

  empty: {
    padding: "20px 18px",
    fontSize: 12,
    color: TEXT4,
  },

  actions: {
    width: "100%",
    maxWidth: 520,
    display: "grid",
    gridTemplateColumns: "1fr 1fr 1fr",
    gap: 8,
  },
  btn: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    gap: 7,
    padding: "11px 0",
    borderRadius: 5,
    fontSize: 12,
    fontWeight: 600,
    letterSpacing: "0.01em",
    border: `1px solid ${BORDER}`,
    background: "rgba(24,27,35,0.80)",
    color: TEXT3,
    cursor: "pointer",
    fontFamily: "inherit",
    transition: "all 0.15s ease",
  },
  btnPrimary: {
    background: "rgba(49,185,123,0.10)",
    border: "1px solid rgba(49,185,123,0.28)",
    color: GREEN,
  },
};

const css = `
  .proj-row { transition: background 0.12s ease; }
  .proj-row:hover { background: rgba(38,42,52,0.42) !important; }
  .proj-row--active:hover { background: rgba(49,185,123,0.07) !important; }

  .action-btn { transition: all 0.14s ease; }
  .action-btn:hover {
    background: rgba(38,42,52,0.70) !important;
    border-color: rgba(62,67,77,0.8) !important;
    color: #DDE0E7 !important;
  }
  .action-btn--primary:hover {
    background: rgba(49,185,123,0.16) !important;
    border-color: rgba(49,185,123,0.45) !important;
    color: #31B97B !important;
  }
`;
