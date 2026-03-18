import { useEffect, useRef, useState, useCallback } from "react";
import {
  checkConnections,
  getProjectSettings,
  issueCreateNative,
  issueCreateOnProvider,
  issueListProviderProjects,
} from "@/lib/api";
import type { ConnectionStatus, ProjectRow, ProviderProject } from "@/types";

const PROVIDER_OPTIONS = [
  { value: "grove",  label: "Grove only (local)" },
  { value: "github", label: "GitHub Issues" },
  { value: "jira",   label: "Jira" },
  { value: "linear", label: "Linear" },
] as const;

const PRIORITY_CONFIG: Record<string, { color: string; border: string; bg: string; icon: string }> = {
  Critical: { color: "#ef4444", border: "rgba(239,68,68,0.2)", bg: "rgba(239,68,68,0.1)", icon: "!!!" },
  High: { color: "#f97316", border: "rgba(249,115,22,0.2)", bg: "rgba(249,115,22,0.1)", icon: "!!" },
  Medium: { color: "#eab308", border: "rgba(234,179,8,0.15)", bg: "rgba(234,179,8,0.08)", icon: "!" },
  Low: { color: "#6b7280", border: "rgba(107,114,128,0.15)", bg: "rgba(107,114,128,0.08)", icon: "—" },
  None: { color: "#475569", border: "rgba(71,85,105,0.15)", bg: "rgba(71,85,105,0.08)", icon: "·" },
};

function denormalizePriority(priority: string): string | null {
  const map: Record<string, string> = {
    Critical: "urgent",
    High: "high",
    Medium: "medium",
    Low: "low",
  };
  return map[priority] ?? null;
}

function projectLabel(project: ProjectRow): string {
  return project.name || project.root_path.split("/").pop() || project.id;
}

const CloseIcon = () => (
  <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
    <path d="M4.5 4.5L11.5 11.5M11.5 4.5L4.5 11.5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
  </svg>
);

