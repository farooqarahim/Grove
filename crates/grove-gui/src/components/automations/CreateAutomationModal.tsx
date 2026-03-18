import { useState, useMemo, useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  createAutomation,
  listConversations,
  getAgentCatalog,
  listProjects,
  getProjectSettings,
  checkConnections,
  listProviderStatuses,
} from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { C, lbl } from "@/lib/theme";
import { XIcon } from "@/components/ui/icons";
import { CronSchedulePicker } from "./CronSchedulePicker";
import type {
  AgentCatalogEntry,
  AgentModelEntry,
  ProjectRow,
  ProjectSettings,
  ConnectionStatus,
  IssueTrackerStatus,
} from "@/types";

interface Props {
  open: boolean;
  projectId: string | null;
  onClose: () => void;
  onCreated: () => void;
}

type TriggerType = "cron" | "webhook" | "manual" | "issue";
type SessionModeChoice = "new" | "dedicated";

const TRIGGER_OPTIONS: { value: TriggerType; label: string; desc: string }[] = [
  { value: "cron", label: "Cron", desc: "Run on a schedule" },
  { value: "manual", label: "Manual", desc: "Trigger by hand" },
  { value: "issue", label: "Issue", desc: "When issues match" },
  { value: "webhook", label: "Webhook", desc: "External HTTP call" },
];

const TRACKER_PROVIDERS = [
  { id: "github", label: "GitHub", settingsKey: "github_project_key" as const },
  { id: "jira", label: "Jira", settingsKey: "jira_project_key" as const },
  { id: "linear", label: "Linear", settingsKey: "linear_project_key" as const },
] as const;

const inputStyle: React.CSSProperties = {
  width: "100%",
  height: 36,
  padding: "0 12px",
  borderRadius: 6,
  border: `1px solid ${C.border}`,
  background: C.surfaceHover,
  color: C.text1,
  fontSize: 13,
  fontFamily: "inherit",
  outline: "none",
  boxSizing: "border-box",
  transition: "border-color .15s",
};

const selectStyle: React.CSSProperties = {
  ...inputStyle,
  appearance: "none",
  backgroundImage: `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 16 16' fill='none' stroke='%2364748b' stroke-width='2'%3E%3Cpath d='M4 6l4 4 4-4'/%3E%3C/svg%3E")`,
  backgroundRepeat: "no-repeat",
  backgroundPosition: "right 12px center",
  paddingRight: 32,
  cursor: "pointer",
};

const sectionGap = 16;

const CATEGORY_ORDER = ["backlog", "todo", "in_progress", "done", "cancelled"];
const CATEGORY_LABELS: Record<string, string> = {
  backlog: "Backlog",
  todo: "To Do",
  in_progress: "In Progress",
  done: "Done",
  cancelled: "Cancelled",
};

