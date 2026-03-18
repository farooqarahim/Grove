import React, { useEffect, useState } from "react";
import {
  ChevronDown, ChevronRight, X, Copy,
  Shield, ArrowRight, Check, AlertTriangle,
  GitCommit, Layers, Cpu, MessageSquare,
} from "lucide-react";
import type { GraphPhaseRecord, GraphStepRecord } from "@/types";
import { GraphStatusBadge } from "./GraphStatusBadge";
import { GradeIndicator } from "./GradeIndicator";
import { StepTypeBadge } from "./StepTypeBadge";

const MONO = "'JetBrains Mono','Fira Code','SF Mono',monospace";
const SANS = "'DM Sans',-apple-system,BlinkMacSystemFont,sans-serif";

const G = {
  bg:      "#0d0e11",
  surface: "#1c1d22", border: "#2a2c33", subtle: "#222329",
  strip:   "#161719",
  text: "#e2e4e9", muted: "#8b8d98", faint: "#5c5e6a",
  green: "#3ecf8e", greenBg: "rgba(62,207,142,0.08)", greenBdr: "rgba(62,207,142,0.2)",
  amber: "#f59e0b",
  blue: "#60a5fa", blueBg: "rgba(96,165,250,0.08)", blueBdr: "rgba(96,165,250,0.2)",
  red: "#f87171", redBg: "rgba(248,113,113,0.08)", redBdr: "rgba(248,113,113,0.2)",
  coral: "#fb923c", coralBg: "rgba(251,146,60,0.08)", coralBdr: "rgba(251,146,60,0.2)",
};

/* ── Collapsible section ── */
function Section({
  label,
  icon: Icon,
  count,
  defaultOpen = true,
  children,
}: {
  label: string;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  icon?: React.ComponentType<any>;
  count?: number;
  defaultOpen?: boolean;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div>
      <button
        onClick={() => setOpen((v) => !v)}
        style={{
          display: "flex", alignItems: "center", gap: 8, width: "100%",
          padding: "11px 0", background: "none", border: "none",
          borderTop: `1px solid ${G.subtle}`, cursor: "pointer",
          fontFamily: SANS, outline: "none",
        }}
      >
        {open
          ? <ChevronDown size={13} color={G.faint} />
          : <ChevronRight size={13} color={G.faint} />}
        {Icon && <Icon size={13} color={G.faint} strokeWidth={2} />}
        <span style={{
          fontSize: 11, fontWeight: 700, letterSpacing: "0.07em",
          color: G.text, textTransform: "uppercase", flex: 1,
        }}>
          {label}
        </span>
        {count !== undefined && (
          <span style={{
            fontFamily: MONO, fontSize: 10, fontWeight: 600, color: G.faint,
            background: "rgba(255,255,255,0.05)", padding: "1px 7px", borderRadius: 99,
          }}>
            {count}
          </span>
        )}
      </button>
      {open && <div style={{ paddingBottom: 14 }}>{children}</div>}
    </div>
  );
}

/* ── Metrics strip cell ── */
function StatCell({
  label, children, last = false,
}: {
  label: string; children: React.ReactNode; last?: boolean;
}) {
  return (
    <div style={{
      flex: 1, padding: "11px 16px", minWidth: 0,
      borderRight: last ? "none" : `1px solid ${G.subtle}`,
    }}>
      <div style={{
        fontSize: 9, fontWeight: 700, letterSpacing: "0.08em",
        color: G.faint, textTransform: "uppercase", marginBottom: 5,
      }}>
        {label}
      </div>
      <div style={{ display: "flex", alignItems: "center" }}>
        {children}
      </div>
    </div>
  );
}

/* ── Pipeline card (tall variant) ── */
type StageState = "done" | "active" | "failed" | "pending";