const ChevronDownIcon = () => (
  <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
    <path d="M4 6L8 10L12 6" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

interface NewIssueModalProps {
  open: boolean;
  projectId: string | null;
  projects?: ProjectRow[];
  onClose: () => void;
  onCreated: (projectId: string) => void;
}

export function NewIssueModal({
  open,
  projectId,
  projects = [],
  onClose,
  onCreated,
}: NewIssueModalProps) {
  const activeProjects = projects.filter((project) => project.state === "active");
  const defaultProjectId = projectId ?? activeProjects[0]?.id ?? null;

  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(defaultProjectId);
  const [provider, setProvider] = useState("grove");
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [priority, setPriority] = useState("None");
  const [priorityOpen, setPriorityOpen] = useState(false);
  const [projectKey, setProjectKey] = useState("");
  const [providerProjects, setProviderProjects] = useState<ProviderProject[]>([]);
  const [loadingProjects, setLoadingProjects] = useState(false);
  const [providerProjectDropdownOpen, setProviderProjectDropdownOpen] = useState(false);
  const [providerProjectSearch, setProviderProjectSearch] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [connections, setConnections] = useState<ConnectionStatus[]>([]);
  const overlayRef = useRef<HTMLDivElement>(null);
  const providerProjectSearchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    setSelectedProjectId(defaultProjectId);
    setTitle("");
    setBody("");
    setPriority("None");
    setProvider("grove");
    setProjectKey("");
    setProviderProjectDropdownOpen(false);
    setProviderProjectSearch("");
    setError(null);
  }, [open, defaultProjectId]);

  useEffect(() => {
    checkConnections().then(setConnections).catch(() => setConnections([]));
  }, []);

  useEffect(() => {
    if (!open || !selectedProjectId) return;
    getProjectSettings(selectedProjectId)
      .then((settings) => {
        if (settings.default_provider) {
          setProvider(settings.default_provider);
          if (settings.default_project_key) setProjectKey(settings.default_project_key);
        }
      })
      .catch(() => {});
  }, [selectedProjectId, open]);

  function isConnected(currentProvider: string): boolean {
    if (currentProvider === "grove") return true;
    return connections.find((connection) => connection.provider === currentProvider)?.connected ?? false;
  }

  useEffect(() => {
    if (provider === "grove" || !isConnected(provider)) {
      setProviderProjects([]);
      setProjectKey("");
      setProviderProjectDropdownOpen(false);
      setProviderProjectSearch("");
      return;
    }
    setLoadingProjects(true);
    setProjectKey("");
    setProviderProjectDropdownOpen(false);
    setProviderProjectSearch("");
    issueListProviderProjects(provider)
      .then((items) => {
        setProviderProjects(items);
        if (items.length === 1) setProjectKey(items[0].key ?? items[0].id);
      })
      .catch(() => setProviderProjects([]))
      .finally(() => setLoadingProjects(false));
  }, [provider, connections]);

  // Auto-focus search when the provider project dropdown opens
  useEffect(() => {
    if (providerProjectDropdownOpen) {
      setTimeout(() => providerProjectSearchRef.current?.focus(), 30);
    }
  }, [providerProjectDropdownOpen]);

  const closeProviderProjectDropdown = useCallback(() => {
    setProviderProjectDropdownOpen(false);
    setProviderProjectSearch("");
  }, []);

  const handleSave = async () => {
    if (!title.trim() || saving) return;
    if (!selectedProjectId) {
      setError("Select a project before creating the issue.");
      return;
    }
    if (provider !== "grove" && !isConnected(provider)) {
      setError(`${PROVIDER_OPTIONS.find((option) => option.value === provider)?.label} is not connected. Configure it in Settings → Connections.`);
      return;
    }
    if (provider !== "grove" && !projectKey) {
      setError("Select a project / board before creating.");
      return;
    }

    setSaving(true);
    setError(null);

    try {
      const apiPriority = denormalizePriority(priority);
      if (provider === "grove") {
        await issueCreateNative(selectedProjectId, title.trim(), body.trim() || null, null, apiPriority);
      } else {
        await issueCreateOnProvider(selectedProjectId, provider, projectKey, title.trim(), body.trim() || null, [], apiPriority);
      }
      onCreated(selectedProjectId);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setSaving(false);
    }
  };

  if (!open) return null;

  const canCreate = title.trim().length > 0 && !!selectedProjectId && !saving;
  const showProjectSelector = activeProjects.length > 1;

  return (
    <div
      ref={overlayRef}
      onClick={(event) => {
        if (event.target === overlayRef.current) onClose();
      }}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 1000,
        background: "rgba(0,0,0,0.6)",
        backdropFilter: "blur(8px)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: 20,
      }}
    >
      <div
        style={{
          background: "#0c1222",
          border: "1px solid rgba(51,65,85,0.35)",
          borderRadius: 16,
          width: "100%",
          maxWidth: 500,
          boxShadow: "0 25px 80px rgba(0,0,0,0.5), 0 0 0 1px rgba(51,65,85,0.15)",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "20px 24px 16px",
            borderBottom: "1px solid rgba(51,65,85,0.2)",
          }}
        >
          <div>
            <h2 style={{ fontSize: 17, fontWeight: 700, color: "#f1f5f9", letterSpacing: "-0.02em", margin: 0 }}>
              New Issue
            </h2>
            <div style={{ marginTop: 4, fontSize: 12, color: "#64748b" }}>
              Create a Grove-native or synced tracker issue from one flow.
            </div>
          </div>
          <button
            onClick={onClose}
            style={{
              background: "rgba(51,65,85,0.2)",
              border: "1px solid rgba(51,65,85,0.2)",
              borderRadius: 8,
              width: 30,
              height: 30,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              cursor: "pointer",
              color: "#64748b",
            }}
          >
            <CloseIcon />
          </button>
        </div>

        <div style={{ padding: "20px 24px 24px", display: "flex", flexDirection: "column", gap: 20 }}>
          {showProjectSelector && (
            <div>
              <label style={{ fontSize: 10.5, fontWeight: 700, color: "#475569", letterSpacing: "0.08em", display: "block", marginBottom: 8 }}>
                PROJECT
              </label>
              <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
                {activeProjects.map((project) => {
                  const active = selectedProjectId === project.id;
                  return (
                    <button
                      key={project.id}
                      onClick={() => setSelectedProjectId(project.id)}
                      style={{
                        padding: "7px 12px",
                        borderRadius: 8,
                        fontSize: 12,
                        fontWeight: 600,
                        cursor: "pointer",
                        fontFamily: "inherit",
                        transition: "all .15s",
                        background: active ? "rgba(99,102,241,0.1)" : "rgba(51,65,85,0.15)",
                        color: active ? "#a5b4fc" : "#94a3b8",
                        border: active ? "1px solid rgba(99,102,241,0.3)" : "1px solid rgba(51,65,85,0.25)",
                      }}
                    >
                      {projectLabel(project)}
                    </button>
                  );
                })}
              </div>
            </div>
          )}

          <div>
            <label style={{ fontSize: 10.5, fontWeight: 700, color: "#475569", letterSpacing: "0.08em", display: "block", marginBottom: 8 }}>
              CREATE ON
            </label>
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
              {PROVIDER_OPTIONS.map((option) => {
                const enabled = isConnected(option.value);
                const active = provider === option.value;
                return (
                  <button
                    key={option.value}
                    onClick={() => {
                      if (!enabled) {
                        setError(`${option.label} is not connected. Configure it in Settings → Connections.`);
                        return;
                      }
                      setError(null);
                      setProvider(option.value);
                    }}
                    style={{
                      position: "relative",
                      padding: "7px 14px",
                      borderRadius: 8,
                      fontSize: 12,
                      fontWeight: 600,
                      cursor: enabled ? "pointer" : "not-allowed",
                      fontFamily: "inherit",
                      transition: "all .15s",
                      background: active ? "rgba(34,197,94,0.1)" : "rgba(51,65,85,0.15)",
                      color: active ? "#4ade80" : enabled ? "#94a3b8" : "#334155",
                      border: active ? "1px solid rgba(34,197,94,0.3)" : "1px solid rgba(51,65,85,0.25)",
                      opacity: enabled ? 1 : 0.5,
                    }}
                  >
                    {option.label}
                  </button>
                );
              })}
            </div>
          </div>

          {provider !== "grove" && isConnected(provider) && (
            <div>
              <label style={{ fontSize: 10.5, fontWeight: 700, color: "#475569", letterSpacing: "0.08em", display: "block", marginBottom: 8 }}>
                {provider === "jira" ? "JIRA BOARD" : provider === "linear" ? "LINEAR TEAM" : "GITHUB REPO"} <span style={{ color: "#ef4444" }}>*</span>
              </label>
              {loadingProjects ? (
                <div style={{
                  display: "flex", alignItems: "center", gap: 8, padding: "11px 14px",
                  borderRadius: 10, background: "rgba(2,6,23,0.6)", border: "1px solid rgba(51,65,85,0.3)",
                  fontSize: 12, color: "#475569",
                }}>
                  <span style={{ display: "inline-block", width: 12, height: 12, borderRadius: "50%", border: "2px solid rgba(99,102,241,0.3)", borderTopColor: "#818cf8", animation: "spin 0.8s linear infinite" }} />
                  Loading…
                </div>
              ) : (() => {
                const selectedPP = providerProjects.find(pp => (pp.key ?? pp.id) === projectKey);
                const filtered = providerProjects.filter(pp => {
                  const q = providerProjectSearch.toLowerCase();
                  return !q || pp.name.toLowerCase().includes(q) || (pp.key ?? "").toLowerCase().includes(q) || pp.id.toLowerCase().includes(q);
                });
                return (
                  <div style={{ position: "relative" }}>
                    {/* Trigger */}
                    <button
                      onClick={() => setProviderProjectDropdownOpen(v => !v)}
                      style={{
                        display: "flex", alignItems: "center", gap: 8, width: "100%",
                        padding: "11px 14px", borderRadius: 10, fontSize: 13,
                        fontFamily: "inherit", background: "rgba(2,6,23,0.6)",
                        border: providerProjectDropdownOpen
                          ? "1px solid rgba(99,102,241,0.5)"
                          : "1px solid rgba(51,65,85,0.3)",
                        color: selectedPP ? "#e2e8f0" : "#475569",
                        cursor: "pointer", textAlign: "left",
                        boxShadow: providerProjectDropdownOpen ? "0 0 0 3px rgba(99,102,241,0.08)" : undefined,
                        transition: "all .15s",
                      }}
                    >
                      {selectedPP ? (
                        <>
                          <span style={{ flex: 1, fontWeight: 500 }}>{selectedPP.name}</span>
                          {selectedPP.key && (
                            <span style={{
                              fontSize: 10, color: "#818cf8", fontFamily: "monospace",
                              background: "rgba(99,102,241,0.1)", padding: "2px 7px", borderRadius: 5,
                              flexShrink: 0,
                            }}>{selectedPP.key}</span>
                          )}
                        </>
                      ) : (
                        <span style={{ flex: 1 }}>
                          {providerProjects.length === 0 ? "No projects found" : "Select a project…"}
                        </span>
                      )}
                      <svg width="12" height="12" viewBox="0 0 16 16" fill="none" style={{ flexShrink: 0, color: "#475569", transform: providerProjectDropdownOpen ? "rotate(180deg)" : undefined, transition: "transform .15s" }}>
                        <path d="M4 6L8 10L12 6" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
                      </svg>
                    </button>

                    {/* Dropdown panel */}
                    {providerProjectDropdownOpen && providerProjects.length > 0 && (
                      <>
                        <div onClick={closeProviderProjectDropdown} style={{ position: "fixed", inset: 0, zIndex: 1050 }} />
                        <div style={{
                          position: "absolute", top: "calc(100% + 5px)", left: 0, right: 0, zIndex: 1100,
                          background: "#080f1e", border: "1px solid rgba(99,102,241,0.25)", borderRadius: 12,
                          boxShadow: "0 20px 60px rgba(0,0,0,0.6), 0 0 0 1px rgba(99,102,241,0.08)",
                          overflow: "hidden", display: "flex", flexDirection: "column",
                        }}>
                          {/* Search */}
                          <div style={{ padding: "10px 10px 8px", borderBottom: "1px solid rgba(51,65,85,0.2)", background: "rgba(15,23,42,0.5)" }}>
                            <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "6px 10px", borderRadius: 8, background: "rgba(2,6,23,0.6)", border: "1px solid rgba(51,65,85,0.25)" }}>
                              <svg width="12" height="12" viewBox="0 0 16 16" fill="none" style={{ flexShrink: 0, color: "#334155" }}>
                                <circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.5" />
                                <path d="M10.5 10.5L13 13" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
                              </svg>
                              <input
                                ref={providerProjectSearchRef}
                                value={providerProjectSearch}
                                onChange={e => setProviderProjectSearch(e.target.value)}
                                placeholder={`Search ${providerProjects.length} projects…`}
                                style={{
                                  flex: 1, background: "none", border: "none", outline: "none",
                                  color: "#e2e8f0", fontSize: 12, fontFamily: "inherit",
                                }}
                              />
                              {providerProjectSearch && (
                                <button onClick={() => setProviderProjectSearch("")} style={{ background: "none", border: "none", cursor: "pointer", color: "#334155", padding: 0, lineHeight: 1, fontSize: 14 }}>×</button>
                              )}
                            </div>
                          </div>

                          {/* List */}
                          <div style={{ overflowY: "auto", maxHeight: 224 }}>
                            {filtered.length === 0 ? (
                              <div style={{ padding: "14px 16px", fontSize: 12, color: "#475569", textAlign: "center" }}>
                                No projects match "{providerProjectSearch}"
                              </div>
                            ) : filtered.map(pp => {
                              const key = pp.key ?? pp.id;
                              const active = projectKey === key;
                              return (
                                <button
                                  key={key}
                                  onClick={() => { setProjectKey(key); closeProviderProjectDropdown(); }}
                                  style={{
                                    display: "flex", alignItems: "center", gap: 10, width: "100%",
                                    padding: "10px 14px", border: "none", cursor: "pointer",
                                    fontFamily: "inherit", textAlign: "left",
                                    background: active ? "rgba(99,102,241,0.12)" : "transparent",
                                    borderBottom: "1px solid rgba(51,65,85,0.08)",
                                    transition: "background .1s",
                                  }}
                                  onMouseEnter={e => { if (!active) e.currentTarget.style.background = "rgba(51,65,85,0.18)"; }}
                                  onMouseLeave={e => { if (!active) e.currentTarget.style.background = "transparent"; }}
                                >
                                  {/* Checkmark or bullet */}
                                  <span style={{ width: 16, flexShrink: 0, display: "flex", alignItems: "center", justifyContent: "center" }}>
                                    {active ? (
                                      <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                                        <path d="M3 8L6.5 11.5L13 4.5" stroke="#818cf8" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
                                      </svg>
                                    ) : (
                                      <span style={{ width: 5, height: 5, borderRadius: "50%", background: "rgba(51,65,85,0.5)" }} />
                                    )}
                                  </span>

                                  <span style={{ flex: 1, minWidth: 0 }}>
                                    <span style={{ fontSize: 13, fontWeight: active ? 600 : 400, color: active ? "#a5b4fc" : "#cbd5e1", display: "block", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                                      {pp.name}
                                    </span>
                                    {pp.id !== pp.name && (
                                      <span style={{ fontSize: 10, color: "#334155", marginTop: 1, display: "block" }}>{pp.id}</span>
                                    )}
                                  </span>

                                  {pp.key && (
                                    <span style={{
                                      fontSize: 10, fontFamily: "monospace", flexShrink: 0,
                                      padding: "2px 7px", borderRadius: 5,
                                      background: active ? "rgba(99,102,241,0.18)" : "rgba(51,65,85,0.2)",
                                      color: active ? "#818cf8" : "#475569",
                                    }}>{pp.key}</span>
                                  )}
                                </button>
                              );
                            })}
                          </div>

                          {/* Footer count */}
                          {filtered.length > 0 && (
                            <div style={{ padding: "6px 14px", borderTop: "1px solid rgba(51,65,85,0.15)", background: "rgba(15,23,42,0.4)" }}>
                              <span style={{ fontSize: 10, color: "#334155" }}>
                                {filtered.length} of {providerProjects.length} project{providerProjects.length !== 1 ? "s" : ""}
                              </span>
                            </div>
                          )}
                        </div>
                      </>
                    )}
                  </div>
                );
              })()}
            </div>
          )}

          <div>
            <label style={{ fontSize: 10.5, fontWeight: 700, color: "#475569", letterSpacing: "0.08em", display: "block", marginBottom: 8 }}>
              TITLE <span style={{ color: "#ef4444" }}>*</span>
            </label>
            <input
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              placeholder="Issue title..."
              style={{
                width: "100%",
                padding: "11px 14px",
                borderRadius: 10,
                fontSize: 14,
                fontFamily: "inherit",
                background: "rgba(2,6,23,0.6)",
                border: "1px solid rgba(51,65,85,0.3)",
                color: "#e2e8f0",
                outline: "none",
                boxSizing: "border-box",
              }}
            />
          </div>

          <div>
            <label style={{ fontSize: 10.5, fontWeight: 700, color: "#475569", letterSpacing: "0.08em", display: "block", marginBottom: 8 }}>
              DESCRIPTION
            </label>
            <textarea
              value={body}
              onChange={(event) => setBody(event.target.value)}
              placeholder="Optional description..."
              rows={4}
              style={{
                width: "100%",
                padding: "11px 14px",
                borderRadius: 10,
                fontSize: 13.5,
                fontFamily: "inherit",
                background: "rgba(2,6,23,0.6)",
                border: "1px solid rgba(51,65,85,0.3)",
                color: "#e2e8f0",
                outline: "none",
                resize: "vertical",
                lineHeight: 1.5,
                minHeight: 80,
                boxSizing: "border-box",
              }}
            />
          </div>

          <div style={{ position: "relative" }}>
            <label style={{ fontSize: 10.5, fontWeight: 700, color: "#475569", letterSpacing: "0.08em", display: "block", marginBottom: 8 }}>
              PRIORITY
            </label>
            <button
              onClick={() => setPriorityOpen((value) => !value)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "9px 14px",
                borderRadius: 10,
                fontSize: 13,
                fontWeight: 500,
                fontFamily: "inherit",
                background: "rgba(2,6,23,0.6)",
                border: "1px solid rgba(51,65,85,0.3)",
                color: "#94a3b8",
                cursor: "pointer",
                minWidth: 160,
              }}
            >
              {(() => {
                const current = PRIORITY_CONFIG[priority] ?? PRIORITY_CONFIG.None;
                return (
                  <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
                    <span style={{ color: current.color, fontWeight: 700, fontSize: 11 }}>{current.icon}</span>
                    {priority}
                  </span>
                );
              })()}
              <span style={{ marginLeft: "auto" }}>
                <ChevronDownIcon />
              </span>
            </button>
            {priorityOpen && (
              <div
                style={{
                  position: "absolute",
                  top: "100%",
                  left: 0,
                  marginTop: 4,
                  zIndex: 10,
                  background: "#0f172a",
                  border: "1px solid rgba(51,65,85,0.35)",
                  borderRadius: 10,
                  boxShadow: "0 12px 40px rgba(0,0,0,0.4)",
                  overflow: "hidden",
                  minWidth: 180,
                }}
              >
                {["Critical", "High", "Medium", "Low", "None"].map((value) => {
                  const current = PRIORITY_CONFIG[value];
                  return (
                    <button
                      key={value}
                      onClick={() => {
                        setPriority(value);
                        setPriorityOpen(false);
                      }}
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 8,
                        width: "100%",
                        padding: "9px 14px",
                        background: priority === value ? "rgba(51,65,85,0.2)" : "transparent",
                        border: "none",
                        color: "#cbd5e1",
                        fontSize: 13,
                        cursor: "pointer",
                        fontFamily: "inherit",
                      }}
                    >
                      <span style={{ color: current.color, fontWeight: 700, fontSize: 11, width: 18 }}>{current.icon}</span>
                      {value}
                    </button>
                  );
                })}
              </div>
            )}
          </div>

          {error && (
            <div
              style={{
                fontSize: 12,
                color: "#ef4444",
                background: "rgba(239,68,68,0.08)",
                border: "1px solid rgba(239,68,68,0.15)",
                borderRadius: 8,
                padding: "8px 12px",
              }}
            >
              {error}
            </div>
          )}
        </div>

        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 8,
            padding: "16px 24px",
            borderTop: "1px solid rgba(51,65,85,0.2)",
            background: "rgba(2,6,23,0.3)",
          }}
        >
          <button
            onClick={onClose}
            style={{
              padding: "9px 20px",
              borderRadius: 9,
              fontSize: 13,
              fontWeight: 600,
              background: "rgba(51,65,85,0.2)",
              border: "1px solid rgba(51,65,85,0.3)",
              color: "#94a3b8",
              cursor: "pointer",
              fontFamily: "inherit",
            }}
          >
            Cancel
          </button>
          <button
            onClick={() => void handleSave()}
            disabled={!canCreate}
            style={{
              padding: "9px 24px",
              borderRadius: 9,
              fontSize: 13,
              fontWeight: 700,
              background: canCreate ? "linear-gradient(135deg,#31B97B,#269962)" : "rgba(51,65,85,0.2)",
              border: canCreate ? "1px solid rgba(49,185,123,0.3)" : "1px solid rgba(51,65,85,0.2)",
              color: canCreate ? "#fff" : "#334155",
              cursor: canCreate ? "pointer" : "not-allowed",
              fontFamily: "inherit",
            }}
          >
            {saving ? "Creating…" : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}