export function CreateAutomationModal({ open, projectId: initialProjectId, onClose, onCreated }: Props) {
  const [selectedProjectId, setSelectedProjectId] = useState<string>(initialProjectId ?? "");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [triggerType, setTriggerType] = useState<TriggerType>("cron");
  const [schedule, setSchedule] = useState("");
  const [provider, setProvider] = useState("");
  const [model, setModel] = useState("");
  const [sessionMode, setSessionMode] = useState<SessionModeChoice>("new");
  const [dedicatedConvId, setDedicatedConvId] = useState("");
  // Issue trigger fields
  const [issueProvider, setIssueProvider] = useState("");
  const [selectedStatuses, setSelectedStatuses] = useState<Set<string>>(new Set());
  const [issueLabels, setIssueLabels] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Sync initial project when modal opens
  useEffect(() => {
    if (open && initialProjectId) {
      setSelectedProjectId(initialProjectId);
    }
  }, [open, initialProjectId]);

  // Reset issue provider when project changes
  useEffect(() => {
    setIssueProvider("");
    setSelectedStatuses(new Set());
  }, [selectedProjectId]);

  // ── Queries ──────────────────────────────────────────

  const { data: projects = [] } = useQuery({
    queryKey: qk.projects(),
    queryFn: listProjects,
    staleTime: 30000,
    enabled: open,
  });

  const { data: agents } = useQuery({
    queryKey: qk.agentCatalog(),
    queryFn: getAgentCatalog,
    staleTime: 60000,
    enabled: open,
  });

  const { data: conversations } = useQuery({
    queryKey: qk.conversations(selectedProjectId || null, 200),
    queryFn: () => selectedProjectId ? listConversations(200, selectedProjectId) : Promise.resolve([]),
    staleTime: 30000,
    enabled: open && sessionMode === "dedicated" && !!selectedProjectId,
  });

  const { data: projectSettings } = useQuery<ProjectSettings>({
    queryKey: ["projectSettings", selectedProjectId],
    queryFn: () => getProjectSettings(selectedProjectId),
    staleTime: 30000,
    enabled: open && triggerType === "issue" && !!selectedProjectId,
  });

  const { data: connections } = useQuery<ConnectionStatus[]>({
    queryKey: ["connections"],
    queryFn: checkConnections,
    staleTime: 30000,
    enabled: open && triggerType === "issue",
  });

  const { data: trackerStatuses } = useQuery<IssueTrackerStatus[]>({
    queryKey: ["trackerStatuses", issueProvider, selectedProjectId],
    queryFn: () => listProviderStatuses(issueProvider, selectedProjectId),
    staleTime: 30000,
    enabled: open && triggerType === "issue" && !!issueProvider && !!selectedProjectId,
  });

  // ── Derived data ────────────────────────────────────

  const enabledAgents = useMemo(() => {
    if (!agents) return [];
    return agents.filter((a: AgentCatalogEntry) => a.enabled);
  }, [agents]);

  // Which tracker providers are both connected AND configured for this project?
  const availableTrackers = useMemo(() => {
    if (!connections || !projectSettings) return [];
    return TRACKER_PROVIDERS.filter((tp) => {
      const conn = connections.find((c) => c.provider === tp.id);
      const isConnected = conn?.connected === true;
      const projectKey = projectSettings[tp.settingsKey];
      const isConfigured = !!projectKey;
      return isConnected && isConfigured;
    });
  }, [connections, projectSettings]);

  const activeProjects = useMemo(() => {
    return projects.filter((p: ProjectRow) => p.state === "active");
  }, [projects]);

  const selectedProject = activeProjects.find((p: ProjectRow) => p.id === selectedProjectId);

  // Group statuses by category for nicer display
  const statusesByCategory = useMemo(() => {
    if (!trackerStatuses) return new Map<string, IssueTrackerStatus[]>();
    const map = new Map<string, IssueTrackerStatus[]>();
    for (const s of trackerStatuses) {
      const list = map.get(s.category) ?? [];
      list.push(s);
      map.set(s.category, list);
    }
    return map;
  }, [trackerStatuses]);

  if (!open) return null;

  const canSubmit =
    name.trim().length > 0 &&
    !!selectedProjectId &&
    (triggerType !== "cron" || schedule.trim().length > 0) &&
    (triggerType !== "issue" || (issueProvider && selectedStatuses.size > 0 && schedule.trim().length > 0)) &&
    !submitting;

  function reset() {
    setName("");
    setDescription("");
    setTriggerType("cron");
    setSchedule("");
    setProvider("");
    setModel("");
    setSessionMode("new");
    setDedicatedConvId("");
    setIssueProvider("");
    setSelectedStatuses(new Set());
    setIssueLabels("");
    setSubmitting(false);
    setError(null);
  }

  function handleClose() {
    reset();
    onClose();
  }

  function toggleStatus(statusName: string) {
    setSelectedStatuses((prev) => {
      const next = new Set(prev);
      if (next.has(statusName)) {
        next.delete(statusName);
      } else {
        next.add(statusName);
      }
      return next;
    });
  }

  async function handleSubmit() {
    if (!canSubmit) return;
    setSubmitting(true);
    setError(null);

    try {
      let triggerConfigJson: string;
      if (triggerType === "cron") {
        triggerConfigJson = JSON.stringify({ type: "cron", schedule });
      } else if (triggerType === "webhook") {
        triggerConfigJson = JSON.stringify({ type: "webhook" });
      } else if (triggerType === "issue") {
        triggerConfigJson = JSON.stringify({
          type: "issue",
          schedule,
          statuses: Array.from(selectedStatuses),
          labels: issueLabels ? issueLabels.split(",").map(s => s.trim()).filter(Boolean) : [],
        });
      } else {
        triggerConfigJson = JSON.stringify({ type: "manual" });
      }

      const defaultsJson =
        provider || model
          ? JSON.stringify({
              provider: provider || null,
              model: model || null,
              pipeline: null,
              permission_mode: null,
            })
          : undefined;

      await createAutomation(
        selectedProjectId,
        name.trim(),
        triggerConfigJson,
        defaultsJson,
        description.trim() || undefined,
        sessionMode,
        sessionMode === "dedicated" && dedicatedConvId ? dedicatedConvId : undefined,
      );
      onCreated();
      handleClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setSubmitting(false);
    }
  }

  return (
    <div
      onClick={handleClose}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 1000,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.55)",
        backdropFilter: "blur(4px)",
      }}
    >
      {/* Card */}
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 540,
          maxHeight: "88vh",
          overflowY: "auto",
          background: C.surface,
          border: `1px solid ${C.border}`,
          borderRadius: 12,
          boxShadow: "0 16px 48px rgba(0,0,0,0.45)",
          display: "flex",
          flexDirection: "column",
        }}
      >
        {/* Header */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "20px 24px 0",
          }}
        >
          <h2
            style={{
              margin: 0,
              fontSize: 16,
              fontWeight: 700,
              color: C.text1,
              letterSpacing: "-0.02em",
            }}
          >
            New Automation
          </h2>
          <button
            onClick={handleClose}
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              width: 28,
              height: 28,
              borderRadius: 6,
              border: "none",
              background: "transparent",
              color: "#64748b",
              cursor: "pointer",
              transition: "background .12s, color .12s",
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = C.surfaceHover;
              e.currentTarget.style.color = C.text1;
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = "transparent";
              e.currentTarget.style.color = "#64748b";
            }}
          >
            <XIcon size={12} />
          </button>
        </div>

        {/* Form */}
        <div style={{ padding: "20px 24px", display: "flex", flexDirection: "column", gap: sectionGap }}>
          {/* ── Project Picker ──────────────────────── */}
          <div>
            <div style={lbl}>Project</div>
            <select
              value={selectedProjectId}
              onChange={(e) => setSelectedProjectId(e.target.value)}
              style={selectStyle}
            >
              <option value="">Select a project...</option>
              {activeProjects.map((p: ProjectRow) => (
                <option key={p.id} value={p.id}>
                  {p.name || p.root_path.split("/").pop() || p.id.slice(0, 8)}
                </option>
              ))}
            </select>
            {selectedProject && (
              <div style={{ fontSize: 10, color: "#475569", marginTop: 3, fontFamily: C.mono }}>
                {selectedProject.root_path}
              </div>
            )}
          </div>

          {/* Name */}
          <div>
            <div style={lbl}>Name</div>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="weekly-dep-update"
              style={inputStyle}
              onFocus={(e) => { e.currentTarget.style.borderColor = C.accent; }}
              onBlur={(e) => { e.currentTarget.style.borderColor = C.border; }}
            />
          </div>

          {/* Description */}
          <div>
            <div style={lbl}>Description</div>
            <input
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="What does this automation do?"
              style={inputStyle}
              onFocus={(e) => { e.currentTarget.style.borderColor = C.accent; }}
              onBlur={(e) => { e.currentTarget.style.borderColor = C.border; }}
            />
          </div>

          {/* Trigger Type */}
          <div>
            <div style={lbl}>Trigger Type</div>
            <div
              style={{
                display: "flex",
                background: C.surfaceHover,
                borderRadius: 8,
                border: `1px solid ${C.border}`,
                padding: 3,
              }}
            >
              {TRIGGER_OPTIONS.map((opt) => {
                const active = triggerType === opt.value;
                return (
                  <button
                    key={opt.value}
                    onClick={() => setTriggerType(opt.value)}
                    title={opt.desc}
                    style={{
                      flex: 1,
                      padding: "7px 0",
                      borderRadius: 6,
                      border: "none",
                      cursor: "pointer",
                      fontFamily: "inherit",
                      fontSize: 12,
                      fontWeight: 600,
                      transition: "all .15s",
                      background: active ? C.accent : "transparent",
                      color: active ? "#fff" : "#64748b",
                    }}
                  >
                    {opt.label}
                  </button>
                );
              })}
            </div>
          </div>

          {/* Cron Schedule (only when trigger=cron) */}
          {triggerType === "cron" && (
            <CronSchedulePicker value={schedule} onChange={setSchedule} />
          )}

          {/* ── Issue Trigger Config ────────────────── */}
          {triggerType === "issue" && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12, padding: "8px 0" }}>
              {/* Info banner */}
              <div style={{
                padding: "10px 14px",
                borderRadius: 8,
                background: "rgba(59,130,246,0.06)",
                border: "1px solid rgba(59,130,246,0.15)",
                fontSize: 12,
                color: "#93c5fd",
                lineHeight: 1.5,
              }}>
                Scans your project's issue board and triggers a run for each issue
                matching the configured statuses. Use <code style={{ fontSize: 11, background: "rgba(255,255,255,0.06)", padding: "1px 4px", borderRadius: 3 }}>{"{{issue.title}}"}</code> and <code style={{ fontSize: 11, background: "rgba(255,255,255,0.06)", padding: "1px 4px", borderRadius: 3 }}>{"{{issue.body}}"}</code> in step objectives.
              </div>

              {!selectedProjectId ? (
                <div style={{
                  padding: "12px 14px",
                  borderRadius: 8,
                  background: "rgba(251,191,36,0.06)",
                  border: "1px solid rgba(251,191,36,0.2)",
                  fontSize: 12,
                  color: "#fbbf24",
                  lineHeight: 1.5,
                }}>
                  Select a project above to configure issue triggers.
                </div>
              ) : !projectSettings ? (
                <div style={{ fontSize: 12, color: "#64748b", padding: "8px 0" }}>
                  Loading project settings...
                </div>
              ) : availableTrackers.length === 0 ? (
                /* No tracker configured/connected */
                <div style={{
                  padding: "14px 16px",
                  borderRadius: 8,
                  background: "rgba(251,191,36,0.06)",
                  border: "1px solid rgba(251,191,36,0.2)",
                  fontSize: 12,
                  color: "#fbbf24",
                  lineHeight: 1.7,
                }}>
                  <div style={{ fontWeight: 700, marginBottom: 4 }}>
                    No issue tracker configured
                  </div>
                  <div>
                    Go to <strong>Project Settings</strong> to connect an issue board
                    (GitHub, Jira, or Linear) for
                    {" "}<strong>{selectedProject?.name || "this project"}</strong>.
                  </div>
                  <div style={{ marginTop: 6, fontSize: 11, color: "#d4a017" }}>
                    {(() => {
                      const unconfigured = TRACKER_PROVIDERS.filter((tp) => {
                        const conn = connections?.find((c) => c.provider === tp.id);
                        return conn?.connected && !projectSettings[tp.settingsKey];
                      });
                      if (unconfigured.length > 0) {
                        return `Connected but not linked to this project: ${unconfigured.map((t) => t.label).join(", ")}`;
                      }
                      const disconnected = TRACKER_PROVIDERS.filter((tp) => {
                        const conn = connections?.find((c) => c.provider === tp.id);
                        return !conn?.connected;
                      });
                      if (disconnected.length > 0) {
                        return `Not connected: ${disconnected.map((t) => t.label).join(", ")}. Connect in Settings → Connections.`;
                      }
                      return null;
                    })()}
                  </div>
                </div>
              ) : (
                <>
                  {/* Issue Tracker Picker */}
                  <div>
                    <div style={lbl}>Issue Tracker</div>
                    <select
                      value={issueProvider}
                      onChange={(e) => {
                        setIssueProvider(e.target.value);
                        setSelectedStatuses(new Set());
                      }}
                      style={selectStyle}
                    >
                      <option value="">Select tracker...</option>
                      {availableTrackers.map((tp) => {
                        const projectKey = projectSettings?.[tp.settingsKey];
                        return (
                          <option key={tp.id} value={tp.id}>
                            {tp.label}{projectKey ? ` — ${projectKey}` : ""}
                          </option>
                        );
                      })}
                    </select>
                    {issueProvider && (() => {
                      const tp = TRACKER_PROVIDERS.find((t) => t.id === issueProvider);
                      const projectKey = tp && projectSettings ? projectSettings[tp.settingsKey] : null;
                      return projectKey ? (
                        <div style={{ fontSize: 10, color: "#475569", marginTop: 3 }}>
                          Board: <span style={{ fontFamily: C.mono }}>{projectKey}</span>
                        </div>
                      ) : null;
                    })()}
                  </div>

                  {/* Status selector */}
                  {issueProvider && (
                    <div>
                      <div style={lbl}>
                        Watch Statuses
                        {selectedStatuses.size > 0 && (
                          <span style={{ fontWeight: 400, color: "#64748b", marginLeft: 6 }}>
                            ({selectedStatuses.size} selected)
                          </span>
                        )}
                      </div>
                      {!trackerStatuses ? (
                        <div style={{ fontSize: 12, color: "#64748b", padding: "8px 0" }}>
                          Loading statuses...
                        </div>
                      ) : trackerStatuses.length === 0 ? (
                        <div style={{ fontSize: 12, color: "#64748b", padding: "8px 0" }}>
                          No statuses found for this tracker.
                        </div>
                      ) : (
                        <div style={{
                          border: `1px solid ${C.border}`,
                          borderRadius: 8,
                          background: C.surfaceHover,
                          padding: "8px 0",
                          maxHeight: 200,
                          overflowY: "auto",
                        }}>
                          {CATEGORY_ORDER.filter((cat) => statusesByCategory.has(cat)).map((cat) => {
                            const statuses = statusesByCategory.get(cat)!;
                            return (
                              <div key={cat}>
                                <div style={{
                                  fontSize: 9,
                                  fontWeight: 700,
                                  textTransform: "uppercase",
                                  letterSpacing: "0.06em",
                                  color: "#475569",
                                  padding: "6px 14px 3px",
                                }}>
                                  {CATEGORY_LABELS[cat] || cat}
                                </div>
                                {statuses.map((s) => {
                                  const checked = selectedStatuses.has(s.name);
                                  return (
                                    <label
                                      key={s.id}
                                      style={{
                                        display: "flex",
                                        alignItems: "center",
                                        gap: 8,
                                        padding: "5px 14px",
                                        cursor: "pointer",
                                        transition: "background .1s",
                                        background: checked ? "rgba(49,185,123,0.06)" : "transparent",
                                      }}
                                      onMouseEnter={(e) => {
                                        if (!checked) e.currentTarget.style.background = "rgba(255,255,255,0.03)";
                                      }}
                                      onMouseLeave={(e) => {
                                        e.currentTarget.style.background = checked ? "rgba(49,185,123,0.06)" : "transparent";
                                      }}
                                    >
                                      <input
                                        type="checkbox"
                                        checked={checked}
                                        onChange={() => toggleStatus(s.name)}
                                        style={{ accentColor: C.accent }}
                                      />
                                      {s.color && (
                                        <span style={{
                                          width: 8,
                                          height: 8,
                                          borderRadius: "50%",
                                          background: s.color,
                                          flexShrink: 0,
                                        }} />
                                      )}
                                      <span style={{ fontSize: 12, color: C.text2 }}>
                                        {s.name}
                                      </span>
                                    </label>
                                  );
                                })}
                              </div>
                            );
                          })}
                        </div>
                      )}
                    </div>
                  )}

                  {/* Label filter */}
                  <div>
                    <div style={lbl}>Label Filter (optional)</div>
                    <input
                      value={issueLabels}
                      onChange={(e) => setIssueLabels(e.target.value)}
                      placeholder="bug, critical"
                      style={inputStyle}
                      onFocus={(e) => { e.currentTarget.style.borderColor = C.accent; }}
                      onBlur={(e) => { e.currentTarget.style.borderColor = C.border; }}
                    />
                    <div style={{ fontSize: 10, color: "#475569", marginTop: 3 }}>
                      Only trigger for issues with ALL of these labels.
                    </div>
                  </div>

                  {/* Schedule */}
                  <CronSchedulePicker value={schedule} onChange={setSchedule} />
                </>
              )}
            </div>
          )}

          {/* ── Session Mode ──────────────────────── */}
          <div>
            <div style={lbl}>Session Mode</div>
            <div
              style={{
                display: "flex",
                background: C.surfaceHover,
                borderRadius: 8,
                border: `1px solid ${C.border}`,
                padding: 3,
              }}
            >
              {(["new", "dedicated"] as SessionModeChoice[]).map((m) => {
                const active = sessionMode === m;
                return (
                  <button
                    key={m}
                    onClick={() => setSessionMode(m)}
                    style={{
                      flex: 1,
                      padding: "7px 0",
                      borderRadius: 6,
                      border: "none",
                      cursor: "pointer",
                      fontFamily: "inherit",
                      fontSize: 12,
                      fontWeight: 600,
                      transition: "all .15s",
                      background: active ? C.blue : "transparent",
                      color: active ? "#fff" : "#64748b",
                    }}
                  >
                    {m === "new" ? "New Session Each Run" : "Dedicated Session"}
                  </button>
                );
              })}
            </div>
            <div style={{ fontSize: 10, color: "#475569", marginTop: 4 }}>
              {sessionMode === "new"
                ? "Each automation run creates a fresh conversation."
                : "All runs share a single conversation (thread reuse)."}
            </div>
          </div>

          {/* Dedicated conversation picker */}
          {sessionMode === "dedicated" && (
            <div>
              <div style={lbl}>Conversation</div>
              <select
                value={dedicatedConvId}
                onChange={(e) => setDedicatedConvId(e.target.value)}
                style={selectStyle}
              >
                <option value="">Create new on first run</option>
                {conversations?.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.title || `Session ${c.id.slice(0, 8)}`}
                  </option>
                ))}
              </select>
            </div>
          )}

          {/* ── Default Provider ──────────────────────── */}
          <div>
            <div style={lbl}>Default Agent</div>
            <select
              value={provider}
              onChange={(e) => {
                setProvider(e.target.value);
                const agent = enabledAgents.find((a: AgentCatalogEntry) => a.id === e.target.value);
                if (agent?.models?.length) {
                  const defaultModel = agent.models.find((m: AgentModelEntry) => m.is_default);
                  setModel(defaultModel?.id ?? agent.models[0].id);
                }
              }}
              style={selectStyle}
            >
              <option value="">Default (from project settings)</option>
              {enabledAgents.map((a: AgentCatalogEntry) => (
                <option key={a.id} value={a.id}>
                  {a.name}
                </option>
              ))}
            </select>
          </div>

          {/* Model (show only when a provider with models is selected) */}
          {provider && (() => {
            const agent = enabledAgents.find((a: AgentCatalogEntry) => a.id === provider);
            const models: AgentModelEntry[] = agent?.models ?? [];
            return models.length > 0 ? (
              <div>
                <div style={lbl}>Default Model</div>
                <select
                  value={model}
                  onChange={(e) => setModel(e.target.value)}
                  style={selectStyle}
                >
                  {models.map((m: AgentModelEntry) => (
                    <option key={m.id} value={m.id}>{m.name || m.id}</option>
                  ))}
                </select>
              </div>
            ) : null;
          })()}
        </div>

        {/* Footer */}
        <div
          style={{
            padding: "0 24px 20px",
            display: "flex",
            flexDirection: "column",
            gap: 12,
          }}
        >
          {/* Inline error */}
          {error && (
            <div
              style={{
                padding: "8px 12px",
                borderRadius: 6,
                background: C.dangerDim,
                border: `1px solid rgba(239,68,68,0.3)`,
                color: C.danger,
                fontSize: 12,
                lineHeight: 1.5,
              }}
            >
              {error}
            </div>
          )}

          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <button
              onClick={handleClose}
              style={{
                padding: "8px 18px",
                borderRadius: 8,
                border: `1px solid ${C.border}`,
                background: "transparent",
                color: "#64748b",
                fontSize: 13,
                fontWeight: 600,
                cursor: "pointer",
                fontFamily: "inherit",
                transition: "background .12s, color .12s",
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.background = C.surfaceHover;
                e.currentTarget.style.color = C.text2;
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.background = "transparent";
                e.currentTarget.style.color = "#64748b";
              }}
            >
              Cancel
            </button>
            <button
              onClick={handleSubmit}
              disabled={!canSubmit}
              style={{
                padding: "8px 22px",
                borderRadius: 8,
                border: "1px solid rgba(49,185,123,0.3)",
                background: canSubmit
                  ? "linear-gradient(135deg, #31B97B, #269962)"
                  : "rgba(49,185,123,0.15)",
                color: canSubmit ? "#fff" : "rgba(49,185,123,0.4)",
                fontSize: 13,
                fontWeight: 700,
                cursor: canSubmit ? "pointer" : "default",
                fontFamily: "inherit",
                boxShadow: canSubmit ? "0 0 20px rgba(49,185,123,0.12)" : "none",
                transition: "all .2s",
              }}
            >
              {submitting ? "Creating..." : "Create"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
