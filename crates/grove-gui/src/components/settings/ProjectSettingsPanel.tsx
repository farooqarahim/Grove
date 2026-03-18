import { useState, useEffect, useRef, useCallback } from "react";
import {
  getProjectSettings,
  updateProjectSettings,
  issueListProviderProjects,
  checkConnections,
  listProviderStatuses,
} from "@/lib/api";
import { C, lbl } from "@/lib/theme";
import { Check, ChevronDown } from "@/components/ui/icons";
import type { ProjectSettings, ConnectionStatus, ProviderProject, WorkflowStepConfig, IssueTrackerStatus } from "@/types";

// ── Constants ─────────────────────────────────────────────────────────────────

const PIPELINES = [
  { value: "",               label: "Inherit from workspace" },
  { value: "auto",           label: "Auto — detect from objective" },
  { value: "standard",       label: "Standard — Architect → Builder → Reviewer → Tester" },
  { value: "quick",          label: "Quick — Builder → Tester" },
  { value: "instant",        label: "Instant — Single Builder" },
  { value: "bugfix",         label: "Bugfix — Debugger → Tester → Reviewer" },
  { value: "secure",         label: "Secure — + Security gates" },
  { value: "refactor",       label: "Refactor — Architect → Refactorer → Reviewer" },
  { value: "test-coverage",  label: "Test Coverage — 3× Tester parallel" },
  { value: "fullstack",      label: "Fullstack — Parallel Builders + Testers" },
  { value: "docs",           label: "Docs — Documenter only" },
];

const PERMISSIONS = [
  { value: "",                label: "Inherit from workspace" },
  { value: "skip_all",        label: "Auto-approve all tools" },
  { value: "human_gate",      label: "Ask human for each tool" },
  { value: "autonomous_gate", label: "AI gatekeeper per tool" },
];

const PARALLEL_OPTS = ["", "1", "2", "3", "4", "6", "8", "12"];

interface ProviderConfig {
  id: string;
  label: string;
  color: string;
  keyField: keyof ProjectSettings;
  keyLabel: string;
  keyPlaceholder: string;
}

const PROVIDER_CONFIGS: ProviderConfig[] = [
  { id: "github", label: "GitHub",       color: "#6E7681", keyField: "github_project_key", keyLabel: "Repository",  keyPlaceholder: "owner/repo" },
  { id: "linear", label: "Linear",       color: "#5E6AD2", keyField: "linear_project_key", keyLabel: "Team",        keyPlaceholder: "e.g. ENG" },
  { id: "jira",   label: "Jira",         color: "#0052CC", keyField: "jira_project_key",   keyLabel: "Project Key", keyPlaceholder: "e.g. PROJ" },
  { id: "grove",  label: "Grove Issues", color: C.accent,  keyField: "github_project_key", keyLabel: "",            keyPlaceholder: "" },
];

// ── Props ─────────────────────────────────────────────────────────────────────

