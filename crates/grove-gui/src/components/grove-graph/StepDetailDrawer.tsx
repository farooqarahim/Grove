import React, { useEffect, useState } from "react";
import {
  ChevronDown, ChevronRight, X, RefreshCw, Copy,
  Shield, ArrowRight, Check, AlertTriangle, Play,
  GitCommit, Cpu, MessageSquare,
} from "lucide-react";
import type { GraphStepRecord } from "@/types";
import { rerunStep } from "@/lib/api";
import { StepTypeBadge } from "./StepTypeBadge";
import { GraphStatusBadge } from "./GraphStatusBadge";
import { GradeIndicator } from "./GradeIndicator";

const MONO = "'JetBrains Mono','Fira Code','SF Mono',monospace";
const SANS = "'DM Sans',-apple-system,BlinkMacSystemFont,sans-serif";

const G = {
  bg:      "#0d0e11",
  surface: "#1c1d22", border: "#2a2c33", subtle: "#222329",
  strip:   "#161719",
  text: "#e2e4e9", muted: "#8b8d98", faint: "#5c5e6a",
  green: "#3ecf8e", greenBg: "rgba(62,207,142,0.08)", greenBdr: "rgba(62,207,142,0.2)",
  amber: "#f59e0b", amberBg: "rgba(245,158,11,0.1)", amberBdr: "rgba(245,158,11,0.25)",
  blue: "#60a5fa", blueBg: "rgba(96,165,250,0.08)", blueBdr: "rgba(96,165,250,0.2)",
  red: "#f87171", redBg: "rgba(248,113,113,0.08)", redBdr: "rgba(248,113,113,0.2)",
  coral: "#fb923c", coralBg: "rgba(251,146,60,0.08)", coralBdr: "rgba(251,146,60,0.2)",
  purple: "#a78bfa", purpleBg: "rgba(167,139,250,0.1)", purpleBdr: "rgba(167,139,250,0.2)",
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

/* ── Pipeline stage card (tall variant) ── */
type StageState = "done" | "active" | "failed" | "pending";

function PipelineCard({
  label, state, subtext, isLast,
}: {
  label: string; state: StageState; subtext?: string; isLast: boolean;
}) {
  const color =
    state === "done"   ? G.green :
    state === "active" ? G.coral :
    state === "failed" ? G.red   :
    G.faint;
  const bg =
    state === "done"   ? G.greenBg :
    state === "active" ? G.coralBg :
    state === "failed" ? G.redBg   :
    "rgba(255,255,255,0.02)";
  const bdr =
    state === "done"   ? G.greenBdr :
    state === "active" ? G.coralBdr :
    state === "failed" ? G.redBdr   :
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
function stageIndexFromStep(step: GraphStepRecord): number {
  if (step.status === "failed") return -1;
  if (step.status === "closed") return 3;
  if (step.status === "inprogress") {
    if (step.judge_run_id) return 2;
    if (step.verdict_run_id) return 1;
    return 0;
  }
  return -1;
}

function parseFeedback(json: string): string[] {
  try {
    const p = JSON.parse(json);
    return Array.isArray(p) ? p.map(String) : [];
  } catch { return []; }
}

function parseDeps(json: string): string[] {
  try {
    const p = JSON.parse(json);
    return Array.isArray(p) ? p.map(String) : [];
  } catch { return []; }
}

/* ══════════════════════════════════════════════════════════
   MAIN DRAWER
   ══════════════════════════════════════════════════════════ */
interface StepDetailDrawerProps {
  step: GraphStepRecord | null;
  open: boolean;
  onClose: () => void;
}

export function StepDetailDrawer({ step, open, onClose }: StepDetailDrawerProps) {
  const [rerunning, setRerunning] = useState(false);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!open) return;
    const h = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("keydown", h);
    return () => document.removeEventListener("keydown", h);
  }, [open, onClose]);

  const canRerun = step && (step.status === "closed" || step.status === "failed");

  async function handleRerun() {
    if (!step || rerunning) return;
    setRerunning(true);
    try { await rerunStep(step.id); onClose(); }
    catch { /* surfaced via graph refresh */ }
    finally { setRerunning(false); }
  }

  const feedbackItems = step ? parseFeedback(step.judge_feedback_json) : [];
  const activeStageIdx = step ? stageIndexFromStep(step) : -1;
  const deps = step ? parseDeps(step.depends_on_json) : [];

  const getStageState = (stageIdx: number): StageState => {
    if (!step) return "pending";
    if (step.status === "failed" && activeStageIdx === -1 && stageIdx === 0) return "failed";
    if (step.status === "closed" || activeStageIdx > stageIdx) return "done";
    if (activeStageIdx === stageIdx) return "active";
    return "pending";
  };

  const stageSubtext = (stageIdx: number): string | undefined => {
    const s = getStageState(stageIdx);
    if (s === "done" && stageIdx < 3) return "complete";
    if (s === "active") return "in progress";
    if (s === "failed") return "step failed";
    return undefined;
  };

  const runIds = step ? [
    { label: "Builder", id: step.builder_run_id },
    { label: "Verdict", id: step.verdict_run_id },
    { label: "Judge",   id: step.judge_run_id },
  ].filter((r) => r.id !== null) : [];

  const showIter = step && step.run_iteration > 0;

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

      {/* Drawer panel */}
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Step Details"
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
        {step && (
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
                  S{step.ordinal}
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
                {step.task_name}
              </h2>
              <div style={{ display: "flex", alignItems: "center", gap: 7, flexWrap: "wrap" }}>
                <StepTypeBadge stepType={step.step_type} />
                <GraphStatusBadge status={step.status} size="sm" />
                {step.run_iteration > 0 && (
                  <>
                    <span style={{ color: G.subtle, fontSize: 11 }}>·</span>
                    <span style={{ fontFamily: MONO, fontSize: 11, color: G.faint }}>
                      Run {step.run_iteration}/{step.max_iterations}
                    </span>
                  </>
                )}
                {step.execution_agent && (
                  <>
                    <span style={{ color: G.subtle, fontSize: 11 }}>·</span>
                    <span style={{
                      display: "flex", alignItems: "center", gap: 4,
                      fontSize: 11, fontFamily: MONO, color: G.faint,
                    }}>
                      <Cpu size={10} strokeWidth={2} />
                      {step.execution_agent.slice(0, 20)}
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
              <StatCell label="Run">
                <span style={{ fontFamily: MONO, fontSize: 13, fontWeight: 600, color: G.text }}>
                  {step.run_iteration}/{step.max_iterations}
                </span>
              </StatCell>
              <StatCell label="Status">
                <GraphStatusBadge status={step.status} size="sm" />
              </StatCell>
              <StatCell label="Grade">
                {step.grade !== null
                  ? <GradeIndicator grade={step.grade} size="sm" />
                  : <span style={{ fontFamily: MONO, fontSize: 12, color: G.faint }}>—</span>}
              </StatCell>
              <StatCell label="Type">
                <StepTypeBadge stepType={step.step_type} />
              </StatCell>
              <StatCell label="Deps" last>
                <span style={{
                  fontFamily: MONO, fontSize: 13, fontWeight: 600,
                  color: deps.length > 0 ? G.text : G.faint,
                }}>
                  {deps.length === 0 ? "—" : deps.length}
                </span>
              </StatCell>
            </div>

            {/* ── Scrollable body ── */}
            <div style={{ flex: 1, overflowY: "auto", padding: "0 24px" }}>

              {/* Objective (prominent, outside sections) */}
              {step.task_objective && (
                <div style={{
                  padding: "16px 0",
                  borderBottom: `1px solid ${G.subtle}`,
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
                    {step.task_objective}
                  </p>
                </div>
              )}

              {/* PIPELINE */}
              <Section label="Pipeline" icon={Play}>
                <div style={{ display: "flex", alignItems: "stretch", gap: 0 }}>
                  {[
                    { label: "Build",   idx: 0 },
                    { label: "Verdict", idx: 1 },
                    { label: "Judge",   idx: 2 },
                  ].map((s, i, arr) => (
                    <PipelineCard
                      key={s.label}
                      label={s.label}
                      state={getStageState(s.idx)}
                      subtext={stageSubtext(s.idx)}
                      isLast={i === arr.length - 1}
                    />
                  ))}
                </div>
              </Section>

              {/* JUDGE FEEDBACK */}
              {feedbackItems.length > 0 && (
                <Section label="Judge Feedback" icon={Shield} count={feedbackItems.length}>
                  {/* Score row */}
                  <div style={{
                    display: "flex", alignItems: "center", gap: 14,
                    padding: "12px 16px",
                    background: G.surface, border: `1px solid ${G.subtle}`,
                    borderRadius: 10, marginBottom: 10,
                  }}>
                    <Shield size={15} color={G.faint} strokeWidth={1.8} />
                    <span style={{ fontSize: 12, color: G.muted }}>Score</span>
                    <span style={{ fontFamily: MONO, fontSize: 15, fontWeight: 700, color: G.text }}>
                      {step.grade !== null ? `${step.grade}/10` : "—"}
                    </span>
                    <div style={{ width: 1, height: 14, background: G.subtle }} />
                    <GraphStatusBadge status={step.status} size="sm" />
                  </div>

                  <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                    {feedbackItems.map((item, i) => (
                      <div
                        key={i}
                        style={{
                          display: "flex", gap: 12, padding: "12px 14px",
                          background: G.blueBg, border: `1px solid ${G.blueBdr}`,
                          borderRadius: 9,
                        }}
                      >
                        <div style={{ flexShrink: 0, paddingTop: 1 }}>
                          <span style={{
                            fontFamily: MONO, fontSize: 10, fontWeight: 700,
                            color: G.blue, background: `${G.blue}20`,
                            padding: "2px 6px", borderRadius: 3,
                          }}>
                            {i + 1}
                          </span>
                        </div>
                        <p style={{ fontSize: 13, color: G.text, lineHeight: 1.65, flex: 1, margin: 0 }}>
                          {item}
                        </p>
                      </div>
                    ))}
                  </div>
                </Section>
              )}

              {/* AI COMMENTS + OUTCOME */}
              {(step.ai_comments || step.outcome) && (
                <Section label="Analysis" icon={MessageSquare}>
                  <div style={{
                    display: "grid",
                    gridTemplateColumns: step.ai_comments && step.outcome ? "1fr 1fr" : "1fr",
                    gap: 12,
                  }}>
                    {step.ai_comments && (
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
                          {step.ai_comments}
                        </p>
                      </div>
                    )}
                    {step.outcome && (
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
                          {step.outcome}
                        </p>
                      </div>
                    )}
                  </div>
                </Section>
              )}

              {/* ITERATIONS */}
              {showIter && (
                <Section label="Iterations" count={step.run_iteration}>
                  <div style={{
                    display: "grid",
                    gridTemplateColumns: `repeat(${Math.min(step.max_iterations, 4)}, 1fr)`,
                    gap: 6,
                  }}>
                    {Array.from({ length: step.max_iterations }, (_, i) => {
                      const iterNum = i + 1;
                      const isCurrent = iterNum === step.run_iteration;
                      const isPast = iterNum < step.run_iteration;
                      const isFuture = iterNum > step.run_iteration;
                      const c = isCurrent ? G.coral : isPast ? G.green : G.faint;
                      return (
                        <div
                          key={i}
                          style={{
                            padding: "10px 12px", borderRadius: 8, textAlign: "center",
                            background: isCurrent ? G.coralBg : isPast ? G.greenBg : "rgba(255,255,255,0.02)",
                            border: `1px solid ${isCurrent ? G.coralBdr : isPast ? G.greenBdr : G.subtle}`,
                          }}
                        >
                          <div style={{
                            fontFamily: MONO, fontSize: 11, fontWeight: 700,
                            color: c, marginBottom: 4,
                          }}>
                            #{iterNum}
                          </div>
                          <div style={{ fontSize: 10, color: isFuture ? G.faint : G.muted }}>
                            {isCurrent
                              ? step.status === "inprogress" ? "active" : step.status
                              : isPast ? "done"
                              : "pending"}
                          </div>
                          {isCurrent && step.status === "inprogress" && (
                            <div style={{
                              marginTop: 6, display: "flex", justifyContent: "center",
                              animation: "graph-activity-spin 1.2s linear infinite",
                            }}>
                              <svg width={11} height={11} viewBox="0 0 14 14" fill="none" stroke={G.coral} strokeWidth={2} strokeLinecap="round">
                                <path d="M7 1.5v2M7 10.5v2M1.5 7h2M10.5 7h2M3.27 3.27l1.42 1.42M9.31 9.31l1.42 1.42M3.27 10.73l1.42-1.42M9.31 4.69l1.42-1.42" />
                              </svg>
                            </div>
                          )}
                        </div>
                      );
                    })}
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

              {/* RUN IDs */}
              {runIds.length > 0 && (
                <Section label="Run IDs" count={runIds.length} defaultOpen={false}>
                  <div style={{
                    display: "grid",
                    gridTemplateColumns: runIds.length > 1 ? "1fr 1fr" : "1fr",
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
                  void navigator.clipboard.writeText(step.id);
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

              <div style={{ flex: 1 }} />

              {canRerun && (
                <button
                  onClick={() => void handleRerun()}
                  disabled={rerunning}
                  style={{
                    display: "flex", alignItems: "center", gap: 6, fontSize: 12,
                    fontWeight: 600, color: G.amber, background: G.amberBg,
                    border: `1px solid ${G.amberBdr}`, borderRadius: 7,
                    padding: "7px 16px", cursor: rerunning ? "default" : "pointer",
                    fontFamily: SANS, opacity: rerunning ? 0.6 : 1,
                    transition: "opacity 0.15s",
                  }}
                >
                  <RefreshCw size={12} strokeWidth={2.5} />
                  {rerunning ? "Re-running…" : "Re-run step"}
                </button>
              )}
            </div>
          </>
        )}
      </div>
    </>
  );
}
