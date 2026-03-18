import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Bolt, XIcon } from "@/components/ui/icons";
import {
  startRun, startRunFromIssue,
  listProviderIssues, checkConnections, getProjectSettings,
  getAgentCatalog, getDefaultProvider, getLastSessionInfo,
} from "@/lib/api";
import type { LastSessionInfo } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { formatRunBundleLabel, formatRunPipelineLabel } from "@/lib/runLabels";
import { C, lbl } from "@/lib/theme";
import type { Issue, ProjectRow, AgentCatalogEntry } from "@/types";

interface NewRunModalProps {
  open: boolean;
  onClose: () => void;
  conversationId: string | null;
  /** When set, the modal is in "Continue Task" mode: locks provider/model to the
   *  prior run's values and resumes its provider thread (e.g. codex thread_id). */
  resumeFromRunId: string | null;
  projectId: string | null;
  projects?: ProjectRow[];
  onProjectChange?: (projectId: string | null) => void;
  onStarted?: (conversationId: string) => void;
  /** Session name provided by SessionNameModal for new sessions. */
  sessionName?: string | null;
}

function isInternalWorkspaceProject(project: { root_path: string }): boolean {
  return project.root_path.includes("/.grove/workspaces/");
}

const PERMISSION_MODES = [
  { value: "skip_all", label: "Auto-approve all tools" },
  { value: "human_gate", label: "Ask human per tool" },
  { value: "autonomous_gate", label: "AI gatekeeper per tool" },
];

const CONNECTOR_SOURCES = [
  { value: "", label: "None" },
  { value: "github", label: "GitHub" },
  { value: "jira", label: "Jira" },
  { value: "linear", label: "Linear" },
  { value: "grove", label: "Grove Issues" },
];

function projectLabel(project: ProjectRow): string {
  return project.name || project.root_path.split("/").pop() || project.id;
}

function inferClassicRunPipeline(objective: string): string {
  const normalized = objective.toLowerCase();
  const bugTerms = ["bug", "error", "failing test", "failure", "broken", "regression", "compile", "panic", "exception", "crash", "issue", "hotfix"];

  const hasBugfix = bugTerms.some((term) => normalized.includes(term))
    || (normalized.includes("fix")
      && ["test", "build", "compile", "runtime", "crash", "panic", "exception", "bug", "regression"]
        .some((term) => normalized.includes(term)));
  if (hasBugfix) return "bugfix";

  return "build_validate_judge";
}