export interface ProjectSettingsPanelProps {
  projectId: string;
  projectName?: string | null;
  rootPath?: string;
  onSaved?: () => void;
  compact?: boolean;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function emptyWorkflow(): WorkflowStepConfig {
  return {
    on_start: null,
    on_success: null,
    on_failure: null,
    comment_on_failure: false,
    comment_on_success: false,
  };
}

function emptySettings(): ProjectSettings {
  return {
    default_provider: null,
    default_project_key: null,
    github_project_key: null,
    linear_project_key: null,
    jira_project_key: null,
    github_workflow: null,
    linear_workflow: null,
    jira_workflow: null,
    grove_workflow: null,
    max_parallel_agents: null,
    default_pipeline: null,
    default_permission_mode: null,
    issue_board: null,
  };
}

// ── Component ─────────────────────────────────────────────────────────────────

export function ProjectSettingsPanel({
  projectId,
  projectName,
  rootPath,
  onSaved,
  compact = false,
}: ProjectSettingsPanelProps) {
  const [settings, setSettings] = useState<ProjectSettings | null>(null);
  const [connections, setConnections] = useState<ConnectionStatus[]>([]);
  const [projects, setProjects] = useState<Record<string, ProviderProject[]>>({
    github: [], linear: [], jira: [],
  });
  const [loading, setLoading] = useState<Record<string, boolean>>({
    github: false, linear: false, jira: false,
  });
  const [statuses, setStatuses] = useState<Record<string, IssueTrackerStatus[]>>({
    github: [], linear: [], jira: [], grove: [],
  });
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getProjectSettings(projectId)
      .then(setSettings)
      .catch(() => setSettings(emptySettings()));
    checkConnections()
      .then(setConnections)
      .catch(() => {});
  }, [projectId]);

  const isConnected = useCallback((prov: string) => {
    if (prov === "grove") return true;
    return connections.some((c) => c.provider === prov && c.connected);
  }, [connections]);

  // Fetch boards for all connected external providers when connections load
  useEffect(() => {
    if (!connections.length) return;
    const providers = ["github", "linear", "jira"];
    providers.forEach((prov) => {
      if (!isConnected(prov)) return;
      setLoading((prev) => ({ ...prev, [prov]: true }));
      issueListProviderProjects(prov)
        .then((list) => setProjects((prev) => ({ ...prev, [prov]: list })))
        .catch(() => {})
        .finally(() => setLoading((prev) => ({ ...prev, [prov]: false })));
    });
    // Fetch workflow statuses for all providers (including grove)
    const allProviders = ["github", "linear", "jira", "grove"];
    allProviders.forEach((prov) => {
      if (prov !== "grove" && !isConnected(prov)) return;
      listProviderStatuses(prov, projectId)
        .then((list) => setStatuses((prev) => ({ ...prev, [prov]: list })))
        .catch(() => {});
    });
  }, [connections, isConnected, projectId]);

  const setField = <K extends keyof ProjectSettings>(key: K, value: ProjectSettings[K]) =>
    setSettings((prev) => prev ? { ...prev, [key]: value } : prev);

  const workflowKey = (prov: string): keyof ProjectSettings => {
    if (prov === "github") return "github_workflow";
    if (prov === "linear") return "linear_workflow";
    if (prov === "jira") return "jira_workflow";
    return "grove_workflow";
  };

  const setWorkflowField = (
    prov: string,
    field: keyof WorkflowStepConfig,
    value: string | boolean | null,
  ) => {
    const key = workflowKey(prov);
    setSettings((prev) => {
      if (!prev) return prev;
      const current: WorkflowStepConfig = (prev[key] as WorkflowStepConfig | null | undefined) ?? emptyWorkflow();
      return { ...prev, [key]: { ...current, [field]: value } };
    });
  };

  const handleSave = async () => {
    if (!settings) return;
    setSaving(true);
    setError(null);
    // Sync default_project_key to whichever provider is active
    const active = settings.default_provider;
    const syncedKey = active === "github" ? settings.github_project_key
      : active === "linear" ? settings.linear_project_key
      : active === "jira" ? settings.jira_project_key
      : null;
    const toSave: ProjectSettings = { ...settings, default_project_key: syncedKey ?? null };
    try {
      await updateProjectSettings(projectId, toSave);
      setSettings(toSave);
      setSaved(true);
      setTimeout(() => setSaved(false), 2500);
      onSaved?.();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  if (!settings) {
    return (
      <div style={{ padding: compact ? 20 : 24, color: C.text4, fontSize: 12 }}>
        <span className="spinner" style={{ width: 14, height: 14, borderWidth: 1.5, display: "inline-block", marginRight: 8, verticalAlign: "middle" }} />
        Loading…
      </div>
    );
  }

  const pad = compact ? 20 : 24;

  return (
    <div style={{ padding: pad, display: "flex", flexDirection: "column", gap: 28 }}>

      {!compact && (
        <div>
          <div style={{ fontSize: 15, fontWeight: 700, color: C.text1 }}>
            {projectName ? `${projectName}` : "Project Settings"}
          </div>
          {rootPath && (
            <div style={{ fontSize: 10, color: C.text4, fontFamily: C.mono, marginTop: 3, opacity: 0.7 }}>
              {rootPath}
            </div>
          )}
        </div>
      )}

      {/* ── Issue Tracker section ─────────────────────────────── */}
      <div>
        <SectionHeader title="Issue Tracker" subtitle="Connect boards and set your default provider." />

        {/* Provider tiles */}
        <div style={{ marginBottom: 20 }}>
          <div style={lbl}>Default Provider</div>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
            {PROVIDER_CONFIGS.map((pc) => {
              const active = settings.default_provider === pc.id;
              const connected = isConnected(pc.id);
              return (
                <button
                  key={pc.id}
                  onClick={() => setField("default_provider", active ? null : pc.id)}
                  title={!connected ? `${pc.label} is not connected` : undefined}
                  style={{
                    display: "flex", alignItems: "center", gap: 10,
                    padding: "10px 14px", borderRadius: 8,
                    background: active ? `${pc.color}14` : C.surfaceHover,
                    border: `1.5px solid ${active ? pc.color : "transparent"}`,
                    cursor: "pointer", textAlign: "left",
                    opacity: connected ? 1 : 0.45,
                    transition: "all 0.15s",
                    position: "relative",
                  }}
                >
                  <span style={{
                    width: 8, height: 8, borderRadius: "50%", flexShrink: 0,
                    background: connected ? (active ? pc.color : C.text4) : C.danger,
                    boxShadow: active && connected ? `0 0 6px ${pc.color}66` : "none",
                  }} />
                  <div style={{ minWidth: 0 }}>
                    <div style={{ fontSize: 12, fontWeight: active ? 600 : 400, color: active ? C.text1 : C.text2, lineHeight: 1.3 }}>
                      {pc.label}
                    </div>
                    <div style={{ fontSize: 10, color: connected ? C.text4 : C.danger, marginTop: 1 }}>
                      {connected ? "connected" : "not connected"}
                    </div>
                  </div>
                  {active && (
                    <span style={{
                      position: "absolute", top: 6, right: 8,
                      width: 14, height: 14, borderRadius: "50%",
                      background: pc.color,
                      display: "flex", alignItems: "center", justifyContent: "center",
                    }}>
                      <Check size={8} />
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        </div>

        {/* Per-provider board selectors */}
        {(["github", "linear", "jira"] as const).map((prov) => {
          const pc = PROVIDER_CONFIGS.find((p) => p.id === prov)!;
          const keyField = pc.keyField as "github_project_key" | "linear_project_key" | "jira_project_key";
          const isDefault = settings.default_provider === prov;
          return (
            <BoardSelector
              key={prov}
              label={pc.keyLabel}
              provider={prov}
              providerColor={pc.color}
              isDefault={isDefault}
              connected={isConnected(prov)}
              providerProjects={projects[prov]}
              loading={loading[prov]}
              value={settings[keyField] ?? ""}
              onChange={(v) => setField(keyField, v || null)}
              placeholder={pc.keyPlaceholder}
            />
          );
        })}
      </div>

      {/* Divider */}
      <div style={{ height: 1, background: C.border }} />

      {/* ── Run Defaults section ──────────────────────────────── */}
      <div>
        <SectionHeader title="Run Defaults" subtitle="Pre-fill configuration for new runs in this project." />

        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          <FieldGroup label="Pipeline">
            <NativeSelect
              value={settings.default_pipeline ?? ""}
              onChange={(v) => setField("default_pipeline", v || null)}
              options={PIPELINES}
            />
          </FieldGroup>

          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <FieldGroup label="Parallel Agents">
              <NativeSelect
                value={settings.max_parallel_agents !== null ? String(settings.max_parallel_agents) : ""}
                onChange={(v) => setField("max_parallel_agents", v ? parseInt(v, 10) : null)}
                options={PARALLEL_OPTS.map((v) => ({
                  value: v,
                  label: v === "" ? "Inherit" : `${v} agent${parseInt(v) !== 1 ? "s" : ""}`,
                }))}
              />
            </FieldGroup>

          </div>

          <FieldGroup label="Permission Mode">
            <NativeSelect
              value={settings.default_permission_mode ?? ""}
              onChange={(v) => setField("default_permission_mode", v || null)}
              options={PERMISSIONS}
            />
          </FieldGroup>
        </div>
      </div>

      {/* Divider */}
      <div style={{ height: 1, background: C.border }} />

      {/* ── Workflow section ───────────────────────────────────── */}
      <div>
        <SectionHeader title="Issue Workflow" subtitle="Automatically transition issues when runs start, succeed, or fail." />
        {(["github", "linear", "jira", "grove"] as const).map((prov) => {
          if (prov !== "grove" && !isConnected(prov)) return null;
          const provStatuses = statuses[prov] ?? [];
          const wfKey = workflowKey(prov);
          const workflow: WorkflowStepConfig = (settings[wfKey] as WorkflowStepConfig | null | undefined) ?? emptyWorkflow();
          const pc = PROVIDER_CONFIGS.find((p) => p.id === prov);
          const provColor = pc?.color ?? C.text4;
          const statusOptions = [
            { value: "", label: "No transition" },
            ...provStatuses.map((s) => ({ value: s.id, label: s.name })),
          ];
          return (
            <div key={prov} style={{ marginBottom: 20 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 10 }}>
                <span style={{
                  width: 8, height: 8, borderRadius: "50%", background: provColor, flexShrink: 0,
                }} />
                <span style={{ fontSize: 11, fontWeight: 600, color: C.text1 }}>{pc?.label ?? prov}</span>
              </div>
              <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 10, marginBottom: 10 }}>
                <FieldGroup label="When run starts">
                  <NativeSelect
                    value={workflow.on_start ?? ""}
                    onChange={(v) => setWorkflowField(prov, "on_start", v || null)}
                    options={statusOptions}
                  />
                </FieldGroup>
                <FieldGroup label="When run succeeds">
                  <NativeSelect
                    value={workflow.on_success ?? ""}
                    onChange={(v) => setWorkflowField(prov, "on_success", v || null)}
                    options={statusOptions}
                  />
                </FieldGroup>
                <FieldGroup label="When run fails">
                  <NativeSelect
                    value={workflow.on_failure ?? ""}
                    onChange={(v) => setWorkflowField(prov, "on_failure", v || null)}
                    options={statusOptions}
                  />
                </FieldGroup>
              </div>
              <div style={{ display: "flex", gap: 20 }}>
                <label style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 11, color: C.text2, cursor: "pointer" }}>
                  <input
                    type="checkbox"
                    checked={workflow.comment_on_success}
                    onChange={(e) => setWorkflowField(prov, "comment_on_success", e.target.checked)}
                    style={{ accentColor: provColor }}
                  />
                  Comment on success
                </label>
                <label style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 11, color: C.text2, cursor: "pointer" }}>
                  <input
                    type="checkbox"
                    checked={workflow.comment_on_failure}
                    onChange={(e) => setWorkflowField(prov, "comment_on_failure", e.target.checked)}
                    style={{ accentColor: provColor }}
                  />
                  Comment on failure
                </label>
              </div>
            </div>
          );
        })}
      </div>

      {/* ── Save footer ───────────────────────────────────────── */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 10, paddingTop: 4 }}>
        {error && <span style={{ fontSize: 11, color: C.danger, marginRight: "auto" }}>{error}</span>}
        {saved && (
          <span style={{ display: "flex", alignItems: "center", gap: 5, fontSize: 11, color: C.accent }}>
            <Check size={11} /> Saved
          </span>
        )}
        <button
          onClick={handleSave}
          disabled={saving}
          style={{
            padding: "8px 22px", borderRadius: 6, fontSize: 12, fontWeight: 600,
            background: C.accent, border: "none", color: "#15171E",
            cursor: saving ? "default" : "pointer",
            opacity: saving ? 0.6 : 1,
            display: "flex", alignItems: "center", gap: 6,
          }}
        >
          {saving && <span className="spinner" style={{ width: 11, height: 11, borderWidth: 1.5, borderTopColor: "#15171E" }} />}
          {saving ? "Saving…" : "Save Settings"}
        </button>
      </div>

    </div>
  );
}