function PipelineCard({
  label, state, subtext, isLast,
}: {
  label: string; state: StageState; subtext?: string; isLast: boolean;
}) {
  const color =
    state === "done" ? G.green :
    state === "active" ? G.coral :
    state === "failed" ? G.red :
    G.faint;
  const bg =
    state === "done"   ? G.greenBg :
    state === "active" ? G.coralBg :
    state === "failed" ? G.redBg :
    "rgba(255,255,255,0.02)";
  const bdr =
    state === "done"   ? G.greenBdr :
    state === "active" ? G.coralBdr :
    state === "failed" ? G.redBdr :
    G.subtle;

  return (
    <div style={{ display: "flex", alignItems: "center", flex: 1 }}>
      <div style={{
        flex: 1, padding: "16px 10px", borderRadius: 10,
        background: bg, border: `1px solid ${bdr}`,
        textAlign: "center", minHeight: 84,
        display: "flex", flexDirection: "column",
        alignItems: "center", justifyContent: "center", gap: 7,
      }}>
        <div style={{ display: "flex", justifyContent: "center" }}>
          {state === "done" && <Check size={18} color={color} strokeWidth={2.5} />}
          {state === "active" && (
            <div style={{ animation: "graph-activity-spin 1.2s linear infinite", display: "flex" }}>
              <svg width={18} height={18} viewBox="0 0 14 14" fill="none" stroke={color} strokeWidth={1.8} strokeLinecap="round">
                <path d="M7 1.5v2M7 10.5v2M1.5 7h2M10.5 7h2M3.27 3.27l1.42 1.42M9.31 9.31l1.42 1.42M3.27 10.73l1.42-1.42M9.31 4.69l1.42-1.42" />
              </svg>
            </div>
          )}
          {state === "failed" && <AlertTriangle size={18} color={color} strokeWidth={2} />}
          {state === "pending" && (
            <svg width={18} height={18} viewBox="0 0 14 14" fill="none" stroke={color} strokeWidth={1.4} strokeLinecap="round">
              <circle cx={7} cy={7} r={5.5} />
              <circle cx={7} cy={7} r={1.5} fill={color} />
            </svg>
          )}
        </div>
        <div style={{ fontSize: 11, fontWeight: 700, color, letterSpacing: "0.04em" }}>
          {label}
        </div>
        {subtext && (
          <div style={{ fontSize: 10, color: state === "pending" ? G.faint : `${color}99`, fontFamily: MONO }}>
            {subtext}
          </div>
        )}
      </div>
      {!isLast && (
        <div style={{
          width: 24, flexShrink: 0,
          display: "flex", alignItems: "center", justifyContent: "center",
        }}>
          <ArrowRight size={12} color={state === "done" ? `${G.green}55` : G.subtle} />
        </div>
      )}
    </div>
  );
}

/* ── Helpers ── */
function validationActiveIndex(status: string): number {
  const order = ["pending", "validating", "fixing"];
  return order.indexOf(status);
}

function getValidationStageState(stageIdx: number, vs: string): StageState {
  if (stageIdx === 3) {
    if (vs === "passed") return "done";
    if (vs === "failed") return "failed";
    return "pending";
  }
  if (vs === "passed" || vs === "failed") return "done";
  const active = validationActiveIndex(vs);
  if (active > stageIdx) return "done";
  if (active === stageIdx) return "active";
  return "pending";
}

function pipelineSubtext(stageIdx: number, vs: string): string | undefined {
  const state = getValidationStageState(stageIdx, vs);
  if (state === "done" && stageIdx < 3) return "complete";
  if (state === "active") return "in progress";
  if (state === "failed" && stageIdx === 3) return "validation failed";
  if (state === "done" && stageIdx === 3) return "all checks passed";
  return undefined;
}

function parseDeps(json: string): string[] {
  try {
    const p = JSON.parse(json);
    return Array.isArray(p) ? p.map(String) : [];
  } catch { return []; }
}

function gradeColor(g: number): string {
  return g >= 9 ? G.green : g >= 7 ? G.amber : g >= 4 ? G.coral : G.red;
}