export function NewRunModal({
  open,
  onClose,
  conversationId,
  resumeFromRunId,
  projectId,
  projects = [],
  onProjectChange,
  onStarted,
  sessionName = null,
}: NewRunModalProps) {
  const activeProjects = useMemo(() => {
    const active = projects.filter((project) => project.state === "active");
    const preferred = active.filter((project) => !isInternalWorkspaceProject(project));
    return preferred.length > 0 ? preferred : active;
  }, [projects]);
  const defaultProjectId =
    (projectId && activeProjects.some((project) => project.id === projectId) ? projectId : null)
    ?? activeProjects[0]?.id
    ?? null;
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(defaultProjectId);
  const [objective, setObjective] = useState("");
  const [selectedAgent, setSelectedAgent] = useState<string>("");
  const [selectedModel, setSelectedModel] = useState<string>("");
  const [customModel, setCustomModel] = useState<string>("");
  const [permissionMode, setPermissionMode] = useState("skip_all");
  const [disablePhaseGates, setDisablePhaseGates] = useState(false);
  const [interactive, setInteractive] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Session continuity: fetched when continuing an existing conversation.
  // When set, provider/model are locked to the prior run's values.
  const [resumeInfo, setResumeInfo] = useState<LastSessionInfo | null>(null);

  // Connector state
  const [connector, setConnector] = useState("");
  const [connectorIssues, setConnectorIssues] = useState<Issue[]>([]);
  const [loadingIssues, setLoadingIssues] = useState(false);
  const [selectedIssue, setSelectedIssue] = useState<Issue | null>(null);
  const effectiveProjectId = selectedProjectId ?? projectId ?? null;
  const effectiveProject = activeProjects.find((project) => project.id === effectiveProjectId) ?? null;

  // Connection statuses (to show which connectors are available)
  const { data: connections } = useQuery({
    queryKey: qk.connections(),
    queryFn: checkConnections,
    refetchInterval: 30000,
    staleTime: 15000,
  });

  // Load project settings once when the modal opens with a valid projectId,
  // and apply any configured defaults that the user hasn't already set.
  useEffect(() => {
    if (!open || !effectiveProjectId) return;
    getProjectSettings(effectiveProjectId)
      .then((s) => {
        if (s.default_permission_mode) setPermissionMode(s.default_permission_mode);
        // Pre-select the connector source if the project has a default provider.
        if (s.default_provider && s.default_provider !== "grove") {
          setConnector(s.default_provider);
        }
      })
      .catch(() => {});
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, effectiveProjectId]);

  // "Continue Task" mode: fetch the specific run's provider/model/thread so we
  // can lock them. Only fires when resumeFromRunId is explicitly set — regular
  // "New Run" on an existing conversation leaves resumeFromRunId null and gets
  // a free choice of provider/model with no thread resumption.
  useEffect(() => {
    if (!open || !resumeFromRunId) {
      setResumeInfo(null);
      return;
    }
    getLastSessionInfo(resumeFromRunId)
      .then((info) => setResumeInfo(info))
      .catch(() => setResumeInfo(null));
  }, [open, resumeFromRunId]);

  // Fetch agent catalog and default provider
  const { data: agentCatalog = [] } = useQuery({
    queryKey: qk.agentCatalog(),
    queryFn: getAgentCatalog,
    staleTime: 60000,
  });

  const { data: defaultProviderValue } = useQuery({
    queryKey: qk.defaultProvider(),
    queryFn: getDefaultProvider,
    staleTime: 60000,
  });

  // Only show agents that are both installed (detected) and enabled in grove.yaml.
  const availableAgents = useMemo(
    () => (agentCatalog as AgentCatalogEntry[]).filter(a => a.detected && a.enabled),
    [agentCatalog],
  );

  // When the modal opens and no agent is selected yet, pre-select the default
  // (only if it's actually available).
  useEffect(() => {
    if (!open || selectedAgent) return;
    if (defaultProviderValue) {
      const isAvailable = (agentCatalog as AgentCatalogEntry[]).some(
        a => a.id === defaultProviderValue && a.detected && a.enabled,
      );
      if (isAvailable) {
        setSelectedAgent(defaultProviderValue);
        setSelectedModel("");
      } else if (availableAgents.length > 0) {
        // Fall back to first available agent
        setSelectedAgent(availableAgents[0].id);
        setSelectedModel("");
      }
    } else if (availableAgents.length > 0) {
      setSelectedAgent(availableAgents[0].id);
      setSelectedModel("");
    }
  }, [open, defaultProviderValue, selectedAgent, agentCatalog, availableAgents]);

  // Reset model when agent changes.
  const currentAgentEntry = useMemo(
    () => availableAgents.find(a => a.id === selectedAgent) ?? null,
    [availableAgents, selectedAgent],
  );

  // Reset model selection when agent changes (start with "Default").
  useEffect(() => {
    setSelectedModel("");
    setCustomModel("");
  }, [currentAgentEntry]);

  // Reset the entire form whenever the modal is dismissed (click-outside, Cancel, or
  // programmatic close). Because `if (!open) return null` only hides the component —
  // it stays mounted and state persists — we need to explicitly clear it on close so
  // the next open always shows a blank form.
  useEffect(() => {
    if (open) return;
    setSelectedProjectId(defaultProjectId);
    setObjective("");
    setSelectedAgent(defaultProviderValue ?? "");
    setSelectedModel("");
    setCustomModel("");
    setPermissionMode("skip_all");
    setDisablePhaseGates(false);
    setInteractive(false);
    setError(null);
    setResumeInfo(null);
    setConnector("");
    setConnectorIssues([]);
    setSelectedIssue(null);
    setLoadingIssues(false);
  }, [open, defaultProjectId, defaultProviderValue]);

  // Also clear context-sensitive fields when the user switches conversation or
  // project while the modal is open (or between opens).
  useEffect(() => {
    setSelectedIssue(null);
    setConnector("");
    setConnectorIssues([]);
    setSelectedProjectId(defaultProjectId);
    setObjective("");
    setError(null);
  }, [conversationId, defaultProjectId]);

  useEffect(() => {
    if (!open) return;
    setSelectedProjectId(defaultProjectId);
  }, [defaultProjectId, open]);

  // Load issues when connector changes
  useEffect(() => {
    if (!connector) {
      setConnectorIssues([]);
      return;
    }
    setLoadingIssues(true);
    setConnectorIssues([]);
    setSelectedIssue(null);
    listProviderIssues(connector, effectiveProjectId)
      .then(setConnectorIssues)
      .catch(() => setConnectorIssues([]))
      .finally(() => setLoadingIssues(false));
  }, [connector, effectiveProjectId]);

  // Auto-fill objective when an issue is selected
  useEffect(() => {
    if (!selectedIssue) return;
    if (selectedIssue.body) {
      setObjective(`${selectedIssue.title}\n\n${selectedIssue.body}`);
    } else {
      setObjective(selectedIssue.title);
    }
  }, [selectedIssue]);

  if (!open) return null;

  const isConnected = (providerId: string): boolean => {
    if (providerId === "grove") return true; // always available
    return connections?.some(c => c.provider === providerId && c.connected) ?? false;
  };

  const handleSubmit = async () => {
    if (!objective.trim()) return;
    setSubmitting(true);
    setError(null);
    try {
      const effectiveObjective = objective.trim();
      const targetProjectId = effectiveProjectId;

      // When resumeInfo is set (Continue Task), lock provider/model to prior run values.
      const agentProvider = resumeInfo ? (resumeInfo.provider ?? null) : (selectedAgent || null);
      // customModel (free-text) takes precedence over the dropdown selection.
      // An empty selectedModel ("") means "Default — let the agent decide" → no --model flag.
      const agentModel = resumeInfo
        ? (resumeInfo.model ?? null)
        : (customModel.trim() || selectedModel || null);

      const result = selectedIssue
        ? await startRunFromIssue(
            selectedIssue.external_id,
            effectiveObjective || null,
            null,
            agentModel,
            targetProjectId,
            agentProvider,
            conversationId,
            disablePhaseGates,
          )
        : await startRun(
            effectiveObjective,
            null,
            agentModel,
            agentProvider,
            conversationId,
            false,
            targetProjectId,
            null,
            null,
            permissionMode === "skip_all" ? null : permissionMode,
            disablePhaseGates,
            interactive,
            resumeInfo ? resumeInfo.provider_session_id : null,
            sessionName,
          );
      onProjectChange?.(targetProjectId ?? null);
      onStarted?.(result.conversation_id);
      // Reset form
      setSelectedProjectId(defaultProjectId);
      setObjective("");
      setSelectedAgent(defaultProviderValue ?? "");
      setSelectedModel("");
      setCustomModel("");
      setPermissionMode("skip_all");
      setDisablePhaseGates(false);
      setInteractive(false);
      setSelectedIssue(null);
      setConnector("");
      setConnectorIssues([]);
      setResumeInfo(null);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  };

  const selectStyle: React.CSSProperties = {
    width: "100%", background: C.base,
    borderRadius: 6,
    padding: "7px 10px", color: C.text2, fontSize: 11,
    appearance: "none", cursor: "pointer", outline: "none",
  };

  const canSubmit =
    !!objective.trim() &&
    !!selectedAgent &&
    !submitting &&
    (conversationId ? true : activeProjects.length === 0 || !!effectiveProjectId);

  const inferredPipeline = inferClassicRunPipeline(objective);
  const inferredBundle = formatRunBundleLabel(inferredPipeline);
  const inferredFlow = formatRunPipelineLabel(inferredPipeline);

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed", inset: 0, zIndex: 100,
        display: "flex", alignItems: "center", justifyContent: "center",
        background: "rgba(0,0,0,0.5)", backdropFilter: "blur(8px)",
        WebkitBackdropFilter: "blur(8px)",
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="New Run"
        onClick={e => e.stopPropagation()}
        style={{
          width: 560, background: C.surface, borderRadius: 10,
          overflow: "hidden",
        }}
      >
        {/* Header */}
        <div style={{
          display: "flex", alignItems: "center", justifyContent: "space-between",
          padding: "16px 20px", background: C.surfaceHover,
        }}>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <div style={{
              width: 28, height: 28, borderRadius: 6,
              background: C.accentDim,
              display: "flex", alignItems: "center", justifyContent: "center",
            }}>
              <span style={{ color: C.accent }}><Bolt size={13} /></span>
            </div>
            <div>
              <div style={{ fontSize: 14, fontWeight: 700, color: C.text1 }}>
                {conversationId ? "New Bundled Run" : "New Run Session"}
              </div>
              <div style={{ fontSize: 10, color: C.text4 }}>
                {conversationId
                  ? "Add an automatic bundled run to the current session"
                  : "Start a new run session with automatic bundled execution"}
              </div>
            </div>
          </div>
          <button
            onClick={onClose}
            aria-label="Close"
            style={{ background: "none", color: C.text4, cursor: "pointer", padding: 4 }}
          >
            <XIcon size={12} />
          </button>
        </div>

          <div style={{ padding: 20, maxHeight: "65vh", overflowY: "auto" }}>
          {conversationId && effectiveProject && (
            <div style={{ marginBottom: 12 }}>
              <div style={lbl}>Project</div>
              <div
                style={{
                  ...selectStyle,
                  display: "flex",
                  alignItems: "center",
                  color: C.text3,
                  cursor: "default",
                  opacity: 0.82,
                }}
              >
                {projectLabel(effectiveProject)}
              </div>
            </div>
          )}

          {!conversationId && activeProjects.length > 0 && (
            <div style={{ marginBottom: 12 }}>
              <div style={lbl}>Project</div>
              <select
                value={effectiveProjectId ?? ""}
                onChange={(e) => {
                  const nextProjectId = e.target.value || null;
                  setSelectedProjectId(nextProjectId);
                  onProjectChange?.(nextProjectId);
                }}
                style={selectStyle}
              >
                <option value="">Select a project…</option>
                {activeProjects.map((project) => (
                  <option key={project.id} value={project.id}>
                    {projectLabel(project)}
                  </option>
                ))}
              </select>
            </div>
          )}

          {/* Objective */}
          <div style={{ marginBottom: 16 }}>
            <div style={lbl}>Objective</div>
            <textarea
              value={objective}
              onChange={e => setObjective(e.target.value)}
              placeholder="What do you want to build, fix, or change?"
              autoFocus
              onKeyDown={e => {
                if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) handleSubmit();
              }}
              style={{
                width: "100%", height: 100, background: C.base,
                borderRadius: 6,
                padding: "12px 14px", color: C.text1, fontSize: 12,
                resize: "none", outline: "none", lineHeight: 1.6, boxSizing: "border-box",
              }}
            />
          </div>

          {/* Quick settings grid */}
          <div style={{ marginBottom: 16 }}>
            <div>
              <div style={lbl}>Execution Flow</div>
              <div
                style={{
                  marginTop: 4,
                  padding: "10px 12px",
                  borderRadius: 8,
                  background: "rgba(255,255,255,0.03)",
                  border: "1px solid rgba(255,255,255,0.06)",
                  display: "flex",
                  flexDirection: "column",
                  gap: 10,
                }}
              >
                <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between", gap: 10 }}>
                  <div style={{ minWidth: 0 }}>
                    <div style={{ fontSize: 10, color: C.text4, textTransform: "uppercase", letterSpacing: "0.05em" }}>
                      Automatic bundle
                    </div>
                    <div style={{ marginTop: 3, fontSize: 11, color: C.text2, lineHeight: 1.4 }}>
                      {inferredBundle}
                    </div>
                  </div>
                  <div
                    style={{
                      flexShrink: 0,
                      padding: "3px 8px",
                      borderRadius: 999,
                      background: "rgba(255,255,255,0.05)",
                      color: C.text4,
                      fontSize: 10,
                      fontWeight: 600,
                      whiteSpace: "nowrap",
                    }}
                  >
                    Judge final check
                  </div>
                </div>

                <div style={{ paddingTop: 10, borderTop: "1px solid rgba(255,255,255,0.06)" }}>
                  <div style={{ fontSize: 11, color: C.text2, fontWeight: 600 }}>
                    Flow
                  </div>
                  <div style={{ marginTop: 2, fontSize: 10, color: C.text4 }}>
                    {inferredFlow}
                  </div>
                  <div style={{ marginTop: 8, fontSize: 10, color: C.text4, lineHeight: 1.45 }}>
                    Classic runs choose the lightweight flow automatically from the objective. No PRD/design phases, no manual pipeline selection.
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* Coding Agent + Model row */}
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12, marginBottom: 12 }}>
            <div>
              <div style={lbl}>
                Coding Agent
                {resumeInfo && <span style={{ color: C.text4, marginLeft: 4, fontSize: 9 }}>locked</span>}
                {!resumeInfo && !selectedAgent && (
                  <span style={{ color: "#EF4444", marginLeft: 4 }}>*</span>
                )}
              </div>
              {resumeInfo ? (
                <div style={{
                  ...selectStyle,
                  color: C.text3,
                  cursor: "default",
                  opacity: 0.7,
                }}>
                  {resumeInfo.provider ?? "default"}
                </div>
              ) : (
                <select
                  value={selectedAgent}
                  onChange={e => {
                    setSelectedAgent(e.target.value);
                    setSelectedModel("");
                  }}
                  style={{
                    ...selectStyle,
                    border: !selectedAgent ? "1px solid rgba(239,68,68,0.5)" : undefined,
                  }}
                >
                  {availableAgents.length === 0 ? (
                    <option value="" disabled>No agents installed</option>
                  ) : (
                    <>
                      {!selectedAgent && <option value="">Select an agent…</option>}
                      {availableAgents.map(agent => (
                        <option key={agent.id} value={agent.id}>
                          {agent.name}
                        </option>
                      ))}
                    </>
                  )}
                </select>
              )}
              {!resumeInfo && availableAgents.length === 0 && (
                <div style={{ fontSize: 10, color: "#EF4444", marginTop: 3 }}>
                  No coding agents detected — install a CLI and enable it in Settings › Editors
                </div>
              )}
              {!resumeInfo && availableAgents.length > 0 && !selectedAgent && (
                <div style={{ fontSize: 10, color: "#EF4444", marginTop: 3 }}>
                  Required — choose an agent to run this task
                </div>
              )}
              {resumeInfo && (
                <div style={{ fontSize: 10, color: C.text4, marginTop: 3 }}>
                  Continuing prior session thread
                </div>
              )}
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <div style={lbl}>
                Model
                {resumeInfo && <span style={{ color: C.text4, marginLeft: 4, fontSize: 9 }}>locked</span>}
              </div>
              {resumeInfo ? (
                <div style={{
                  ...selectStyle,
                  color: C.text3,
                  cursor: "default",
                  opacity: 0.7,
                }}>
                  {resumeInfo.model ?? "default"}
                </div>
              ) : currentAgentEntry && currentAgentEntry.models.length > 0 ? (
                <select
                  value={selectedModel}
                  onChange={e => { setSelectedModel(e.target.value); setCustomModel(""); }}
                  style={selectStyle}
                >
                  <option value="">Default</option>
                  {currentAgentEntry.models.map(m => (
                    <option key={m.id} value={m.id}>{m.name}</option>
                  ))}
                </select>
              ) : (
                <div style={{
                  ...selectStyle,
                  color: C.text4,
                  background: C.base,
                  borderRadius: 6,
                  padding: "7px 10px",
                  fontSize: 11,
                }}>
                  {currentAgentEntry ? "Default" : "Select an agent first"}
                </div>
              )}
              {!resumeInfo && currentAgentEntry && (
                <input
                  type="text"
                  value={customModel}
                  onChange={e => { setCustomModel(e.target.value); setSelectedModel(""); }}
                  placeholder="Custom model ID (optional)"
                  style={{
                    ...selectStyle,
                    padding: "5px 10px",
                    fontSize: 10,
                    color: customModel ? C.text1 : C.text4,
                  }}
                />
              )}
            </div>
          </div>

          {/* Connector + Issue row */}
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12, marginBottom: 16 }}>
            <div>
              <div style={lbl}>Connector</div>
              <select
                value={connector}
                onChange={e => setConnector(e.target.value)}
                style={selectStyle}
              >
                {CONNECTOR_SOURCES.map(src => {
                  const connected = src.value === "" || isConnected(src.value);
                  return (
                    <option
                      key={src.value}
                      value={src.value}
                      disabled={!connected}
                    >
                      {src.label}{!connected ? " (not connected)" : ""}
                    </option>
                  );
                })}
              </select>
            </div>
            <div>
              <div style={lbl}>Issue / Task</div>
              <select
                value={selectedIssue?.external_id ?? ""}
                disabled={!connector || loadingIssues}
                onChange={e => {
                  const id = e.target.value;
                  if (!id) {
                    setSelectedIssue(null);
                    return;
                  }
                  const issue = connectorIssues.find(i => i.external_id === id) ?? null;
                  setSelectedIssue(issue);
                }}
                style={{
                  ...selectStyle,
                  opacity: !connector ? 0.4 : 1,
                }}
              >
                <option value="">
                  {!connector
                    ? "Select a connector first"
                    : loadingIssues
                      ? "Loading..."
                      : connectorIssues.length === 0
                        ? "No open issues"
                        : "Select an issue..."}
                </option>
                {connectorIssues.map(issue => (
                  <option key={`${issue.provider}-${issue.external_id}`} value={issue.external_id}>
                    #{issue.external_id} — {issue.title}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {/* Selected issue preview */}
          {selectedIssue && (
            <div style={{
              marginBottom: 16, padding: "8px 12px", borderRadius: 6,
              background: "rgba(99,102,241,0.06)",
              display: "flex", alignItems: "center", gap: 8,
            }}>
              <span style={{
                fontSize: 10, fontWeight: 700, color: C.accent,
                fontFamily: C.mono,
              }}>
                #{selectedIssue.external_id}
              </span>
              <span style={{ fontSize: 11, color: C.text2, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {selectedIssue.title}
              </span>
              <span style={{
                fontSize: 9, padding: "2px 6px", borderRadius: 6,
                background: "rgba(99,102,241,0.15)", color: "#818CF8",
                fontWeight: 600,
              }}>
                {selectedIssue.provider}
              </span>
              {selectedIssue.assignee && (
                <span style={{ fontSize: 9, color: C.text4 }}>{selectedIssue.assignee}</span>
              )}
              <button
                onClick={() => {
                  setSelectedIssue(null);
                  setObjective("");
                }}
                style={{
                  background: "none", border: "none",
                  color: C.text4, cursor: "pointer", fontSize: 9,
                  padding: "2px 4px",
                }}
              >
                Clear
              </button>
            </div>
          )}

          <div style={{ marginBottom: 16 }}>
            <div style={lbl}>Permission Mode</div>
            <select
              value={permissionMode}
              onChange={e => setPermissionMode(e.target.value)}
              style={selectStyle}
            >
              {PERMISSION_MODES.map(o => (
                <option key={o.value} value={o.value}>{o.label}</option>
              ))}
            </select>
            <div style={{ marginTop: 4, fontSize: 10, color: C.text4 }}>
              Controls tool approval prompts during the run. It does not affect phase-gate approvals.
            </div>
          </div>

          {error && (
            <p style={{ fontSize: 11, color: "#EF4444", marginTop: 8 }}>{error}</p>
          )}

          {/* Conversation context indicator */}
          {conversationId && (
            <div style={{
              marginTop: 10, padding: "6px 10px", borderRadius: 6,
              background: `${C.accent}08`,
              fontSize: 10, color: C.accent, display: "flex", alignItems: "center", gap: 6,
            }}>
              <span style={{ fontWeight: 600 }}>Continuing conversation</span>
              <span style={{ fontFamily: C.mono, fontSize: 9, color: `${C.accent}99` }}>
                {conversationId.slice(0, 12)}...
              </span>
            </div>
          )}
        </div>

        {/* Footer */}
        <div style={{
          display: "flex", alignItems: "center", justifyContent: "space-between",
          padding: "14px 20px", background: C.base,
        }}>
          <span style={{ fontSize: 10, color: C.text4, display: "flex", alignItems: "center", gap: 4 }}>
            <span style={{
              fontSize: 9, background: "rgba(255,255,255,0.04)",
              padding: "2px 5px", borderRadius: 4,
              fontFamily: C.mono,
            }}>
              {"\u2318\u21B5"}
            </span>
            {" "}to start
          </span>
          <div style={{ display: "flex", gap: 6 }}>
            <button
              onClick={onClose}
              style={{
                padding: "7px 16px", borderRadius: 6,
                background: "transparent",
                color: C.text3, fontSize: 11, fontWeight: 500, cursor: "pointer",
              }}
            >
              Cancel
            </button>
            <button
              onClick={handleSubmit}
              disabled={!canSubmit}
              className="btn-accent"
              style={{
                padding: "7px 16px", borderRadius: 6,
                background: C.surfaceRaised, color: "#FFFFFF",
                fontSize: 11, fontWeight: 700, cursor: "pointer",
                display: "flex", alignItems: "center", gap: 5,
                opacity: canSubmit ? 1 : 0.5,
              }}
            >
              {submitting ? (
                <span className="spinner" style={{ width: 12, height: 12, borderWidth: 1.5, borderTopColor: "#fff" }} />
              ) : (
                <Bolt size={11} />
              )}
              {submitting ? "Starting..." : conversationId ? "Start Run" : "Start Session"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