// ── BoardSelector ─────────────────────────────────────────────────────────────

interface BoardSelectorProps {
  label: string;
  provider: string;
  providerColor: string;
  isDefault: boolean;
  connected: boolean;
  providerProjects: ProviderProject[];
  loading: boolean;
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
}

function BoardSelector({
  label, provider, providerColor, isDefault, connected,
  providerProjects, loading, value, onChange, placeholder,
}: BoardSelectorProps) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const close = useCallback(() => { setOpen(false); setSearch(""); }, []);

  useEffect(() => {
    if (open) setTimeout(() => searchRef.current?.focus(), 30);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) close();
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open, close]);

  const filtered = providerProjects.filter((p) => {
    const q = search.toLowerCase();
    return !q || p.name.toLowerCase().includes(q) || (p.key ?? "").toLowerCase().includes(q);
  });

  const selectedProject = providerProjects.find((p) => (p.key ?? p.id) === value);

  if (!connected) return null;

  return (
    <div style={{ marginBottom: 16 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 6 }}>
        <div style={lbl as React.CSSProperties}>{label}</div>
        <span style={{
          fontSize: 9, padding: "1px 6px", borderRadius: 10, fontWeight: 600,
          background: isDefault ? `${providerColor}18` : "rgba(255,255,255,0.04)",
          color: isDefault ? providerColor : C.text4,
          border: `1px solid ${isDefault ? `${providerColor}33` : "transparent"}`,
        }}>
          {provider}{isDefault ? " · default" : ""}
        </span>
      </div>

      <div ref={containerRef} style={{ position: "relative" }}>
        <button
          onClick={() => setOpen((v) => !v)}
          style={{
            width: "100%", background: C.base, borderRadius: 6,
            padding: "8px 12px", color: value ? C.text1 : C.text4, fontSize: 11,
            border: `1px solid ${open ? C.borderHover : C.border}`,
            cursor: "pointer", textAlign: "left",
            display: "flex", alignItems: "center", gap: 8,
            transition: "border-color 0.15s",
          }}
        >
          {loading ? (
            <span style={{ color: C.text4 }}>Fetching {label.toLowerCase()}s…</span>
          ) : selectedProject ? (
            <>
              <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {selectedProject.name}
              </span>
              {selectedProject.key && (
                <span style={{
                  fontSize: 9, padding: "1px 6px", borderRadius: 4, flexShrink: 0,
                  background: `${providerColor}14`, color: providerColor,
                  fontFamily: C.mono, fontWeight: 600,
                }}>
                  {selectedProject.key}
                </span>
              )}
            </>
          ) : (
            <span style={{ color: C.text4, flex: 1 }}>
              {providerProjects.length === 0 ? `No ${label.toLowerCase()}s found` : `Select ${label.toLowerCase()}…`}
            </span>
          )}
          <span style={{
            color: C.text4, flexShrink: 0,
            transform: open ? "rotate(180deg)" : "none", transition: "transform 0.15s",
          }}>
            <ChevronDown size={10} />
          </span>
        </button>

        {/* Manual key entry when no projects available */}
        {!loading && providerProjects.length === 0 && (
          <input
            value={value}
            onChange={(e) => onChange(e.target.value)}
            placeholder={placeholder}
            style={{
              marginTop: 6, width: "100%", background: C.base,
              borderRadius: 6, padding: "8px 12px",
              color: C.text2, fontSize: 11,
              border: `1px solid ${C.border}`, outline: "none",
              boxSizing: "border-box",
            }}
          />
        )}

        {open && providerProjects.length > 0 && (
          <>
            <div
              onClick={close}
              style={{ position: "fixed", inset: 0, zIndex: 1040 }}
            />
            <div style={{
              position: "absolute", top: "calc(100% + 4px)", left: 0, right: 0,
              zIndex: 1050, borderRadius: 8,
              background: C.surfaceActive,
              border: `1px solid ${C.borderHover}`,
              boxShadow: "0 8px 32px rgba(0,0,0,0.4)",
              overflow: "hidden",
            }}>
              {/* Search */}
              <div style={{ padding: "8px 10px", borderBottom: `1px solid ${C.border}` }}>
                <input
                  ref={searchRef}
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder={`Search ${providerProjects.length} ${label.toLowerCase()}s…`}
                  style={{
                    width: "100%", background: C.base,
                    borderRadius: 4, padding: "5px 10px",
                    color: C.text1, fontSize: 11, border: "none", outline: "none",
                    boxSizing: "border-box",
                  }}
                />
              </div>

              {/* List */}
              <div style={{ maxHeight: 200, overflowY: "auto" }}>
                {/* Clear option */}
                <button
                  onClick={() => { onChange(""); close(); }}
                  style={{
                    width: "100%", padding: "8px 14px", background: "transparent",
                    border: "none", cursor: "pointer", textAlign: "left",
                    display: "flex", alignItems: "center", gap: 8,
                    color: C.text4, fontSize: 11,
                    borderBottom: `1px solid ${C.border}`,
                  }}
                >
                  <span style={{ width: 14, flexShrink: 0 }} />
                  None — clear selection
                </button>
                {filtered.map((p) => {
                  const key = p.key ?? p.id;
                  const active = value === key;
                  return (
                    <button
                      key={p.id}
                      onClick={() => { onChange(key); close(); }}
                      style={{
                        width: "100%", padding: "8px 14px",
                        background: active ? `${providerColor}10` : "transparent",
                        border: "none", cursor: "pointer", textAlign: "left",
                        display: "flex", alignItems: "center", gap: 8,
                        color: active ? C.text1 : C.text2, fontSize: 11,
                      }}
                    >
                      <span style={{
                        width: 14, flexShrink: 0, display: "flex",
                        alignItems: "center", justifyContent: "center",
                        color: providerColor,
                      }}>
                        {active && <Check size={10} />}
                      </span>
                      <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                        {p.name}
                      </span>
                      {p.key && (
                        <span style={{
                          fontSize: 9, padding: "1px 5px", borderRadius: 3, flexShrink: 0,
                          background: "rgba(255,255,255,0.06)", color: C.text4,
                          fontFamily: C.mono,
                        }}>
                          {p.key}
                        </span>
                      )}
                    </button>
                  );
                })}
                {filtered.length === 0 && (
                  <div style={{ padding: "12px 14px", color: C.text4, fontSize: 11 }}>
                    No matches
                  </div>
                )}
              </div>

              <div style={{
                padding: "5px 14px", borderTop: `1px solid ${C.border}`,
                fontSize: 10, color: C.text4,
              }}>
                {filtered.length} of {providerProjects.length}
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

// ── Sub-components ─────────────────────────────────────────────────────────────

function SectionHeader({ title, subtitle }: { title: string; subtitle?: string }) {
  return (
    <div style={{ marginBottom: 20 }}>
      <div style={{ fontSize: 12, fontWeight: 700, color: C.text1 }}>{title}</div>
      {subtitle && <div style={{ fontSize: 11, color: C.text4, marginTop: 2 }}>{subtitle}</div>}
    </div>
  );
}

function FieldGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <div style={lbl}>{label}</div>
      {children}
    </div>
  );
}

function NativeSelect({
  value, onChange, options,
}: {
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string }[];
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      style={{
        width: "100%", background: C.base, borderRadius: 6,
        padding: "7px 10px", color: value ? C.text2 : C.text4, fontSize: 11,
        border: `1px solid ${C.border}`, outline: "none", cursor: "pointer",
        appearance: "none",
      }}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>{o.label}</option>
      ))}
    </select>
  );
}