/* ══════════════════════════════════════════════════════════
   MAIN DRAWER
   ══════════════════════════════════════════════════════════ */
interface PhaseValidationDrawerProps {
  phase: GraphPhaseRecord | null;
  steps: GraphStepRecord[];
  open: boolean;
  onClose: () => void;
}

export function PhaseValidationDrawer({
  phase, steps, open, onClose,
}: PhaseValidationDrawerProps) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!open) return;
    const h = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("keydown", h);
    return () => document.removeEventListener("keydown", h);
  }, [open, onClose]);

  const sortedSteps = [...steps].sort((a, b) => a.ordinal - b.ordinal);
  const closedSteps = sortedSteps.filter((s) => s.status === "closed").length;
  const deps = phase ? parseDeps(phase.depends_on_json) : [];
  const vs = phase?.validation_status ?? "pending";
  const resultLabel = vs === "failed" ? "Failed" : "Passed";

  const runIds = phase ? [
    { label: "Created",   id: phase.created_run_id },
    { label: "Executed",  id: phase.executed_run_id },
    { label: "Validator", id: phase.validator_run_id },
    { label: "Judge",     id: phase.judge_run_id },
  ].filter((r) => r.id !== null) : [];

  return (
    <>
      {/* Overlay */}
      <div
        onClick={onClose}
        style={{
          position: "fixed", inset: 0, zIndex: 200,
          background: "rgba(0,0,0,0.55)",
          backdropFilter: "blur(3px)", WebkitBackdropFilter: "blur(3px)",
          opacity: open ? 1 : 0, pointerEvents: open ? "auto" : "none",
          transition: "opacity 0.2s",
        }}
      />

      {/* Drawer */}
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Phase Details"
        style={{
          position: "fixed", top: 0, right: 0, bottom: 0, zIndex: 201,
          width: "clamp(300px, 60vw, 650px)",
          background: G.bg,
          borderLeft: `1px solid ${G.border}`,
          display: "flex", flexDirection: "column",
          transform: open ? "translateX(0)" : "translateX(100%)",
          transition: "transform 0.28s cubic-bezier(0.16,1,0.3,1)",
          fontFamily: SANS,
        }}
      >
        {phase && (
          <>
            {/* ── Header ── */}
            <div style={{
              padding: "18px 24px 16px",
              borderBottom: `1px solid ${G.subtle}`,
              flexShrink: 0,
            }}>
              {/* Breadcrumb */}
              <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 12 }}>
                <button
                  onClick={onClose}
                  style={{
                    display: "flex", alignItems: "center", gap: 4,
                    fontSize: 12, color: G.faint,
                    background: "none", border: "none", cursor: "pointer",
                    padding: "2px 0", fontFamily: SANS,
                  }}
                >
                  <ChevronRight size={14} color={G.faint} style={{ transform: "rotate(180deg)" }} />
                  Back
                </button>
                <span style={{ color: G.subtle, fontSize: 12 }}>·</span>
                <span style={{
                  fontFamily: MONO, fontSize: 11, fontWeight: 600,
                  color: G.muted, letterSpacing: "0.04em",
                }}>
                  P{phase.ordinal}
                </span>
                <div style={{ flex: 1 }} />
                <button
                  onClick={onClose}
                  aria-label="Close"
                  style={{
                    padding: "5px", borderRadius: 6, background: "none",
                    border: "none", color: G.faint, cursor: "pointer",
                  }}
                >
                  <X size={16} strokeWidth={2} />
                </button>
              </div>

              {/* Title + badges */}
              <h2 style={{
                fontSize: 17, fontWeight: 600, color: G.text,
                lineHeight: 1.3, margin: "0 0 10px",
                letterSpacing: "-0.01em",
              }}>
                {phase.task_name}
              </h2>
              <div style={{ display: "flex", alignItems: "center", gap: 7, flexWrap: "wrap" }}>
                <GraphStatusBadge status={phase.status} size="sm" />
                {phase.validation_status !== "pending" && (
                  <GraphStatusBadge status={phase.validation_status} size="sm" />
                )}
                {phase.execution_agent && (
                  <>
                    <span style={{ color: G.subtle, fontSize: 11 }}>·</span>
                    <span style={{
                      display: "flex", alignItems: "center", gap: 4,
                      fontSize: 11, fontFamily: MONO, color: G.faint,
                    }}>
                      <Cpu size={10} strokeWidth={2} />
                      {phase.execution_agent.slice(0, 20)}
                    </span>
                  </>
                )}
              </div>
            </div>

            {/* ── Metrics strip ── */}
            <div style={{
              display: "flex",
              background: G.strip,
              borderBottom: `1px solid ${G.subtle}`,
              flexShrink: 0,
            }}>
              <StatCell label="Phase">
                <span style={{ fontFamily: MONO, fontSize: 13, fontWeight: 600, color: G.text }}>
                  P{phase.ordinal}
                </span>
              </StatCell>
              <StatCell label="Status">
                <GraphStatusBadge status={phase.status} size="sm" />
              </StatCell>
              <StatCell label="Validation">
                <GraphStatusBadge status={phase.validation_status} size="sm" />
              </StatCell>
              <StatCell label="Grade">
                {phase.grade !== null
                  ? <GradeIndicator grade={phase.grade} size="sm" />
                  : <span style={{ fontFamily: MONO, fontSize: 12, color: G.faint }}>—</span>}
              </StatCell>
              <StatCell label="Steps" last>
                <span style={{
                  fontFamily: MONO, fontSize: 13, fontWeight: 600,
                  color: closedSteps === sortedSteps.length && sortedSteps.length > 0 ? G.green : G.text,
                }}>
                  {closedSteps}/{sortedSteps.length}
                </span>
              </StatCell>
            </div>

            {/* ── Scrollable body ── */}
            <div style={{ flex: 1, overflowY: "auto", padding: "0 24px" }}>

              {/* Objective (prominent, outside sections) */}
              {phase.task_objective && (
                <div style={{
                  padding: "16px 0 0",
                  borderBottom: `1px solid ${G.subtle}`,
                  paddingBottom: 16,
                }}>
                  <div style={{
                    fontSize: 10, fontWeight: 700, letterSpacing: "0.08em",
                    color: G.faint, textTransform: "uppercase", marginBottom: 8,
                  }}>
                    Objective
                  </div>
                  <p style={{
                    fontSize: 13.5, color: G.text, lineHeight: 1.7,
                    margin: 0, fontWeight: 400,
                  }}>
                    {phase.task_objective}
                  </p>
                </div>
              )}

              {/* VALIDATION PIPELINE */}
              <Section label="Validation Pipeline" icon={Shield}>
                <div style={{ display: "flex", alignItems: "stretch", gap: 0 }}>
                  {[
                    { label: "Pending",    idx: 0 },
                    { label: "Validating", idx: 1 },
                    { label: "Fixing",     idx: 2 },
                    { label: resultLabel,  idx: 3 },
                  ].map((s, i, arr) => (
                    <PipelineCard
                      key={s.label}
                      label={s.label}
                      state={getValidationStageState(s.idx, vs)}
                      subtext={pipelineSubtext(s.idx, vs)}
                      isLast={i === arr.length - 1}
                    />
                  ))}
                </div>
              </Section>

              {/* STEPS TABLE */}
              {sortedSteps.length > 0 && (
                <Section label="Steps" icon={Layers} count={sortedSteps.length}>
                  {/* Table header */}
                  <div style={{
                    display: "grid",
                    gridTemplateColumns: "38px 64px 1fr 90px 56px",
                    gap: 8, padding: "0 10px 6px",
                    alignItems: "center",
                  }}>
                    {["#", "Type", "Name", "Status", "Grade"].map((h) => (
                      <div key={h} style={{
                        fontSize: 9, fontWeight: 700, letterSpacing: "0.08em",
                        color: G.faint, textTransform: "uppercase",
                      }}>
                        {h}
                      </div>
                    ))}
                  </div>
                  {/* Rows */}
                  <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
                    {sortedSteps.map((step) => {
                      const isActive = step.status === "inprogress";
                      const isDone = step.status === "closed";
                      const ordColor = isDone ? G.green : isActive ? G.coral : G.faint;
                      return (
                        <div
                          key={step.id}
                          style={{
                            display: "grid",
                            gridTemplateColumns: "38px 64px 1fr 90px 56px",
                            gap: 8, alignItems: "center",
                            padding: "9px 10px",
                            borderRadius: 8,
                            background: isActive ? G.coralBg : "rgba(255,255,255,0.02)",
                            border: `1px solid ${isActive ? G.coralBdr : G.subtle}`,
                          }}
                        >
                          <span style={{
                            fontFamily: MONO, fontSize: 11, fontWeight: 600,
                            color: ordColor,
                          }}>
                            S{step.ordinal}
                          </span>
                          <div>
                            <StepTypeBadge stepType={step.step_type} />
                          </div>
                          <span style={{
                            fontSize: 12.5, color: isDone ? G.muted : G.text,
                            overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                          }}>
                            {step.task_name}
                          </span>
                          <div>
                            <GraphStatusBadge status={step.status} size="sm" />
                          </div>
                          <div>
                            {step.grade !== null
                              ? <GradeIndicator grade={step.grade} size="sm" />
                              : <span style={{ fontFamily: MONO, fontSize: 11, color: G.faint }}>—</span>}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </Section>
              )}

              {/* AI COMMENTS + OUTCOME */}
              {(phase.ai_comments || phase.outcome) && (
                <Section label="Analysis" icon={MessageSquare}>
                  <div style={{
                    display: "grid",
                    gridTemplateColumns: phase.ai_comments && phase.outcome ? "1fr 1fr" : "1fr",
                    gap: 12,
                  }}>
                    {phase.ai_comments && (
                      <div style={{
                        padding: "12px 14px",
                        background: G.surface, border: `1px solid ${G.subtle}`, borderRadius: 10,
                      }}>
                        <div style={{
                          fontSize: 10, fontWeight: 700, letterSpacing: "0.07em",
                          color: G.faint, textTransform: "uppercase", marginBottom: 7,
                        }}>
                          AI Comments
                        </div>
                        <p style={{ fontSize: 12.5, color: G.muted, lineHeight: 1.65, margin: 0 }}>
                          {phase.ai_comments}
                        </p>
                      </div>
                    )}
                    {phase.outcome && (
                      <div style={{
                        padding: "12px 14px",
                        background: G.surface, border: `1px solid ${G.subtle}`, borderRadius: 10,
                      }}>
                        <div style={{
                          fontSize: 10, fontWeight: 700, letterSpacing: "0.07em",
                          color: G.faint, textTransform: "uppercase", marginBottom: 7,
                        }}>
                          Outcome
                        </div>
                        <p style={{ fontSize: 12.5, color: G.muted, lineHeight: 1.65, margin: 0 }}>
                          {phase.outcome}
                        </p>
                      </div>
                    )}
                  </div>
                </Section>
              )}

              {/* DEPENDENCIES */}
              {deps.length > 0 && (
                <Section label="Dependencies" icon={GitCommit} count={deps.length} defaultOpen={false}>
                  <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                    {deps.map((dep, i) => (
                      <div key={i} style={{
                        display: "flex", alignItems: "center", gap: 8,
                        padding: "8px 12px", borderRadius: 6,
                        background: G.surface, border: `1px solid ${G.subtle}`,
                      }}>
                        <GitCommit size={12} color={G.faint} strokeWidth={2} />
                        <span style={{ fontFamily: MONO, fontSize: 11, color: G.muted }}>
                          {dep}
                        </span>
                      </div>
                    ))}
                  </div>
                </Section>
              )}

              {/* GIT COMMIT */}
              {phase.git_commit_sha && (
                <Section label="Git Commit" defaultOpen={false}>
                  <div style={{
                    display: "flex", alignItems: "center", gap: 12,
                    padding: "12px 16px",
                    background: G.surface, border: `1px solid ${G.subtle}`, borderRadius: 10,
                  }}>
                    <svg width={14} height={14} viewBox="0 0 16 16" fill="none"
                      stroke={G.faint} strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round">
                      <circle cx={8} cy={8} r={3} />
                      <line x1={1} y1={8} x2={5} y2={8} />
                      <line x1={11} y1={8} x2={15} y2={8} />
                    </svg>
                    <span style={{ fontFamily: MONO, fontSize: 12, color: G.muted }}>
                      {phase.git_commit_sha.slice(0, 12)}
                    </span>
                    <span style={{ fontFamily: MONO, fontSize: 11, color: G.faint }}>
                      …{phase.git_commit_sha.slice(-6)}
                    </span>
                  </div>
                </Section>
              )}

              {/* RUN IDs */}
              {runIds.length > 0 && (
                <Section label="Run IDs" count={runIds.length} defaultOpen={false}>
                  <div style={{
                    display: "grid",
                    gridTemplateColumns: runIds.length > 2 ? "1fr 1fr" : "1fr",
                    gap: 6,
                  }}>
                    {runIds.map(({ label, id }) => (
                      <div key={label} style={{
                        padding: "10px 12px",
                        background: G.surface, border: `1px solid ${G.subtle}`, borderRadius: 8,
                      }}>
                        <div style={{
                          fontSize: 9, fontWeight: 700, color: G.faint,
                          textTransform: "uppercase", letterSpacing: "0.08em", marginBottom: 5,
                        }}>
                          {label}
                        </div>
                        <span style={{ fontFamily: MONO, fontSize: 11, color: G.muted }}>
                          {id!.slice(0, 16)}…
                        </span>
                      </div>
                    ))}
                  </div>
                </Section>
              )}

              <div style={{ height: 12 }} />
            </div>

            {/* ── Footer ── */}
            <div style={{
              borderTop: `1px solid ${G.subtle}`,
              padding: "12px 24px",
              display: "flex", alignItems: "center", gap: 8, flexShrink: 0,
            }}>
              <button
                onClick={() => {
                  void navigator.clipboard.writeText(phase.id);
                  setCopied(true);
                  setTimeout(() => setCopied(false), 1800);
                }}
                style={{
                  display: "flex", alignItems: "center", gap: 5, fontSize: 12,
                  fontWeight: 500, color: copied ? G.green : G.muted,
                  background: "transparent", border: `1px solid ${G.subtle}`,
                  borderRadius: 7, padding: "7px 13px", cursor: "pointer",
                  fontFamily: SANS, transition: "color 0.15s, border-color 0.15s",
                }}
              >
                <Copy size={13} strokeWidth={2} />
                {copied ? "Copied!" : "Copy ID"}
              </button>
              {phase.grade !== null && (
                <>
                  <div style={{ flex: 1 }} />
                  <div style={{
                    display: "flex", alignItems: "center", gap: 8,
                    padding: "6px 14px",
                    background: `${gradeColor(phase.grade)}12`,
                    border: `1px solid ${gradeColor(phase.grade)}30`,
                    borderRadius: 7,
                  }}>
                    <span style={{ fontSize: 11, color: G.faint }}>Grade</span>
                    <span style={{
                      fontFamily: MONO, fontSize: 13, fontWeight: 700,
                      color: gradeColor(phase.grade),
                    }}>
                      {phase.grade}/10
                    </span>
                  </div>
                </>
              )}
            </div>
          </>
        )}
      </div>
    </>
  );
}
