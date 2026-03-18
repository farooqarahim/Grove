import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Bolt, Check, Folder, GitBranch, Terminal, XIcon } from "@/components/ui/icons";
import { createConversation, getAgentCatalog, getDefaultProvider } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { C } from "@/lib/theme";
import type { AgentCatalogEntry, ProjectRow } from "@/types";

interface NewCliConversationModalProps {
  open: boolean;
  onClose: () => void;
  projectId: string | null;
  projects?: ProjectRow[];
  sessionName?: string | null;
  onProjectChange?: (projectId: string | null) => void;
  onCreated?: (conversationId: string) => void;
}

function isInternalWorkspaceProject(project: { root_path: string }): boolean {
  return project.root_path.includes("/.grove/workspaces/");
}

function projectLabel(project: ProjectRow): string {
  return project.name || project.root_path.split("/").pop() || project.id;
}

function titleCase(value: string): string {
  return value
    .replaceAll("_", " ")
    .replaceAll("-", " ")
    .replace(/\b\w/g, (match) => match.toUpperCase());
}

export function NewCliConversationModal({
  open,
  onClose,
  projectId,
  projects = [],
  sessionName = null,
  onProjectChange,
  onCreated,
}: NewCliConversationModalProps) {
  const queryClient = useQueryClient();
  const activeProjects = useMemo(() => {
    const active = projects.filter((project) => project.state === "active");
    const preferred = active.filter(
      (project) => !isInternalWorkspaceProject(project) && project.source_kind !== "ssh",
    );
    return preferred.length > 0 ? preferred : active.filter((project) => project.source_kind !== "ssh");
  }, [projects]);

  const defaultProjectId =
    (projectId && activeProjects.some((project) => project.id === projectId) ? projectId : null)
    ?? activeProjects[0]?.id
    ?? null;

  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(defaultProjectId);
  const [selectedAgent, setSelectedAgent] = useState("");
  const [selectedModel, setSelectedModel] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

  const availableAgents = useMemo(
    () => (agentCatalog as AgentCatalogEntry[]).filter((agent) => agent.detected && agent.enabled),
    [agentCatalog],
  );

  const currentAgentEntry = useMemo(
    () => availableAgents.find((agent) => agent.id === selectedAgent) ?? null,
    [availableAgents, selectedAgent],
  );

  const selectedProject = useMemo(
    () => activeProjects.find((project) => project.id === selectedProjectId) ?? null,
    [activeProjects, selectedProjectId],
  );

  useEffect(() => {
    if (!open) return;
    setSelectedProjectId(defaultProjectId);
  }, [defaultProjectId, open]);

  useEffect(() => {
    if (!open) return;
    if (selectedAgent) return;
    if (defaultProviderValue) {
      const provider = availableAgents.find((agent) => agent.id === defaultProviderValue);
      if (provider) {
        setSelectedAgent(provider.id);
        return;
      }
    }
    if (availableAgents.length > 0) {
      setSelectedAgent(availableAgents[0].id);
    }
  }, [availableAgents, defaultProviderValue, open, selectedAgent]);

  useEffect(() => {
    if (!open) {
      setSelectedProjectId(defaultProjectId);
      setSelectedAgent("");
      setSelectedModel("");
      setSubmitting(false);
      setError(null);
      return;
    }
    setError(null);
  }, [defaultProjectId, open]);

  useEffect(() => {
    const defaultModel = currentAgentEntry?.models.find((model) => model.is_default)?.id ?? "";
    setSelectedModel(defaultModel);
  }, [currentAgentEntry?.id, currentAgentEntry?.models]);

  if (!open) return null;

  const canSubmit = !!selectedProjectId && !!selectedAgent && !submitting;

  const handleSubmit = async () => {
    if (!canSubmit) return;
    setSubmitting(true);
    setError(null);
    try {
      const result = await createConversation(
            selectedProjectId!,
            sessionName,
            "cli",
            selectedAgent,
            selectedModel || null,
          );
      onProjectChange?.(selectedProjectId);
      onCreated?.(result.conversation_id);
      void queryClient.invalidateQueries({ queryKey: ["conversations", selectedProjectId] });
      void queryClient.invalidateQueries({ queryKey: qk.conversation(result.conversation_id) });
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  };

  const selectStyle: CSSProperties = {
    width: "100%",
    background: C.base,
    borderRadius: 12,
    padding: "10px 12px",
    color: C.text2,
    fontSize: 12,
    appearance: "none",
    cursor: "pointer",
    outline: "none",
    border: `1px solid ${C.border}`,
    boxShadow: "inset 0 1px 0 rgba(255,255,255,0.03)",
  };

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 120,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.5)",
        backdropFilter: "blur(8px)",
        WebkitBackdropFilter: "blur(8px)",
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="New CLI conversation"
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 760,
          maxWidth: "calc(100vw - 40px)",
          background: C.surface,
          borderRadius: 20,
          overflow: "hidden",
          border: `1px solid ${C.border}`,
          boxShadow: "0 28px 80px rgba(0,0,0,0.42)",
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "20px 24px",
            background: "linear-gradient(135deg, rgba(59,130,246,0.18) 0%, rgba(49,185,123,0.12) 100%)",
            borderBottom: `1px solid ${C.border}`,
          }}
        >
          <div>
            <div style={{ display: "inline-flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
              <span
                style={{
                  width: 34,
                  height: 34,
                  borderRadius: 10,
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                  background: "rgba(15,23,42,0.36)",
                  color: "#8CC8FF",
                }}
              >
                <Terminal size={16} />
              </span>
              <span
                style={{
                  padding: "4px 9px",
                  borderRadius: 999,
                  fontSize: 10,
                  fontWeight: 700,
                  letterSpacing: "0.06em",
                  textTransform: "uppercase",
                  background: "rgba(59,130,246,0.14)",
                  color: "#9CCBFF",
                }}
              >
                CLI Session
              </span>
            </div>
            <div style={{ fontSize: 18, fontWeight: 700, color: C.text1 }}>
              New CLI Session
            </div>
            <div style={{ fontSize: 12, color: "rgba(255,255,255,0.76)", marginTop: 6, maxWidth: 520, lineHeight: 1.5 }}>
              This creates the conversation worktree immediately, binds the provider permanently, and opens the real interactive CLI in the center column.
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

        <div style={{ padding: 24, display: "grid", gridTemplateColumns: "1.05fr 1.35fr", gap: 18 }}>
          <div
            style={{
              borderRadius: 18,
              border: `1px solid ${C.border}`,
              background: "linear-gradient(180deg, rgba(36,39,47,0.92) 0%, rgba(21,23,30,0.96) 100%)",
              padding: 18,
              display: "flex",
              flexDirection: "column",
              gap: 16,
            }}
          >
            <div>
              <div style={{ fontSize: 10, fontWeight: 700, color: C.text4, textTransform: "uppercase", letterSpacing: "0.08em", marginBottom: 8 }}>
                Session Preview
              </div>
              <div style={{ fontSize: 18, fontWeight: 700, color: C.text1 }}>
                {sessionName ?? "Untitled CLI Session"}
              </div>
              <div style={{ marginTop: 6, color: "rgba(255,255,255,0.66)", fontSize: 12, lineHeight: 1.5 }}>
                Provider and model are fixed once created. The worktree is ready before the first prompt.
              </div>
            </div>

            <div
              style={{
                borderRadius: 16,
                border: `1px solid ${C.border}`,
                background: "linear-gradient(145deg, rgba(12,16,24,0.9) 0%, rgba(21,23,30,0.98) 100%)",
                overflow: "hidden",
              }}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  padding: "10px 12px",
                  borderBottom: `1px solid ${C.border}`,
                }}
              >
                <span style={{ width: 8, height: 8, borderRadius: "50%", background: "#FB7185" }} />
                <span style={{ width: 8, height: 8, borderRadius: "50%", background: "#FBBF24" }} />
                <span style={{ width: 8, height: 8, borderRadius: "50%", background: "#34D399" }} />
                <span style={{ marginLeft: 8, color: "rgba(255,255,255,0.55)", fontSize: 11, fontFamily: C.mono }}>
                  live-agent-terminal
                </span>
              </div>
              <div style={{ padding: 14, display: "flex", flexDirection: "column", gap: 10 }}>
                {[
                  { icon: <Folder size={13} />, label: "Project", value: selectedProject ? projectLabel(selectedProject) : "Select a project" },
                  { icon: <Terminal size={13} />, label: "CLI", value: currentAgentEntry?.name ?? "Select a provider" },
                  { icon: <GitBranch size={13} />, label: "Model", value: selectedModel ? titleCase(selectedModel) : "Default model" },
                ].map((item) => (
                  <div
                    key={item.label}
                    style={{
                      display: "grid",
                      gridTemplateColumns: "18px 58px minmax(0, 1fr)",
                      gap: 10,
                      alignItems: "center",
                      fontSize: 11,
                      color: "rgba(255,255,255,0.78)",
                    }}
                  >
                    <span style={{ color: C.blue }}>{item.icon}</span>
                    <span style={{ color: "rgba(255,255,255,0.5)", textTransform: "uppercase", letterSpacing: "0.06em", fontSize: 10 }}>
                      {item.label}
                    </span>
                    <span style={{ fontFamily: C.mono, color: C.text1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                      {item.value}
                    </span>
                  </div>
                ))}
              </div>
            </div>

            <div style={{ display: "grid", gap: 10 }}>
              {[
                "Creates the conversation worktree immediately.",
                "Launches the real CLI in the middle column.",
                "Skips runs, queueing, and run history for this session.",
              ].map((line) => (
                <div
                  key={line}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 10,
                    color: "rgba(255,255,255,0.76)",
                    fontSize: 12,
                  }}
                >
                  <span
                    style={{
                      width: 20,
                      height: 20,
                      borderRadius: "50%",
                      display: "inline-flex",
                      alignItems: "center",
                      justifyContent: "center",
                      background: C.accentDim,
                      color: C.accent,
                      flexShrink: 0,
                    }}
                  >
                    <Check size={10} />
                  </span>
                  <span>{line}</span>
                </div>
              ))}
            </div>
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
            <div>
              <div style={{ fontSize: 10, fontWeight: 700, color: C.text4, textTransform: "uppercase", letterSpacing: "0.08em", marginBottom: 6 }}>
                Project
              </div>
              <select
                value={selectedProjectId ?? ""}
                onChange={(e) => setSelectedProjectId(e.target.value || null)}
                style={selectStyle}
              >
                <option value="" disabled>Select project</option>
                {activeProjects.map((project) => (
                  <option key={project.id} value={project.id}>
                    {projectLabel(project)}
                  </option>
                ))}
              </select>
            </div>

            <div>
              <div style={{ fontSize: 10, fontWeight: 700, color: C.text4, textTransform: "uppercase", letterSpacing: "0.08em", marginBottom: 8 }}>
                CLI Provider
              </div>
              <div style={{ display: "grid", gap: 10 }}>
                {availableAgents.map((agent) => {
                  const isSelected = selectedAgent === agent.id;
                  return (
                    <button
                      key={agent.id}
                      type="button"
                      onClick={() => setSelectedAgent(agent.id)}
                      style={{
                        textAlign: "left",
                        borderRadius: 14,
                        border: isSelected ? `1px solid ${C.blue}` : `1px solid ${C.border}`,
                        background: isSelected
                          ? "linear-gradient(135deg, rgba(59,130,246,0.18) 0%, rgba(21,23,30,0.92) 100%)"
                          : "linear-gradient(180deg, rgba(36,39,47,0.84) 0%, rgba(21,23,30,0.92) 100%)",
                        padding: 14,
                        cursor: "pointer",
                        boxShadow: isSelected ? `0 0 0 1px ${C.blueDim} inset` : "none",
                      }}
                    >
                      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
                        <div>
                          <div style={{ fontSize: 13, fontWeight: 700, color: C.text1 }}>
                            {agent.name}
                          </div>
                          <div style={{ marginTop: 5, color: "rgba(255,255,255,0.62)", fontSize: 11, fontFamily: C.mono }}>
                            {agent.cli}
                            {agent.model_flag ? ` ${agent.model_flag} <model>` : " uses CLI default model selection"}
                          </div>
                        </div>
                        <span
                          style={{
                            width: 24,
                            height: 24,
                            borderRadius: "50%",
                            display: "inline-flex",
                            alignItems: "center",
                            justifyContent: "center",
                            background: isSelected ? C.blue : "transparent",
                            border: isSelected ? "none" : `1px solid ${C.borderHover}`,
                            color: isSelected ? "#fff" : "transparent",
                            flexShrink: 0,
                          }}
                        >
                          <Check size={10} />
                        </span>
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>

            <div>
              <div style={{ fontSize: 10, fontWeight: 700, color: C.text4, textTransform: "uppercase", letterSpacing: "0.08em", marginBottom: 8 }}>
                Model
              </div>
              {currentAgentEntry?.models.length ? (
                <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
                  <button
                    type="button"
                    onClick={() => setSelectedModel("")}
                    style={{
                      padding: "9px 12px",
                      borderRadius: 999,
                      border: !selectedModel ? `1px solid ${C.accent}` : `1px solid ${C.border}`,
                      background: !selectedModel ? C.accentDim : C.base,
                      color: !selectedModel ? C.accent : C.text3,
                      cursor: "pointer",
                      fontSize: 11,
                      fontWeight: 700,
                    }}
                  >
                    CLI default
                  </button>
                  {currentAgentEntry.models.map((model) => {
                    const isSelected = selectedModel === model.id;
                    return (
                      <button
                        key={model.id}
                        type="button"
                        onClick={() => setSelectedModel(model.id)}
                        style={{
                          padding: "9px 12px",
                          borderRadius: 999,
                          border: isSelected ? `1px solid ${C.blue}` : `1px solid ${C.border}`,
                          background: isSelected ? C.blueDim : C.base,
                          color: isSelected ? C.blue : C.text3,
                          cursor: "pointer",
                          fontSize: 11,
                          fontWeight: 700,
                        }}
                        title={model.description}
                      >
                        {model.name}
                        {model.is_default ? " · default" : ""}
                      </button>
                    );
                  })}
                </div>
              ) : (
                <div
                  style={{
                    borderRadius: 12,
                    border: `1px solid ${C.border}`,
                    background: C.base,
                    padding: "12px 14px",
                    color: "rgba(255,255,255,0.62)",
                    fontSize: 12,
                  }}
                >
                  This provider does not expose named models in Grove. The CLI will use its own default.
                </div>
              )}
              <div style={{ marginTop: 8, display: "flex", alignItems: "center", gap: 8, fontSize: 11, color: "rgba(255,255,255,0.58)" }}>
                <Bolt size={12} />
                <span>
                  {currentAgentEntry?.model_flag
                    ? `Grove passes the model with ${currentAgentEntry.model_flag}.`
                    : "No model flag is passed for this provider."}
                </span>
              </div>
            </div>

            {error && (
              <div
                style={{
                  fontSize: 11,
                  color: "#FCA5A5",
                  background: "rgba(127,29,29,0.24)",
                  border: "1px solid rgba(239,68,68,0.35)",
                  borderRadius: 12,
                  padding: "10px 12px",
                }}
              >
                {error}
              </div>
            )}
          </div>
        </div>

        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 12,
            padding: "16px 24px",
            background: C.base,
            borderTop: `1px solid ${C.border}`,
          }}
        >
          <div style={{ color: "rgba(255,255,255,0.58)", fontSize: 11 }}>
            Session type is locked after creation.
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <button
              onClick={onClose}
              style={{
                padding: "8px 16px",
                borderRadius: 10,
                background: "transparent",
                color: C.text3,
                fontSize: 11,
                fontWeight: 600,
                cursor: "pointer",
              }}
            >
              Cancel
            </button>
            <button
              onClick={handleSubmit}
              disabled={!canSubmit}
              className="btn-accent"
              style={{
                padding: "9px 18px",
                borderRadius: 10,
                background: "linear-gradient(135deg, #3B82F6 0%, #2563EB 100%)",
                color: "#fff",
                fontSize: 11,
                fontWeight: 800,
                cursor: "pointer",
                opacity: canSubmit ? 1 : 0.5,
                boxShadow: canSubmit ? "0 12px 28px rgba(37,99,235,0.28)" : "none",
              }}
            >
              {submitting ? "Creating..." : "Create CLI Session"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
