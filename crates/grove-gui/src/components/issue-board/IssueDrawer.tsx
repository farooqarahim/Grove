import { useEffect, useState } from "react";
import type { Issue, IssueComment, IssueEvent, IssueTrackerStatus, RunRecord } from "@/types";
import {
  getProjectSettings,
  getRun,
  issueCommentAdd, issueDelete, issueListActivity, issueListComments,
  issueMove, issueReopen, issueUpdate,
  listProviderStatuses, pushIssueToProvider,
} from "@/lib/api";
import { COLUMN_CONFIGS, LABEL_COLORS, PRIORITY_CONFIG } from "./constants";
import { compositeId, displayProvider, formatRelative, metadataPretty, normalizePriority } from "./helpers";
import { CloseIcon } from "./Icons";

type DetailTab = "comments" | "activity";

const PRIORITY_OPTIONS = ["Critical", "High", "Medium", "Low", "None"] as const;

interface IssueDrawerProps {
  issue: Issue | null;
  open: boolean;
  projectId: string | null;
  onClose: () => void;
  onDeleted: () => void;
  onReopen: () => void;
  onUpdated: (updated: Issue) => void;
}

export function IssueDrawer({
  issue, open, projectId, onClose, onDeleted, onReopen, onUpdated,
}: IssueDrawerProps) {
  const [tab, setTab] = useState<DetailTab>("comments");
  const [comment, setComment] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [comments, setComments] = useState<IssueComment[]>([]);
  const [activity, setActivity] = useState<IssueEvent[]>([]);
  const [loaded, setLoaded] = useState(false);

  const [editingField, setEditingField] = useState<string | null>(null);
  const [draft, setDraft] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);

  const [statuses, setStatuses] = useState<IssueTrackerStatus[]>([]);
  const [statusesLoaded, setStatusesLoaded] = useState(false);

  const [linkedRun, setLinkedRun] = useState<RunRecord | null>(null);

  const [pushing, setPushing] = useState(false);
  const [pushError, setPushError] = useState<string | null>(null);

  const [labelInput, setLabelInput] = useState("");
  const [configuredProviders, setConfiguredProviders] = useState<string[]>([]);

  const cid = issue ? compositeId(issue) : null;

  // Load which providers the project has configured (have a project_key set).
  useEffect(() => {
    if (!projectId) { setConfiguredProviders([]); return; }
    getProjectSettings(projectId).then((s) => {
      const providers: string[] = [];
      if (s.github_project_key) providers.push("github");
      if (s.jira_project_key) providers.push("jira");
      if (s.linear_project_key) providers.push("linear");
      setConfiguredProviders(providers);
    }).catch(() => setConfiguredProviders([]));
  }, [projectId]);

  useEffect(() => {
    if (!open || !cid) return;
    setLoaded(false); setComments([]); setActivity([]);
    setEditingField(null); setDraft({});
    setStatuses([]); setStatusesLoaded(false);
    setLinkedRun(null); setPushError(null);

    const load = async () => {
      try {
        const [cs, evts] = await Promise.all([
          issueListComments(cid),
          issueListActivity(cid),
        ]);
        setComments(cs);
        setActivity(evts);
      } finally {
        setLoaded(true);
      }
    };
    void load();

    if (issue?.run_id) {
      getRun(issue.run_id).then(run => { if (run) setLinkedRun(run); }).catch(() => {});
    }
  }, [cid, open]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") setEditingField(null);
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open]);

  const startEdit = (field: string, current: string) => {
    setEditingField(field);
    setDraft(prev => ({ ...prev, [field]: current }));
  };

  const cancelEdit = () => setEditingField(null);

  const saveField = async (field: string, value: string) => {
    if (!cid || !issue) return;
    setSaving(true);
    try {
      if (field === "status") {
        await issueMove(cid, value);
        onUpdated({ ...issue, status: value, canonical_status: value });
      } else if (field === "title") {
        await issueUpdate(cid, { title: value });
        onUpdated({ ...issue, title: value });
      } else if (field === "body") {
        await issueUpdate(cid, { body: value });
        onUpdated({ ...issue, body: value });
      } else if (field === "priority") {
        await issueUpdate(cid, { priority: value.toLowerCase() });
        onUpdated({ ...issue, priority: value.toLowerCase() });
      } else if (field === "assignee") {
        await issueUpdate(cid, { assignee: value });
        onUpdated({ ...issue, assignee: value || null });
      } else if (field === "labels") {
        const labels = value.split(",").map((l: string) => l.trim()).filter(Boolean);
        await issueUpdate(cid, { labels });
        onUpdated({ ...issue, labels });
      }
    } catch {
      // non-fatal — field reverts visually on next render
    } finally {
      setSaving(false);
      setEditingField(null);
    }
  };

  const handleAddComment = async () => {
    if (!comment.trim() || submitting || !cid) return;
    setSubmitting(true);
    try {
      const c = await issueCommentAdd(cid, comment.trim(), null, false);
      setComments(prev => [...prev, c]);
      setComment("");
    } catch { /* non-fatal */ } finally { setSubmitting(false); }
  };

  const handleDelete = async () => {
    if (!cid || !issue || !window.confirm(`Delete issue "${issue.title}"?`)) return;
    try { await issueDelete(cid); onDeleted(); }
    catch (e) { window.alert(e instanceof Error ? e.message : String(e)); }
  };

  const handleReopen = async () => {
    if (!cid) return;
    try { await issueReopen(cid, false); onReopen(); } catch { /* non-fatal */ }
  };

  const loadStatuses = async () => {
    if (statusesLoaded || !issue) return;
    try {
      const s = await listProviderStatuses(issue.provider, projectId ?? undefined);
      setStatuses(s);
    } catch { /* leave empty */ }
    setStatusesLoaded(true);
  };

  const handlePush = async (targetProvider: string) => {
    if (!cid || !issue || pushing) return;
    setPushing(true);
    setPushError(null);
    try {
      const updated = await pushIssueToProvider(cid, targetProvider, "", projectId);
      onUpdated(updated);
    } catch (e) {
      setPushError(e instanceof Error ? e.message : String(e));
    } finally {
      setPushing(false);
    }
  };

  if (!open || !issue) return null;

  // suppress unused-variable lint for saving — it is wired to setSaving in saveField
  void saving;

  const displayPriority = normalizePriority(issue.priority);
  const pc = PRIORITY_CONFIG[displayPriority] ?? PRIORITY_CONFIG.None;
  const cs = issue.canonical_status ?? "open";
  const cfg = COLUMN_CONFIGS[cs] ?? { label: cs, dot: "#6b7280" };
  const isDone = cs === "done" || cs === "cancelled" || issue.status === "closed";
  const providerMetadata = metadataPretty(issue.provider_metadata);
  const scopeValue = issue.provider_scope_key
    ? `${issue.provider_scope_type ?? "scope"} · ${issue.provider_scope_name ?? issue.provider_scope_key}`
    : null;

  const labelStyle: React.CSSProperties = {
    fontSize: 11, fontWeight: 600, color: "#475569", letterSpacing: "0.05em",
    display: "flex", alignItems: "center",
  };
  const inputStyle: React.CSSProperties = {
    background: "rgba(99,102,241,0.08)", border: "1px solid rgba(99,102,241,0.35)",
    borderRadius: 7, padding: "6px 10px", color: "#e2e8f0", fontSize: 13,
    fontFamily: "inherit", outline: "none", width: "100%", boxSizing: "border-box" as const,
  };
  const hoverFieldStyle: React.CSSProperties = {
    cursor: "pointer", borderRadius: 6, padding: "2px 6px", margin: "-2px -6px",
  };

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed", inset: 0, zIndex: 1000,
        background: "rgba(0,0,0,0.45)", backdropFilter: "blur(4px)",
        display: "flex", justifyContent: "flex-end",
        animation: "fadeIn .15s ease",
      }}
    >
      <div
        onClick={e => e.stopPropagation()}
        style={{
          width: "100%", maxWidth: 500, height: "100%",
          background: "#0c1222", borderLeft: "1px solid rgba(51,65,85,0.3)",
          boxShadow: "-20px 0 60px rgba(0,0,0,0.3)",
          animation: "slideIn .25s ease", overflowY: "auto",
          display: "flex", flexDirection: "column",
        }}
      >
        {/* Header */}
        <div style={{
          display: "flex", alignItems: "center", justifyContent: "space-between",
          padding: "16px 24px", borderBottom: "1px solid rgba(51,65,85,0.2)", flexShrink: 0,
        }}>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <span style={{ fontSize: 12, fontWeight: 700, color: "#475569", fontFamily: "monospace" }}>
              {issue.external_id || compositeId(issue)}
            </span>
            {editingField === "priority" ? (
              <select
                autoFocus
                value={draft.priority ?? displayPriority}
                onChange={e => { void saveField("priority", e.target.value); }}
                onBlur={cancelEdit}
                style={{ ...inputStyle, width: "auto", fontSize: 11, padding: "3px 8px" }}
              >
                {PRIORITY_OPTIONS.map(p => <option key={p} value={p}>{p}</option>)}
              </select>
            ) : (
              <span
                onClick={() => startEdit("priority", displayPriority)}
                title="Click to change priority"
                style={{
                  fontSize: 10, fontWeight: 700, padding: "2px 8px", borderRadius: 5,
                  background: pc.bg, color: pc.color, border: `1px solid ${pc.border}`,
                  cursor: "pointer",
                }}
              >{displayPriority.toUpperCase()}</span>
            )}
            <span style={{
              fontSize: 10, padding: "2px 8px", borderRadius: 5,
              background: "rgba(51,65,85,0.2)", color: "#94a3b8", fontWeight: 500,
            }}>{displayProvider(issue.provider)}</span>
          </div>
          <button
            onClick={onClose}
            className="ib-close-btn"
            style={{
              background: "rgba(51,65,85,0.2)", border: "1px solid rgba(51,65,85,0.2)",
              borderRadius: 8, width: 30, height: 30, display: "flex", alignItems: "center",
              justifyContent: "center", cursor: "pointer", color: "#64748b",
            }}
          >
            <CloseIcon />
          </button>
        </div>

        <div style={{ padding: 24, flex: 1, display: "flex", flexDirection: "column" }}>

          {/* Title */}
          {editingField === "title" ? (
            <input
              autoFocus
              value={draft.title ?? issue.title}
              onChange={e => setDraft(prev => ({ ...prev, title: e.target.value }))}
              onBlur={() => { void saveField("title", draft.title ?? issue.title); }}
              onKeyDown={e => {
                if (e.key === "Enter") void saveField("title", draft.title ?? issue.title);
                if (e.key === "Escape") cancelEdit();
              }}
              style={{ ...inputStyle, fontSize: 18, fontWeight: 700, marginBottom: 20 }}
            />
          ) : (
            <h2
              onClick={() => startEdit("title", issue.title)}
              title="Click to edit"
              style={{
                fontSize: 18, fontWeight: 700, color: "#f1f5f9", lineHeight: 1.4,
                margin: "0 0 20px -4px", letterSpacing: "-0.02em",
                cursor: "pointer", borderRadius: 8, padding: "2px 4px",
              }}
            >{issue.title}</h2>
          )}

          {/* Metadata grid */}
          <div style={{ display: "grid", gridTemplateColumns: "100px 1fr", gap: "14px 16px", alignItems: "start" }}>

            {/* STATUS */}
            <span style={labelStyle}>STATUS</span>
            <div style={{ position: "relative" }}>
              {editingField === "status" && (
                <div style={{
                  position: "absolute", top: -4, left: -4, zIndex: 100,
                  background: "#0f172a", border: "1px solid rgba(99,102,241,0.4)",
                  borderRadius: 10, overflow: "hidden", minWidth: 210,
                  boxShadow: "0 8px 32px rgba(0,0,0,0.4)",
                }}>
                  {!statusesLoaded ? (
                    <div style={{ padding: "10px 14px", fontSize: 12, color: "#64748b" }}>Loading…</div>
                  ) : statuses.length === 0 ? (
                    <div style={{ padding: "10px 14px", fontSize: 12, color: "#64748b" }}>No statuses found.</div>
                  ) : statuses.map(s => (
                    <button
                      key={s.id}
                      onClick={() => { void saveField("status", s.id); }}
                      style={{
                        display: "flex", alignItems: "center", gap: 8, width: "100%",
                        padding: "9px 14px", background: "transparent",
                        border: "none", cursor: "pointer", fontFamily: "inherit",
                        color: "#cbd5e1", fontSize: 13, textAlign: "left",
                      }}
                      onMouseEnter={e => { (e.currentTarget as HTMLButtonElement).style.background = "rgba(99,102,241,0.1)"; }}
                      onMouseLeave={e => { (e.currentTarget as HTMLButtonElement).style.background = "transparent"; }}
                    >
                      <div style={{ width: 8, height: 8, borderRadius: "50%", background: s.color ? `#${s.color}` : "#475569", flexShrink: 0 }} />
                      <span>{s.name}</span>
                      <span style={{ fontSize: 10, color: "#475569", marginLeft: "auto" }}>{s.category}</span>
                    </button>
                  ))}
                  <button
                    onClick={cancelEdit}
                    style={{
                      width: "100%", padding: "8px 14px", background: "rgba(239,68,68,0.05)",
                      border: "none", borderTop: "1px solid rgba(51,65,85,0.2)",
                      color: "#64748b", fontSize: 12, cursor: "pointer", fontFamily: "inherit",
                    }}
                  >Cancel</button>
                </div>
              )}
              <div
                onClick={() => { startEdit("status", issue.status); void loadStatuses(); }}
                title="Click to change status"
                style={{ display: "flex", alignItems: "center", gap: 6, cursor: "pointer", ...hoverFieldStyle }}
              >
                <div style={{ width: 7, height: 7, borderRadius: "50%", background: cfg.dot, boxShadow: `0 0 6px ${cfg.dot}66` }} />
                <span style={{ fontSize: 13, color: "#cbd5e1", fontWeight: 500 }}>{issue.status || cfg.label}</span>
                <span style={{ fontSize: 10, color: "#475569" }}>▾</span>
              </div>
            </div>

            {/* PRIORITY */}
            <span style={labelStyle}>PRIORITY</span>
            <div
              onClick={() => startEdit("priority", displayPriority)}
              title="Click to change priority"
              style={{ display: "flex", alignItems: "center", gap: 6, cursor: "pointer", ...hoverFieldStyle }}
            >
              <span style={{ color: pc.color, fontWeight: 700, fontSize: 12 }}>{pc.icon}</span>
              <span style={{ fontSize: 13, color: "#cbd5e1" }}>{displayPriority}</span>
              <span style={{ fontSize: 10, color: "#475569" }}>▾</span>
            </div>

            {/* ASSIGNEE */}
            <span style={labelStyle}>ASSIGNEE</span>
            {editingField === "assignee" ? (
              <input
                autoFocus
                value={draft.assignee ?? (issue.assignee ?? "")}
                onChange={e => setDraft(prev => ({ ...prev, assignee: e.target.value }))}
                onBlur={() => { void saveField("assignee", draft.assignee ?? ""); }}
                onKeyDown={e => {
                  if (e.key === "Enter") void saveField("assignee", draft.assignee ?? "");
                  if (e.key === "Escape") cancelEdit();
                }}
                placeholder="@username"
                style={inputStyle}
              />
            ) : (
              <div onClick={() => startEdit("assignee", issue.assignee ?? "")} title="Click to assign" style={{ cursor: "pointer", ...hoverFieldStyle }}>
                {issue.assignee ? (
                  <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <div style={{
                      width: 22, height: 22, borderRadius: 6, fontSize: 10, fontWeight: 700,
                      background: "rgba(99,102,241,0.15)", color: "#818cf8",
                      display: "flex", alignItems: "center", justifyContent: "center",
                    }}>{issue.assignee.slice(0, 2).toUpperCase()}</div>
                    <span style={{ fontSize: 13, color: "#cbd5e1" }}>{issue.assignee}</span>
                  </div>
                ) : (
                  <span style={{ fontSize: 13, color: "#334155", fontStyle: "italic" }}>Unassigned — click to assign</span>
                )}
              </div>
            )}

            {/* LABELS */}
            <span style={labelStyle}>LABELS</span>
            <div style={{ display: "flex", gap: 5, flexWrap: "wrap", alignItems: "center" }}>
              {issue.labels.map(l => {
                const lc = LABEL_COLORS[l] ?? { color: "#94a3b8", bg: "rgba(148,163,184,0.08)" };
                return (
                  <span key={l} style={{
                    fontSize: 11, padding: "2px 8px", borderRadius: 5,
                    background: lc.bg, color: lc.color, fontWeight: 500,
                    display: "flex", alignItems: "center", gap: 4,
                  }}>
                    {l}
                    <button
                      onClick={() => {
                        const labels = issue.labels.filter(x => x !== l).join(",");
                        void saveField("labels", labels);
                      }}
                      style={{ background: "none", border: "none", color: "inherit", cursor: "pointer", fontSize: 11, lineHeight: 1, padding: 0, opacity: 0.6 }}
                    >×</button>
                  </span>
                );
              })}
              {editingField === "labelInput" ? (
                <input
                  autoFocus
                  value={labelInput}
                  onChange={e => setLabelInput(e.target.value)}
                  onKeyDown={e => {
                    if (e.key === "Enter" && labelInput.trim()) {
                      const labels = [...issue.labels, labelInput.trim()].join(",");
                      setLabelInput(""); cancelEdit();
                      void saveField("labels", labels);
                    }
                    if (e.key === "Escape") { setLabelInput(""); cancelEdit(); }
                  }}
                  onBlur={() => { setLabelInput(""); cancelEdit(); }}
                  placeholder="label name"
                  style={{ ...inputStyle, width: 100 }}
                />
              ) : (
                <button
                  onClick={() => startEdit("labelInput", "")}
                  style={{
                    fontSize: 11, padding: "2px 8px", borderRadius: 5,
                    background: "rgba(51,65,85,0.15)", border: "1px dashed rgba(51,65,85,0.4)",
                    color: "#475569", cursor: "pointer", fontFamily: "inherit",
                  }}
                >+ Add</button>
              )}
            </div>

            {/* REFERENCE */}
            <span style={labelStyle}>REFERENCE</span>
            <span style={{ fontSize: 13, color: "#cbd5e1", fontFamily: "monospace" }}>{issue.external_id || compositeId(issue)}</span>

            {scopeValue && (
              <>
                <span style={labelStyle}>SCOPE</span>
                <span style={{ fontSize: 13, color: "#cbd5e1", wordBreak: "break-word" }}>{scopeValue}</span>
              </>
            )}

            {issue.url && (
              <>
                <span style={labelStyle}>LINK</span>
                <a href={issue.url} target="_blank" rel="noreferrer" style={{ fontSize: 12, color: "#3b82f6", textDecoration: "none", wordBreak: "break-all" }}>
                  {issue.url}
                </a>
              </>
            )}

            {issue.created_at && (
              <>
                <span style={labelStyle}>CREATED</span>
                <span style={{ fontSize: 13, color: "#64748b" }}>{formatRelative(issue.created_at)}</span>
              </>
            )}
          </div>

          {/* Description */}
          <div style={{ marginTop: 24, paddingTop: 20, borderTop: "1px solid rgba(51,65,85,0.15)" }}>
            <label style={{ fontSize: 10.5, fontWeight: 700, color: "#475569", letterSpacing: "0.08em", display: "block", marginBottom: 10 }}>DESCRIPTION</label>
            {editingField === "body" ? (
              <textarea
                autoFocus
                value={draft.body ?? (issue.body ?? "")}
                onChange={e => setDraft(prev => ({ ...prev, body: e.target.value }))}
                onBlur={() => { void saveField("body", draft.body ?? ""); }}
                onKeyDown={e => {
                  if (e.key === "Escape") cancelEdit();
                  if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) void saveField("body", draft.body ?? "");
                }}
                rows={6}
                placeholder="Add a description…"
                style={{ ...inputStyle, resize: "vertical" as const, lineHeight: 1.6 }}
              />
            ) : (
              <div
                onClick={() => startEdit("body", issue.body ?? "")}
                title="Click to edit description"
                style={{
                  padding: "12px 14px", borderRadius: 10, background: "rgba(2,6,23,0.4)",
                  border: "1px solid rgba(51,65,85,0.15)", cursor: "pointer",
                  color: issue.body ? "#cbd5e1" : "#475569",
                  fontSize: 13, lineHeight: 1.6, minHeight: 60,
                  whiteSpace: issue.body ? "pre-wrap" : undefined,
                  wordBreak: "break-word",
                  fontStyle: issue.body ? "normal" : "italic",
                }}
              >{issue.body ?? "No description — click to add one."}</div>
            )}
          </div>

          {/* Metadata */}
          {providerMetadata && (
            <div style={{ marginTop: 20, paddingTop: 16, borderTop: "1px solid rgba(51,65,85,0.1)" }}>
              <label style={{ fontSize: 10.5, fontWeight: 700, color: "#475569", letterSpacing: "0.08em", display: "block", marginBottom: 10 }}>METADATA</label>
              <pre style={{
                margin: 0, padding: 14, borderRadius: 10,
                background: "rgba(2,6,23,0.4)", border: "1px solid rgba(51,65,85,0.15)",
                color: "#cbd5e1", fontSize: 11, lineHeight: 1.6,
                whiteSpace: "pre-wrap", wordBreak: "break-word",
                fontFamily: "ui-monospace, Menlo, monospace",
              }}>{providerMetadata}</pre>
            </div>
          )}

          {/* Run info banner */}
          {linkedRun && (
            <div style={{
              marginTop: 20, padding: "12px 14px", borderRadius: 10,
              background: linkedRun.state === "done" ? "rgba(49,185,123,0.06)" : linkedRun.state === "failed" ? "rgba(239,68,68,0.06)" : "rgba(59,130,246,0.06)",
              border: `1px solid ${linkedRun.state === "done" ? "rgba(49,185,123,0.2)" : linkedRun.state === "failed" ? "rgba(239,68,68,0.2)" : "rgba(59,130,246,0.2)"}`,
            }}>
              <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <span style={{
                  fontSize: 10, fontWeight: 700, padding: "2px 8px", borderRadius: 5, textTransform: "uppercase" as const,
                  background: linkedRun.state === "done" ? "rgba(49,185,123,0.15)" : linkedRun.state === "failed" ? "rgba(239,68,68,0.15)" : "rgba(59,130,246,0.15)",
                  color: linkedRun.state === "done" ? "#4ade80" : linkedRun.state === "failed" ? "#f87171" : "#93c5fd",
                }}>{linkedRun.state}</span>
                <span style={{ fontSize: 12, color: "#94a3b8" }}>Run {linkedRun.id.slice(0, 8)}</span>
                {linkedRun.cost_used_usd > 0 && (
                  <span style={{ fontSize: 12, color: "#64748b" }}>${linkedRun.cost_used_usd.toFixed(2)}</span>
                )}
                {linkedRun.pr_url && (
                  <a href={linkedRun.pr_url} target="_blank" rel="noreferrer"
                    style={{ fontSize: 12, color: "#3b82f6", textDecoration: "none", marginLeft: "auto" }}>
                    View PR ↗
                  </a>
                )}
              </div>
              {linkedRun.publish_error && (
                <div style={{ marginTop: 6, fontSize: 12, color: "#f87171" }}>{linkedRun.publish_error}</div>
              )}
            </div>
          )}

          {/* Reopen */}
          {isDone && (
            <button
              onClick={() => void handleReopen()}
              style={{
                marginTop: 16, display: "flex", alignItems: "center", gap: 6, padding: "8px 14px",
                borderRadius: 8, fontSize: 12, fontWeight: 600, cursor: "pointer", fontFamily: "inherit",
                background: "rgba(49,185,123,0.08)", border: "1px solid rgba(49,185,123,0.2)", color: "#4ade80",
              }}
            >↺ Reopen</button>
          )}

          {/* Push to Provider (Grove-native only, shows only configured providers) */}
          {issue.is_native && configuredProviders.length > 0 && (
            <div style={{ marginTop: 24, paddingTop: 20, borderTop: "1px solid rgba(51,65,85,0.15)" }}>
              <label style={{ fontSize: 10.5, fontWeight: 700, color: "#475569", letterSpacing: "0.08em", display: "block", marginBottom: 10 }}>PUSH TO PLATFORM</label>
              <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                {configuredProviders.map(p => (
                  <button
                    key={p}
                    onClick={() => void handlePush(p)}
                    disabled={pushing}
                    style={{
                      padding: "7px 14px", borderRadius: 8, fontSize: 12, fontWeight: 600,
                      background: "rgba(51,65,85,0.15)", border: "1px solid rgba(51,65,85,0.25)",
                      color: "#94a3b8", cursor: pushing ? "default" : "pointer",
                      fontFamily: "inherit", opacity: pushing ? 0.5 : 1,
                    }}
                  >{pushing ? "Pushing…" : `→ ${p.charAt(0).toUpperCase() + p.slice(1)}`}</button>
                ))}
              </div>
              {pushError && <div style={{ marginTop: 8, fontSize: 12, color: "#f87171" }}>{pushError}</div>}
            </div>
          )}

          {/* Comments / Activity tabs */}
          <div style={{ marginTop: 28, paddingTop: 20, borderTop: "1px solid rgba(51,65,85,0.15)" }}>
            <div style={{ display: "flex", gap: 4, marginBottom: 16 }}>
              {(["comments", "activity"] as DetailTab[]).map(t => (
                <button key={t} onClick={() => setTab(t)} style={{
                  padding: "5px 12px", borderRadius: 7, fontSize: 12, fontWeight: tab === t ? 600 : 500,
                  border: "none", cursor: "pointer", fontFamily: "inherit", transition: "all .15s",
                  background: tab === t ? "rgba(99,102,241,0.12)" : "transparent",
                  color: tab === t ? "#818cf8" : "#475569",
                }}>
                  {t.charAt(0).toUpperCase() + t.slice(1)} ({t === "comments" ? comments.length : activity.length})
                </button>
              ))}
            </div>

            {tab === "comments" && (
              <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                {comments.map(c => (
                  <div key={c.id} style={{
                    padding: "10px 14px", borderRadius: 10,
                    background: c.author?.startsWith("grove/") ? "rgba(49,185,123,0.04)" : "rgba(15,23,42,0.5)",
                    border: c.author?.startsWith("grove/") ? "1px solid rgba(49,185,123,0.15)" : "1px solid rgba(51,65,85,0.2)",
                  }}>
                    <div style={{ display: "flex", gap: 6, marginBottom: 6, alignItems: "center" }}>
                      <span style={{ fontSize: 11, fontWeight: 600, color: c.author?.startsWith("grove/") ? "#4ade80" : "#818cf8" }}>{c.author ?? "grove"}</span>
                      <span style={{ fontSize: 11, color: "#334155" }}>{formatRelative(c.created_at)}</span>
                      {c.posted_to_provider && <span style={{ fontSize: 10, color: "#3b82f6", marginLeft: "auto" }}>synced</span>}
                    </div>
                    <div style={{ fontSize: 13, color: "#cbd5e1", whiteSpace: "pre-wrap", wordBreak: "break-word", lineHeight: 1.5 }}>{c.body}</div>
                  </div>
                ))}
                {comments.length === 0 && loaded && <div style={{ fontSize: 12, color: "#334155", fontStyle: "italic" }}>No comments yet.</div>}
              </div>
            )}

            {tab === "activity" && (
              <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                {activity.map(evt => (
                  <div key={evt.id} style={{ display: "flex", gap: 10, alignItems: "flex-start" }}>
                    <span style={{ fontSize: 10, color: "#334155", whiteSpace: "nowrap", marginTop: 2, minWidth: 64 }}>{formatRelative(evt.created_at)}</span>
                    <div>
                      <span style={{ fontSize: 12, color: "#94a3b8" }}>{evt.event_type}</span>
                      {evt.actor && <span style={{ fontSize: 11, color: "#475569" }}> by {evt.actor}</span>}
                      {evt.old_value && evt.new_value && <span style={{ fontSize: 11, color: "#475569" }}> {evt.old_value} → {evt.new_value}</span>}
                    </div>
                  </div>
                ))}
                {activity.length === 0 && loaded && <div style={{ fontSize: 12, color: "#334155", fontStyle: "italic" }}>No activity yet.</div>}
              </div>
            )}

            {tab === "comments" && (
              <div style={{ marginTop: 16, display: "flex", gap: 8 }}>
                <textarea
                  value={comment}
                  onChange={e => setComment(e.target.value)}
                  placeholder="Add a comment… (Ctrl+Enter to post)"
                  rows={2}
                  style={{
                    flex: 1, resize: "none" as const, fontSize: 13, fontFamily: "inherit",
                    background: "rgba(2,6,23,0.6)", color: "#e2e8f0",
                    border: "1px solid rgba(51,65,85,0.3)", borderRadius: 10,
                    padding: "10px 14px", outline: "none", lineHeight: 1.5,
                  }}
                  onFocus={e => { e.target.style.borderColor = "rgba(99,102,241,0.5)"; }}
                  onBlur={e => { e.target.style.borderColor = "rgba(51,65,85,0.3)"; }}
                  onKeyDown={e => {
                    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void handleAddComment(); }
                  }}
                />
                <button
                  onClick={() => void handleAddComment()}
                  disabled={!comment.trim() || submitting}
                  style={{
                    padding: "10px 14px", borderRadius: 10, fontSize: 12, fontWeight: 600,
                    background: comment.trim() ? "rgba(49,185,123,0.1)" : "rgba(51,65,85,0.15)",
                    border: comment.trim() ? "1px solid rgba(49,185,123,0.2)" : "1px solid rgba(51,65,85,0.2)",
                    color: comment.trim() ? "#4ade80" : "#334155",
                    cursor: comment.trim() ? "pointer" : "default",
                    fontFamily: "inherit", alignSelf: "flex-end",
                  }}
                >Post</button>
              </div>
            )}
          </div>
        </div>

        {/* Footer — delete button for native Grove issues only */}
        {issue.provider === "grove" && issue.is_native && (
          <div style={{
            padding: "16px 24px", borderTop: "1px solid rgba(51,65,85,0.2)",
            display: "flex", gap: 8, flexShrink: 0, background: "rgba(2,6,23,0.3)",
          }}>
            <button
              onClick={() => void handleDelete()}
              style={{
                flex: 1, padding: "10px 0", borderRadius: 9, fontSize: 13, fontWeight: 600,
                background: "rgba(239,68,68,0.08)", border: "1px solid rgba(239,68,68,0.15)",
                color: "#ef4444", cursor: "pointer", fontFamily: "inherit",
              }}
            >Delete</button>
          </div>
        )}
      </div>
    </div>
  );
}
