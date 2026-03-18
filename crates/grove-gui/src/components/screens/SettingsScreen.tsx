import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { formatBytes } from "@/lib/hooks";
import { qk } from "@/lib/queryKeys";
import {
  getWorkspace, getLlmSelection,
  updateWorkspaceName, updateProjectName,
  listProjects, archiveProject, deleteProject,
  getWorkspaceRoot,
  listWorktrees, cleanWorktrees, cleanWorktreesScoped, deleteWorktree,
  getHooksConfig,
  listProviders, listModels, setApiKey, removeApiKey, setLlmSelection,
  detectEditors, checkConnections,
  getAgentCatalog, getDefaultProvider, setDefaultProvider, setAgentEnabled,
  getAppVersion,
} from "@/lib/api";
import { C, lbl } from "@/lib/theme";
import {
  Check, Gear, Sparkles, LinkIcon, Folder,
  Worktree, HandIcon, Trash, Pencil, Archive, Terminal, Bolt, InfoCircle,
} from "@/components/ui/icons";
import { GroveLogo } from "@/components/ui/GroveLogo";
import { ConnectionsPanel } from "@/components/connections/ConnectionsPanel";
import { ProjectSettingsPanel } from "@/components/settings/ProjectSettingsPanel";
import { AgentStudioPanel } from "@/components/settings/AgentStudioPanel";
import type {
  WorkspaceRow, ProjectRow, LlmSelection, NavScreen,
  WorktreeEntry, HookConfig, ProviderStatus, EditorIntegrationStatus, ConnectionStatus,
  AgentCatalogEntry,
} from "@/types";

// ── Types ─────────────────────────────────────────────────────────────────────

type SettingsTab =
  | "general"
  | "agents"
  | "studio"
  | "llm"
  | "connections"
  | "editors"
  | "projects"
  | "worktrees"
  | "hooks"
  | "about";

interface NavItem {
  id: SettingsTab;
  label: string;
  icon: React.ReactNode;
}

interface NavGroup {
  label: string;
  items: NavItem[];
}

const NAV_GROUPS: NavGroup[] = [
  {
    label: "Workspace",
    items: [
      { id: "general",     label: "General",       icon: <Gear size={14} /> },
      { id: "agents",      label: "Coding Agents", icon: <Bolt size={14} /> },
      { id: "studio",      label: "Agent Studio",  icon: <Pencil size={14} /> },
    ],
  },
  {
    label: "Integrations",
    items: [
      { id: "llm",         label: "LLM Providers", icon: <Sparkles size={14} /> },
      { id: "editors",     label: "Editors",       icon: <Terminal size={14} /> },
      { id: "connections", label: "Connections",    icon: <LinkIcon size={14} /> },
    ],
  },
  {
    label: "Projects",
    items: [
      { id: "projects",    label: "Projects",       icon: <Folder size={14} /> },
    ],
  },
  {
    label: "System",
    items: [
      { id: "worktrees",   label: "Worktrees",      icon: <Worktree size={14} /> },
      { id: "hooks",       label: "Hooks & Guards", icon: <HandIcon size={14} /> },
      { id: "about",       label: "About",          icon: <InfoCircle size={14} /> },
    ],
  },
];

// ── Props ─────────────────────────────────────────────────────────────────────

interface SettingsScreenProps {
  onNavigate: (screen: NavScreen) => void;
  onCreateProject: () => void;
}

// ── Main component ────────────────────────────────────────────────────────────

export function SettingsScreen({ onNavigate, onCreateProject }: SettingsScreenProps) {
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<SettingsTab>("general");

  const { data: workspace,  refetch: refetchWs   } = useQuery({ queryKey: qk.workspace(),    queryFn: getWorkspace,    refetchInterval: 60000, staleTime: 30000 });
  const { data: selection                        } = useQuery({ queryKey: qk.llmSelection(), queryFn: getLlmSelection, refetchInterval: 60000, staleTime: 30000 });
  const { data: providers                        } = useQuery({ queryKey: qk.providers(),    queryFn: listProviders,   refetchInterval: 60000, staleTime: 30000 });
  const { data: editors                          } = useQuery({ queryKey: qk.editors(),      queryFn: detectEditors,   refetchInterval: 60000, staleTime: 30000 });
  const { data: connections                      } = useQuery({ queryKey: qk.connections(),  queryFn: checkConnections,refetchInterval: 60000, staleTime: 30000 });
  const { data: projects,   refetch: refetchProjs} = useQuery({ queryKey: qk.projects(),     queryFn: listProjects,    refetchInterval: 60000, staleTime: 30000 });
  const { data: projectRoot                      } = useQuery({ queryKey: qk.projectRoot(),  queryFn: getWorkspaceRoot,refetchInterval: 60000, staleTime: 30000 });
  const { data: worktrees,  refetch: refetchWt   } = useQuery({ queryKey: qk.worktrees(),    queryFn: listWorktrees,   refetchInterval: 60000, staleTime: 30000 });
  const { data: hooksConfig                      } = useQuery({ queryKey: qk.hooks(),        queryFn: getHooksConfig,  refetchInterval: 60000, staleTime: 30000 });
  const { data: agentCatalog = [],  refetch: refetchAgents } = useQuery({ queryKey: qk.agentCatalog(),     queryFn: getAgentCatalog,    refetchInterval: 60000, staleTime: 30000 });
  const { data: defaultProviderValue, refetch: refetchDefaultProvider } = useQuery({ queryKey: qk.defaultProvider(), queryFn: getDefaultProvider, refetchInterval: 60000, staleTime: 30000 });
  const { data: appVersion } = useQuery({ queryKey: ["appVersion"], queryFn: getAppVersion, staleTime: Infinity });

  return (
    <div style={{ flex: 1, display: "flex", overflow: "hidden", background: C.base }}>

      {/* ── Left navigation ─────────────────────────────────── */}
      <aside style={{
        width: 210,
        flexShrink: 0,
        background: C.surface,
        borderRight: `1px solid ${C.border}`,
        display: "flex",
        flexDirection: "column",
        padding: "24px 0",
        overflowY: "auto",
      }}>
        {/* Title */}
        <div style={{ padding: "0 20px 20px" }}>
          <div style={{ fontSize: 15, fontWeight: 700, color: C.text1 }}>Settings</div>
          <div style={{ fontSize: 10, color: C.text4, marginTop: 2 }}>Workspace & project config</div>
        </div>

        {/* Nav groups */}
        {NAV_GROUPS.map((group) => (
          <div key={group.label} style={{ marginBottom: 4 }}>
            <div style={{
              padding: "4px 20px 6px",
              fontSize: 9, fontWeight: 700, color: C.text4,
              textTransform: "uppercase", letterSpacing: "0.08em",
            }}>
              {group.label}
            </div>
            {group.items.map((item) => {
              const active = tab === item.id;
              return (
                <button
                  key={item.id}
                  onClick={() => setTab(item.id)}
                  style={{
                    width: "100%",
                    display: "flex", alignItems: "center", gap: 10,
                    padding: "7px 20px",
                    background: active ? C.accentMuted : "transparent",
                    border: "none",
                    borderLeft: `2px solid ${active ? C.accent : "transparent"}`,
                    color: active ? C.accent : C.text3,
                    fontSize: 12, fontWeight: active ? 600 : 400,
                    cursor: "pointer",
                    textAlign: "left",
                    transition: "background 0.1s, color 0.1s",
                  }}
                  onMouseEnter={(e) => {
                    if (!active) (e.currentTarget as HTMLButtonElement).style.background = C.surfaceHover;
                  }}
                  onMouseLeave={(e) => {
                    if (!active) (e.currentTarget as HTMLButtonElement).style.background = "transparent";
                  }}
                >
                  <span style={{ opacity: active ? 1 : 0.65, flexShrink: 0 }}>{item.icon}</span>
                  {item.label}
                </button>
              );
            })}
            <div style={{ height: 12 }} />
          </div>
        ))}

        {/* Version at bottom of sidebar */}
        <div style={{ marginTop: "auto", padding: "16px 20px", fontSize: 10, color: C.text4 }}>
          Grove v{appVersion ?? "..."}
        </div>
      </aside>

      {/* ── Right content ────────────────────────────────────── */}
      <main style={{ flex: 1, overflowY: "auto" }}>
        <div style={{ maxWidth: 980, padding: "32px 40px" }}>
          {tab === "general" && (
            <GeneralSection
              workspace={workspace ?? null}
              projectRoot={projectRoot ?? undefined}
              projectCount={projects?.filter((project) => project.state === "active").length ?? 0}
              providerCount={providers?.length ?? 0}
              onSaved={refetchWs}
              onNavigate={onNavigate}
            />
          )}
          {tab === "agents" && (
            <AgentsSection
              catalog={agentCatalog as AgentCatalogEntry[]}
              defaultProvider={defaultProviderValue ?? ""}
              onSaved={() => {
                void refetchAgents();
                void refetchDefaultProvider();
              }}
            />
          )}
          {tab === "studio" && <AgentStudioPanel />}
          {tab === "llm" && (
            <LlmSection
              providers={providers ?? []}
              selection={selection ?? null}
              onSaved={() => {
                void queryClient.invalidateQueries({ queryKey: qk.providers() });
                void queryClient.invalidateQueries({ queryKey: qk.llmSelection() });
              }}
            />
          )}
          {tab === "connections" && <ConnectionsSection connections={connections ?? []} />}
          {tab === "editors" && (
            <EditorsSection
              editors={editors ?? []}
              catalog={agentCatalog as AgentCatalogEntry[]}
              onSaved={() => void refetchAgents()}
            />
          )}
          {tab === "projects" && (
            <ProjectsSection
              projects={projects ?? undefined}
              projectRoot={projectRoot ?? undefined}
              onChanged={refetchProjs}
              onCreateProject={onCreateProject}
            />
          )}
          {tab === "worktrees" && (
            <WorktreesSection
              worktrees={worktrees ?? undefined}
              projects={projects ?? undefined}
              onChanged={refetchWt}
            />
          )}
          {tab === "hooks" && <HooksSection hooksConfig={hooksConfig ?? null} />}
          {tab === "about" && <AboutSection version={appVersion ?? null} />}
        </div>
      </main>

    </div>
  );
}

// ── Shared primitives ─────────────────────────────────────────────────────────

function PageHeader({ title, subtitle }: { title: string; subtitle?: string }) {
  return (
    <div style={{ marginBottom: 28 }}>
      <div style={{ fontSize: 18, fontWeight: 700, color: C.text1 }}>{title}</div>
      {subtitle && <div style={{ fontSize: 12, color: C.text4, marginTop: 4 }}>{subtitle}</div>}
    </div>
  );
}

function Card({ children }: { children: React.ReactNode }) {
  return (
    <div style={{
      background: C.surface,
      borderRadius: 10,
      border: `1px solid ${C.border}`,
      overflow: "hidden",
    }}>
      {children}
    </div>
  );
}

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div style={{
      display: "grid", gridTemplateColumns: "160px 1fr",
      alignItems: "center", gap: 12,
      padding: "10px 0",
      borderBottom: `1px solid ${C.border}`,
    }}>
      <span style={{ fontSize: 11, color: C.text4, fontWeight: 500 }}>{label}</span>
      <span style={{
        fontSize: 12, color: C.text2,
        fontFamily: mono ? C.mono : undefined,
        overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
      }}>
        {value || "—"}
      </span>
    </div>
  );
}

function EditRow({ label, value, onSave }: { label: string; value: string; onSave: (v: string) => Promise<void> }) {
  const [editing, setEditing] = useState(false);
  const [input, setInput]     = useState(value);
  const [saved, setSaved]     = useState(false);

  const handleSave = async () => {
    await onSave(input);
    setEditing(false);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  return (
    <div style={{
      display: "grid", gridTemplateColumns: "160px 1fr",
      alignItems: "center", gap: 12,
      padding: "10px 0",
      borderBottom: `1px solid ${C.border}`,
    }}>
      <span style={{ fontSize: 11, color: C.text4, fontWeight: 500 }}>{label}</span>
      {editing ? (
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <input
            autoFocus
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") handleSave(); if (e.key === "Escape") setEditing(false); }}
            style={{
              flex: 1, padding: "5px 10px", borderRadius: 6,
              background: C.surfaceHover, border: `1px solid ${C.borderHover}`,
              color: C.text1, fontSize: 12, outline: "none",
            }}
          />
          <Btn onClick={handleSave} accent>Save</Btn>
          <Btn onClick={() => setEditing(false)}>Cancel</Btn>
        </div>
      ) : (
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <span style={{ flex: 1, fontSize: 12, color: C.text2 }}>{value || "—"}</span>
          <button
            onClick={() => { setInput(value); setEditing(true); }}
            style={{
              display: "flex", alignItems: "center", gap: 4,
              padding: "3px 8px", borderRadius: 6, fontSize: 10,
              background: "transparent", border: `1px solid ${C.border}`,
              color: C.text3, cursor: "pointer",
            }}
          >
            <Pencil size={10} /> Edit
          </button>
          {saved && <span style={{ fontSize: 10, color: C.accent }}><Check size={10} /></span>}
        </div>
      )}
    </div>
  );
}

function Btn({
  onClick, children, accent = false, danger = false, disabled = false,
}: {
  onClick: () => void;
  children: React.ReactNode;
  accent?: boolean;
  danger?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      style={{
        padding: "5px 12px", borderRadius: 6, fontSize: 10, fontWeight: 600,
        border: "none",
        background: accent ? C.accent : danger ? "#EF4444" : C.surfaceHover,
        color: accent ? "#15171E" : danger ? "#fff" : C.text3,
        cursor: disabled ? "default" : "pointer",
        opacity: disabled ? 0.5 : 1,
        flexShrink: 0,
      }}
    >
      {children}
    </button>
  );
}

function Badge({ label, variant = "default" }: { label: string; variant?: "default" | "green" | "yellow" | "red" }) {
  const colors: Record<string, [string, string]> = {
    default: ["rgba(113,118,127,0.15)", "#71767F"],
    green:   ["rgba(49,185,123,0.12)",  "#31B97B"],
    yellow:  ["rgba(245,158,11,0.12)",  "#F59E0B"],
    red:     ["rgba(239,68,68,0.12)", "#EF4444"],
  };
  const [bg, fg] = colors[variant] ?? colors.default;
  return (
    <span style={{
      fontSize: 9, padding: "2px 7px", borderRadius: 10,
      background: bg, color: fg, fontWeight: 600,
    }}>
      {label}
    </span>
  );
}

// ── General section ───────────────────────────────────────────────────────────

function GeneralSection({ workspace, projectRoot, projectCount, providerCount, onSaved, onNavigate: _onNavigate }: {
  workspace: WorkspaceRow | null | undefined;
  projectRoot: string | undefined;
  projectCount: number;
  providerCount: number;
  onSaved: () => void;
  onNavigate: (s: NavScreen) => void;
}) {
  const rootName = projectRoot ? projectRoot.split("/").filter(Boolean).pop() ?? projectRoot : "Not set";

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
      <PageHeader title="General" subtitle="Workspace identity and meta-information." />
      <Card>
        <div style={{ padding: "18px 20px" }}>
          <div style={{ display: "grid", gap: 10, gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))" }}>
            <MetricTile label="Workspace" value={workspace?.name ?? "Unnamed"} hint="Display name used across Grove" />
            <MetricTile label="Projects" value={String(projectCount)} hint="Active projects in this workspace" />
            <MetricTile label="Providers" value={String(providerCount)} hint="Available LLM integrations" />
            <MetricTile label="Root" value={rootName} hint="Current workspace root folder" />
          </div>
        </div>
      </Card>
      <Card>
        <div style={{ padding: "8px 20px 4px" }}>
          <Row label="Workspace ID" value={workspace?.id ?? ""} mono />
          <EditRow
            label="Display Name"
            value={workspace?.name ?? ""}
            onSave={async (v) => { await updateWorkspaceName(v); onSaved(); }}
          />
          <Row label="State" value={workspace?.state ?? ""} />
          <Row label="Root Path" value={projectRoot ?? ""} mono />
          <Row label="Created" value={workspace?.created_at ? new Date(workspace.created_at).toLocaleDateString() : ""} />
          <div style={{ height: 4 }} />
        </div>
      </Card>
    </div>
  );
}

// ── Agents section ────────────────────────────────────────────────────────────

function AgentsSection({
  catalog,
  defaultProvider,
  onSaved,
}: {
  catalog: AgentCatalogEntry[];
  defaultProvider: string;
  onSaved: () => void;
}) {
  const [saving, setSaving] = useState(false);
  const [toggling, setToggling] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pendingDefault, setPendingDefault] = useState<string | null>(null);

  const effectiveDefault = pendingDefault ?? defaultProvider;

  const handleSetDefault = async (id: string) => {
    if (!catalog.find(a => a.id === id)?.enabled) return;
    setPendingDefault(id);
    setSaving(true);
    setError(null);
    try {
      await setDefaultProvider(id);
      onSaved();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setPendingDefault(null);
    } finally {
      setSaving(false);
    }
  };

  const handleToggleEnabled = async (agent: AgentCatalogEntry) => {
    setToggling(agent.id);
    setError(null);
    try {
      await setAgentEnabled(agent.id, !agent.enabled);
      // If we just disabled the current default, clear it
      if (agent.enabled && effectiveDefault === agent.id) {
        setPendingDefault(null);
      }
      onSaved();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setToggling(null);
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
      <PageHeader
        title="Coding Agents"
        subtitle="Enable the agents you have installed and choose a default. The default is pre-selected for every new task."
      />

      {/* Agent list with enable/disable toggles and default selector */}
      <Card>
        <div style={{ padding: "18px 20px" }}>
          <div style={{ fontSize: 10, fontWeight: 700, color: C.text4, textTransform: "uppercase", letterSpacing: "0.07em", marginBottom: 12 }}>
            Available Agents
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {catalog.map(agent => {
              const isDefault = effectiveDefault === agent.id;
              const isEnabled = agent.enabled;
              const isToggling = toggling === agent.id;
              return (
                <div
                  key={agent.id}
                  style={{
                    display: "flex", alignItems: "center", gap: 10,
                    padding: "10px 14px", borderRadius: 8,
                    background: isDefault ? `${C.accent}18` : C.surfaceHover,
                    border: isDefault ? `1px solid ${C.accent}55` : "1px solid transparent",
                    opacity: isEnabled ? 1 : 0.5,
                    transition: "all 0.15s",
                  }}
                >
                  {/* Default radio button — only clickable when enabled */}
                  <button
                    disabled={!isEnabled || saving || !!toggling}
                    onClick={() => handleSetDefault(agent.id)}
                    title={isEnabled ? "Set as default" : "Enable this agent first"}
                    style={{
                      width: 16, height: 16, borderRadius: "50%", flexShrink: 0,
                      border: `2px solid ${isDefault ? C.accent : C.text4}`,
                      background: isDefault ? C.accent : "transparent",
                      display: "flex", alignItems: "center", justifyContent: "center",
                      cursor: isEnabled ? "pointer" : "not-allowed",
                      padding: 0,
                    }}
                  >
                    {isDefault && (
                      <div style={{ width: 5, height: 5, borderRadius: "50%", background: "#fff" }} />
                    )}
                  </button>

                  {/* Agent info */}
                  <div style={{ flex: 1 }}>
                    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                      <span style={{ fontSize: 12, fontWeight: 600, color: isDefault ? C.accent : C.text1 }}>
                        {agent.name}
                      </span>
                      {agent.detected ? (
                        <span style={{ fontSize: 9, padding: "1px 5px", borderRadius: 6, background: "rgba(34,197,94,0.12)", color: "#22c55e", fontWeight: 600 }}>
                          installed
                        </span>
                      ) : (
                        <span style={{ fontSize: 9, padding: "1px 5px", borderRadius: 6, background: "rgba(255,255,255,0.05)", color: C.text4, fontWeight: 600 }}>
                          not installed
                        </span>
                      )}
                    </div>
                    <div style={{ fontSize: 10, color: C.text4, marginTop: 1 }}>
                      <span style={{ fontFamily: C.mono }}>{agent.cli}</span>
                      {agent.models.length > 0 && (
                        <span style={{ marginLeft: 6 }}>
                          · {agent.models.length} model{agent.models.length !== 1 ? "s" : ""}
                        </span>
                      )}
                    </div>
                  </div>

                  {/* DEFAULT badge */}
                  {isDefault && (
                    <span style={{
                      fontSize: 9, padding: "2px 7px", borderRadius: 10,
                      background: `${C.accent}22`, color: C.accent, fontWeight: 700,
                    }}>
                      DEFAULT
                    </span>
                  )}

                  {/* Enable/Disable toggle */}
                  <button
                    disabled={isToggling || !!toggling}
                    onClick={() => handleToggleEnabled(agent)}
                    style={{
                      fontSize: 10, padding: "4px 10px", borderRadius: 6,
                      border: `1px solid ${isEnabled ? "rgba(255,255,255,0.12)" : C.accent + "55"}`,
                      background: isEnabled ? "rgba(255,255,255,0.05)" : `${C.accent}18`,
                      color: isEnabled ? C.text3 : C.accent,
                      cursor: isToggling ? "wait" : "pointer",
                      fontWeight: 600,
                      transition: "all 0.15s",
                    }}
                  >
                    {isToggling ? "…" : isEnabled ? "Disable" : "Enable"}
                  </button>
                </div>
              );
            })}
          </div>
          {error && (
            <div style={{ marginTop: 10, fontSize: 11, color: "#EF4444" }}>{error}</div>
          )}
        </div>
      </Card>

      {/* Agent model reference grid */}
      {catalog.some(a => a.models.length > 0) && (
        <div>
          <div style={{ fontSize: 10, fontWeight: 700, color: C.text4, textTransform: "uppercase", letterSpacing: "0.07em", marginBottom: 10 }}>
            Available Models by Agent
          </div>
          <div style={{ display: "grid", gap: 10, gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))" }}>
            {catalog.filter(a => a.models.length > 0).map(agent => (
              <Card key={agent.id}>
                <div style={{ padding: "14px 16px" }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 10 }}>
                    <div style={{ fontSize: 12, fontWeight: 700, color: C.text1 }}>{agent.name}</div>
                    {!agent.enabled && (
                      <span style={{ fontSize: 9, color: C.text4, background: "rgba(255,255,255,0.05)", padding: "1px 5px", borderRadius: 6 }}>
                        disabled
                      </span>
                    )}
                  </div>
                  <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                    {agent.models.map(m => (
                      <div key={m.id} style={{
                        display: "flex", alignItems: "center", gap: 6,
                        padding: "5px 8px", borderRadius: 6,
                        background: m.is_default ? `${C.accent}10` : "rgba(255,255,255,0.03)",
                      }}>
                        <span style={{ fontSize: 11, color: m.is_default ? C.accent : C.text2, fontWeight: m.is_default ? 600 : 400 }}>
                          {m.name}
                        </span>
                        {m.is_default && (
                          <span style={{ fontSize: 9, color: `${C.accent}99` }}>default</span>
                        )}
                        <span style={{ marginLeft: "auto", fontSize: 9, color: C.text4, fontFamily: C.mono }}>
                          {m.id}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              </Card>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// ── LLM section ───────────────────────────────────────────────────────────────

function LlmSection({ providers, selection, onSaved }: {
  providers: ProviderStatus[];
  selection: LlmSelection | null | undefined;
  onSaved: () => void;
}) {
  const readyProviders = providers.filter((provider) => provider.authenticated);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
      <PageHeader title="LLM Providers" subtitle="Configure provider access and choose the workspace default model routing." />

      <Card>
        <div style={{ padding: "18px 20px" }}>
          <div style={{ fontSize: 10, fontWeight: 700, color: C.text4, textTransform: "uppercase", letterSpacing: "0.07em", marginBottom: 12 }}>
            Workspace Default
          </div>
          <div style={{ display: "grid", gap: 10, gridTemplateColumns: "repeat(auto-fit, minmax(160px, 1fr))" }}>
            <MetricTile label="Provider" value={selection?.provider ?? "Not set"} hint="Default routing target" />
            <MetricTile label="Model" value={selection?.model ?? "Provider default"} hint="Falls back automatically" />
            <MetricTile
              label="Authenticated"
              value={providers.length > 0 ? `${readyProviders.length}/${providers.length}` : "0"}
              hint={readyProviders.length === providers.length && providers.length > 0 ? "All providers ready" : "Review missing keys below"}
            />
          </div>
        </div>
      </Card>

      <WorkspaceLlmSelectionCard providers={providers} selection={selection} onSaved={onSaved} />

      <div>
        <div style={{ fontSize: 10, fontWeight: 700, color: C.text4, textTransform: "uppercase", letterSpacing: "0.07em", marginBottom: 10 }}>
          Provider Access
        </div>
        {providers.length === 0 ? (
          <Card>
            <div style={{ padding: "18px 20px", fontSize: 12, color: C.text3 }}>
              No providers are registered yet.
            </div>
          </Card>
        ) : (
          <div style={{ display: "grid", gap: 12, gridTemplateColumns: "repeat(auto-fit, minmax(320px, 1fr))" }}>
            {providers.map((provider) => (
              <ProviderSettingsCard key={provider.kind} provider={provider} onSaved={onSaved} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function MetricTile({ label, value, hint }: { label: string; value: string; hint: string }) {
  return (
    <div style={{
      background: C.surfaceHover,
      borderRadius: 8,
      padding: "12px 14px",
    }}>
      <div style={{ fontSize: 10, fontWeight: 700, color: C.text4, textTransform: "uppercase", letterSpacing: "0.07em" }}>
        {label}
      </div>
      <div style={{ marginTop: 8, fontSize: 18, fontWeight: 700, color: C.text1 }}>
        {value}
      </div>
      <div style={{ marginTop: 4, fontSize: 11, color: C.text3 }}>
        {hint}
      </div>
    </div>
  );
}

function WorkspaceLlmSelectionCard({
  providers,
  selection,
  onSaved,
}: {
  providers: ProviderStatus[];
  selection: LlmSelection | null | undefined;
  onSaved: () => void;
}) {
  const [provider, setProvider] = useState(selection?.provider ?? "");
  const [model, setModel] = useState(selection?.model ?? "");
  const [authMode, setAuthMode] = useState(selection?.auth_mode ?? "user_key");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    setProvider(selection?.provider ?? "");
    setModel(selection?.model ?? "");
    setAuthMode(selection?.auth_mode ?? "user_key");
  }, [selection?.provider, selection?.model, selection?.auth_mode]);

  const { data: models } = useQuery({
    queryKey: qk.models(provider),
    queryFn: () => listModels(provider),
    enabled: !!provider,
    refetchInterval: 60000,
    staleTime: 30000,
  });

  const handleSave = async () => {
    if (!provider) return;
    setSaving(true);
    await setLlmSelection(provider, model || null, authMode);
    setSaving(false);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
    onSaved();
  };

  return (
    <Card>
      <div style={{ padding: "18px 20px" }}>
        <div style={{ fontSize: 10, fontWeight: 700, color: C.text4, textTransform: "uppercase", letterSpacing: "0.07em", marginBottom: 12 }}>
          Workspace Selection
        </div>
        <div style={{ display: "grid", gap: 12, gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))" }}>
          <div>
            <div style={lbl}>Provider</div>
            <select
              value={provider}
              onChange={(e) => {
                setProvider(e.target.value);
                setModel("");
              }}
              style={selectStyle}
            >
              <option value="">Select provider...</option>
              {providers.map((item) => (
                <option key={item.kind} value={item.kind}>
                  {item.name}
                </option>
              ))}
            </select>
          </div>
          <div>
            <div style={lbl}>Model</div>
            <select
              value={model}
              onChange={(e) => setModel(e.target.value)}
              style={selectStyle}
              disabled={!provider}
            >
              <option value="">Provider default</option>
              {models?.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
            </select>
          </div>
          <div>
            <div style={lbl}>Auth Mode</div>
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
              <RadioBtn active={authMode === "user_key"} onClick={() => setAuthMode("user_key")}>
                User Key
              </RadioBtn>
              <RadioBtn active={authMode === "workspace_credits"} onClick={() => setAuthMode("workspace_credits")}>
                Credits
              </RadioBtn>
            </div>
          </div>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 10, marginTop: 14 }}>
          <Btn onClick={handleSave} accent disabled={saving || !provider}>
            {saving ? "Saving…" : "Save Default"}
          </Btn>
          {saved && (
            <span style={{ color: C.accent, fontSize: 11, display: "flex", alignItems: "center", gap: 4 }}>
              <Check size={10} /> Updated
            </span>
          )}
        </div>
      </div>
    </Card>
  );
}

function ProviderSettingsCard({
  provider,
  onSaved,
}: {
  provider: ProviderStatus;
  onSaved: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [keyInput, setKeyInput] = useState("");
  const [saving, setSaving] = useState(false);

  const { data: models } = useQuery({
    queryKey: qk.models(provider.kind),
    queryFn: () => listModels(provider.kind),
    enabled: expanded,
    refetchInterval: expanded ? 60000 : false,
    staleTime: 30000,
  });

  const handleSetKey = async () => {
    if (!keyInput.trim()) return;
    setSaving(true);
    await setApiKey(provider.kind, keyInput.trim());
    setKeyInput("");
    setSaving(false);
    onSaved();
  };

  const handleRemoveKey = async () => {
    setSaving(true);
    await removeApiKey(provider.kind);
    setSaving(false);
    onSaved();
  };

  return (
    <Card>
      <div style={{ padding: "18px 20px" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <span style={{
            width: 9,
            height: 9,
            borderRadius: "50%",
            background: provider.authenticated ? C.accent : C.text4,
            flexShrink: 0,
          }} />
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
              <span style={{ fontSize: 14, fontWeight: 700, color: C.text1 }}>
                {provider.name}
              </span>
              <Badge label={provider.authenticated ? "Ready" : "Auth Required"} variant={provider.authenticated ? "green" : "yellow"} />
            </div>
            <div style={{ marginTop: 4, fontSize: 11, color: C.text3 }}>
              {provider.model_count} models available. Default model: {provider.default_model}
            </div>
          </div>
          <Btn onClick={() => setExpanded((value) => !value)}>
            {expanded ? "Hide Models" : "Show Models"}
          </Btn>
        </div>

        <div style={{ marginTop: 14 }}>
          <div style={lbl}>API Key</div>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <input
              type="password"
              value={keyInput}
              onChange={(e) => setKeyInput(e.target.value)}
              placeholder={provider.authenticated ? "Key already set" : "Paste provider key"}
              style={{
                flex: 1,
                minWidth: 180,
                padding: "8px 10px",
                borderRadius: 8,
                background: C.surfaceHover,
                border: `1px solid ${C.border}`,
                color: C.text1,
                fontSize: 12,
                outline: "none",
                fontFamily: C.mono,
              }}
            />
            <Btn onClick={handleSetKey} accent disabled={saving || !keyInput.trim()}>
              Set Key
            </Btn>
            {provider.authenticated && (
              <Btn onClick={handleRemoveKey} danger disabled={saving}>
                Remove
              </Btn>
            )}
          </div>
        </div>

        {expanded && (
          <div style={{ marginTop: 14 }}>
            <div style={lbl}>Models</div>
            <div style={{
              background: C.surfaceHover,
              borderRadius: 8,
              overflow: "hidden",
            }}>
              <div style={{
                display: "grid",
                gridTemplateColumns: "minmax(0, 1.4fr) 80px 90px 120px",
                gap: 10,
                padding: "8px 12px",
                fontSize: 10,
                fontWeight: 700,
                color: C.text4,
                textTransform: "uppercase",
                letterSpacing: "0.07em",
              }}>
                <span>Model</span>
                <span>Context</span>
                <span>Output</span>
                <span>Capabilities</span>
              </div>
              {(models ?? []).map((item) => (
                <div
                  key={item.id}
                  style={{
                    display: "grid",
                    gridTemplateColumns: "minmax(0, 1.4fr) 80px 90px 120px",
                    gap: 10,
                    padding: "10px 12px",
                    borderTop: `1px solid ${C.border}`,
                    alignItems: "center",
                  }}
                >
                  <div style={{ minWidth: 0 }}>
                    <div style={{ fontSize: 12, fontWeight: 600, color: C.text1, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                      {item.name}
                    </div>
                    <div style={{ marginTop: 2, fontSize: 10, color: C.text4, fontFamily: C.mono, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                      {item.id}
                    </div>
                  </div>
                  <span style={{ fontSize: 11, color: C.text2 }}>
                    {(item.context_window / 1000).toFixed(0)}K
                  </span>
                  <span style={{ fontSize: 11, color: C.text2 }}>
                    {(item.max_output_tokens / 1000).toFixed(0)}K
                  </span>
                  <div style={{ display: "flex", gap: 4, flexWrap: "wrap" }}>
                    {item.vision && <CapabilityChip label="Vision" />}
                    {item.tools && <CapabilityChip label="Tools" />}
                    {item.reasoning && <CapabilityChip label="Reasoning" />}
                    {!item.vision && !item.tools && !item.reasoning && <CapabilityChip label="Base" />}
                  </div>
                </div>
              ))}
              {(models ?? []).length === 0 && (
                <div style={{ padding: "12px", fontSize: 11, color: C.text3 }}>
                  No models reported for this provider.
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </Card>
  );
}

function CapabilityChip({ label }: { label: string }) {
  return (
    <span style={{
      padding: "3px 7px",
      borderRadius: 999,
      background: C.base,
      color: C.text2,
      fontSize: 10,
      fontWeight: 600,
    }}>
      {label}
    </span>
  );
}

function EditorsSection({
  editors,
  catalog,
  onSaved,
}: {
  editors: EditorIntegrationStatus[];
  catalog: AgentCatalogEntry[];
  onSaved: () => void;
}) {
  const [toggling, setToggling] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Build a lookup from agent catalog for enabled state (source of truth = grove.yaml)
  const catalogById = Object.fromEntries(catalog.map(a => [a.id, a]));

  // Merge editor detection with agent catalog enabled state.
  // Sort: detected first, then by name.
  const sortedEditors = [...editors].sort((a, b) => {
    if (a.detected && !b.detected) return -1;
    if (!a.detected && b.detected) return 1;
    return a.name.localeCompare(b.name);
  });

  const isEnabled = (editor: EditorIntegrationStatus) =>
    catalogById[editor.id]?.enabled ?? true;

  const detectedCount = sortedEditors.filter(e => e.detected).length;
  const enabledCount = sortedEditors.filter(e => e.detected && isEnabled(e)).length;

  const handleToggle = async (editor: EditorIntegrationStatus) => {
    setToggling(editor.id);
    setError(null);
    try {
      await setAgentEnabled(editor.id, !isEnabled(editor));
      onSaved();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setToggling(null);
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
      <PageHeader
        title="Editors"
        subtitle="Detect local coding CLIs and decide which ones Grove uses for tasks. Enabled CLIs appear in the New Task dropdown."
      />

      <Card>
        <div style={{ padding: "18px 20px" }}>
          <div style={{ display: "grid", gap: 10, gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))" }}>
            <MetricTile label="Detected" value={`${detectedCount}/${sortedEditors.length}`} hint="Installed on this machine" />
            <MetricTile label="Enabled" value={String(enabledCount)} hint="Shown in New Task dropdown" />
            <MetricTile
              label="Coverage"
              value={detectedCount > 0 ? "Ready" : "Missing"}
              hint={detectedCount > 0 ? "At least one coding CLI is available" : "Install a CLI to enable it here"}
            />
          </div>
        </div>
      </Card>

      <Card>
        <div style={{ padding: "18px 20px", borderBottom: `1px solid ${C.border}` }}>
          <div style={{ fontSize: 10, fontWeight: 700, color: C.text4, textTransform: "uppercase", letterSpacing: "0.07em" }}>
            Editor Integrations
          </div>
        </div>
        {sortedEditors.map((editor, index) => {
          const enabled = isEnabled(editor);
          const isToggling = toggling === editor.id;
          return (
            <div
              key={editor.id}
              style={{
                padding: "16px 20px",
                borderTop: index === 0 ? "none" : `1px solid ${C.border}`,
                display: "flex",
                gap: 16,
                alignItems: "flex-start",
                justifyContent: "space-between",
                flexWrap: "wrap",
                opacity: editor.detected ? 1 : 0.5,
              }}
            >
              <div style={{ flex: 1, minWidth: 260 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
                  <span style={{ fontSize: 14, fontWeight: 700, color: C.text1 }}>
                    {editor.name}
                  </span>
                  <Badge label={editor.detected ? "Detected" : "Not Installed"} variant={editor.detected ? "green" : "default"} />
                  {editor.detected && enabled && <Badge label="Enabled" variant="yellow" />}
                </div>
                <div style={{ marginTop: 4, fontSize: 12, color: C.text3 }}>
                  {editor.description}
                </div>
                <div style={{ display: "flex", gap: 8, marginTop: 10, flexWrap: "wrap" }}>
                  <span style={{
                    padding: "4px 8px", borderRadius: 999,
                    background: C.surfaceHover, color: C.text2,
                    fontSize: 10, fontFamily: C.mono,
                  }}>
                    {editor.command}
                  </span>
                  <span style={{
                    padding: "4px 8px", borderRadius: 999,
                    background: C.surfaceHover, color: editor.path ? C.text2 : C.text4,
                    fontSize: 10, fontFamily: C.mono,
                    maxWidth: "100%", whiteSpace: "nowrap",
                    overflow: "hidden", textOverflow: "ellipsis",
                  }}>
                    {editor.path ?? "Not found on PATH"}
                  </span>
                </div>
              </div>
              <div style={{ display: "flex", alignItems: "center", gap: 8, flexShrink: 0 }}>
                {editor.detected ? (
                  <Btn
                    onClick={() => void handleToggle(editor)}
                    accent={enabled}
                    disabled={isToggling || !!toggling}
                  >
                    {isToggling ? "…" : enabled ? "Enabled" : "Disabled"}
                  </Btn>
                ) : (
                  <Btn onClick={() => {}} disabled>
                    Not Installed
                  </Btn>
                )}
              </div>
            </div>
          );
        })}
        {error && (
          <div style={{ padding: "12px 20px", fontSize: 11, color: "#EF4444" }}>{error}</div>
        )}
      </Card>
    </div>
  );
}

const selectStyle: React.CSSProperties = {
  width: "100%",
  padding: "8px 10px",
  borderRadius: 8,
  background: C.surfaceHover,
  border: `1px solid ${C.border}`,
  color: C.text1,
  fontSize: 12,
  outline: "none",
  appearance: "none",
};

function RadioBtn({ active, onClick, children }: {
  active: boolean; onClick: () => void; children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      style={{
        padding: "6px 12px", borderRadius: 8, fontSize: 11, fontWeight: 600,
        background: active ? C.accentMuted : C.surfaceHover,
        border: `1px solid ${active ? C.accent + "33" : C.border}`,
        color: active ? C.accent : C.text3,
        cursor: "pointer", transition: "all 0.15s",
      }}
    >
      {children}
    </button>
  );
}

// ── Connections section ───────────────────────────────────────────────────────

function ConnectionsSection({ connections }: { connections: ConnectionStatus[] }) {
  const connected = connections.filter((connection) => connection.connected).length;
  const failing = connections.filter((connection) => !!connection.error).length;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
      <PageHeader title="Connections" subtitle="Connect external issue trackers to Grove." />
      <Card>
        <div style={{ padding: "18px 20px" }}>
          <div style={{ display: "grid", gap: 10, gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))" }}>
            <MetricTile label="Connected" value={`${connected}/${connections.length || 0}`} hint="Tracker integrations ready" />
            <MetricTile label="Errors" value={failing > 0 ? String(failing) : "Clear"} hint={failing > 0 ? "Connections need attention" : "No provider errors reported"} />
            <MetricTile label="Local Board" value="Ready" hint="Grove-native issues always available" />
          </div>
        </div>
      </Card>
      <ConnectionsPanel />
    </div>
  );
}

// ── Projects section ──────────────────────────────────────────────────────────

function ProjectsSection({ projects, projectRoot, onChanged, onCreateProject }: {
  projects: ProjectRow[] | undefined;
  projectRoot: string | undefined;
  onChanged: () => void;
  onCreateProject: () => void;
}) {
  const projectList = projects ?? [];
  const activeProjects = projectList.filter((project) => project.state === "active");
  const sshProjects = activeProjects.filter((project) => project.source_kind === "ssh").length;
  const localProjects = activeProjects.length - sshProjects;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
      <div style={{ display: "flex", alignItems: "center", marginBottom: 28 }}>
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: 18, fontWeight: 700, color: C.text1 }}>Projects</div>
          <div style={{ fontSize: 12, color: C.text4, marginTop: 4 }}>
            Manage registered projects and their defaults.
          </div>
        </div>
        <button
          onClick={onCreateProject}
          style={{
            display: "flex", alignItems: "center", gap: 6,
            padding: "7px 16px", borderRadius: 8, fontSize: 11, fontWeight: 600,
            background: C.accent, border: "none", color: "#15171E", cursor: "pointer",
          }}
        >
          + Add Project
        </button>
      </div>

      <Card>
        <div style={{ padding: "18px 20px" }}>
          <div style={{ display: "grid", gap: 10, gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))" }}>
            <MetricTile label="Active" value={String(activeProjects.length)} hint="Projects available right now" />
            <MetricTile label="Local" value={String(localProjects)} hint="Local checkouts registered" />
            <MetricTile label="SSH" value={String(sshProjects)} hint="Remote shell-only projects" />
            <MetricTile label="Current" value={projectRoot ? (projectList.find((project) => project.root_path === projectRoot)?.name ?? "Selected") : "None"} hint="Current project in the app shell" />
          </div>
        </div>
      </Card>

      {projectList.length === 0 ? (
        <div style={{
          padding: "40px 24px", textAlign: "center",
          background: C.surface, borderRadius: 10, border: `1px solid ${C.border}`,
        }}>
          <div style={{ fontSize: 13, color: C.text3, marginBottom: 8 }}>No projects registered</div>
          <div style={{ fontSize: 11, color: C.text4 }}>Click "Add Project" to register a directory as a Grove project.</div>
        </div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          {projectList.map((p) => (
            <ProjectCard
              key={p.id}
              project={p}
              isCurrent={projectRoot ? p.root_path === projectRoot : false}
              onChanged={onChanged}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function ProjectCard({ project, isCurrent, onChanged }: {
  project: ProjectRow;
  isCurrent: boolean;
  onChanged: () => void;
}) {
  const [confirming,    setConfirming]    = useState(false);
  const [renaming,      setRenaming]      = useState(false);
  const [nameInput,     setNameInput]     = useState(project.name ?? "");
  const [showSettings,  setShowSettings]  = useState(false);

  const handleArchive = async () => {
    await archiveProject(project.id);
    onChanged();
  };

  const handleDelete = async () => {
    if (!confirming) { setConfirming(true); return; }
    await deleteProject(project.id);
    setConfirming(false);
    onChanged();
  };

  const handleRename = async () => {
    await updateProjectName(project.id, nameInput);
    setRenaming(false);
    onChanged();
  };

  return (
    <div style={{
      background: C.surface,
      borderRadius: 10,
      border: `1px solid ${isCurrent ? C.accent + "44" : C.border}`,
      overflow: "hidden",
    }}>
      {/* Project header */}
      <div style={{
        display: "flex", alignItems: "center", gap: 12,
        padding: "14px 18px",
      }}>
        {/* Icon */}
        <div style={{
          width: 36, height: 36, borderRadius: 8,
          background: isCurrent ? C.accentMuted : C.surfaceHover,
          border: `1px solid ${isCurrent ? C.accent + "33" : C.border}`,
          display: "flex", alignItems: "center", justifyContent: "center",
          flexShrink: 0,
        }}>
          <Folder size={16} />
        </div>

        {/* Name + path */}
        <div style={{ flex: 1, minWidth: 0 }}>
          {renaming ? (
            <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <input
                autoFocus
                value={nameInput}
                onChange={(e) => setNameInput(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") handleRename(); if (e.key === "Escape") setRenaming(false); }}
                style={{
                  flex: 1, padding: "4px 8px", borderRadius: 6,
                  background: C.surfaceHover, border: `1px solid ${C.borderHover}`,
                  color: C.text1, fontSize: 12, outline: "none",
                }}
              />
              <Btn onClick={handleRename} accent>Save</Btn>
              <Btn onClick={() => setRenaming(false)}>Cancel</Btn>
            </div>
          ) : (
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span style={{
                fontSize: 13, fontWeight: 600, color: C.text1,
                overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
              }}>
                {project.name || project.id.slice(0, 12)}
              </span>
              {isCurrent && <Badge label="Current" variant="green" />}
              <Badge label={project.source_kind.replace(/_/g, " ")} variant="default" />
              <Badge
                label={project.state}
                variant={project.state === "active" ? "green" : "default"}
              />
            </div>
          )}
          {!renaming && (
            <div style={{
              fontSize: 10, color: C.text4, fontFamily: C.mono, marginTop: 3,
              overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
            }}>
              {project.root_path}
            </div>
          )}
        </div>

        {/* Actions */}
        {!renaming && (
          <div style={{ display: "flex", alignItems: "center", gap: 4, flexShrink: 0 }}>
            {project.state === "active" && (
              <button
                onClick={() => { setNameInput(project.name ?? ""); setRenaming(true); }}
                title="Rename project"
                style={{
                  padding: "5px 6px", borderRadius: 6, background: "transparent",
                  border: "none", color: C.text4, cursor: "pointer",
                }}
                onMouseEnter={(e) => ((e.currentTarget as HTMLButtonElement).style.color = C.text2)}
                onMouseLeave={(e) => ((e.currentTarget as HTMLButtonElement).style.color = C.text4)}
              >
                <Pencil size={12} />
              </button>
            )}
            <button
              onClick={() => setShowSettings((s) => !s)}
              style={{
                padding: "5px 10px", borderRadius: 6, fontSize: 10, fontWeight: 600,
                background: showSettings ? C.accentMuted : C.surfaceHover,
                border: `1px solid ${showSettings ? C.accent + "44" : C.border}`,
                color: showSettings ? C.accent : C.text3, cursor: "pointer",
              }}
            >
              {showSettings ? "Hide" : "Configure"}
            </button>
            {project.state === "active" && !isCurrent && (
              <button
                onClick={handleArchive}
                title="Archive project"
                style={{
                  padding: "5px 6px", borderRadius: 6, background: "transparent",
                  border: "none", color: C.text4, cursor: "pointer",
                }}
                onMouseEnter={(e) => ((e.currentTarget as HTMLButtonElement).style.color = C.warn)}
                onMouseLeave={(e) => ((e.currentTarget as HTMLButtonElement).style.color = C.text4)}
              >
                <Archive size={12} />
              </button>
            )}
            {!isCurrent && (
              <>
                {confirming ? (
                  <div style={{ display: "flex", gap: 4 }}>
                    <Btn onClick={handleDelete} danger>Confirm delete</Btn>
                    <Btn onClick={() => setConfirming(false)}>Cancel</Btn>
                  </div>
                ) : (
                  <button
                    onClick={() => setConfirming(true)}
                    title="Delete project"
                    style={{
                      padding: "5px 6px", borderRadius: 6, background: "transparent",
                      border: "none", color: C.text4, cursor: "pointer",
                    }}
                    onMouseEnter={(e) => ((e.currentTarget as HTMLButtonElement).style.color = C.danger)}
                    onMouseLeave={(e) => ((e.currentTarget as HTMLButtonElement).style.color = C.text4)}
                  >
                    <Trash size={12} />
                  </button>
                )}
              </>
            )}
          </div>
        )}
      </div>

      {/* Expandable settings panel */}
      {showSettings && (
        <div style={{ borderTop: `1px solid ${C.border}`, background: C.surfaceHover }}>
          <ProjectSettingsPanel
            projectId={project.id}
            projectName={project.name}
            rootPath={project.root_path}
            compact
            onSaved={onChanged}
          />
        </div>
      )}
    </div>
  );
}

// ── Worktrees section ─────────────────────────────────────────────────────────

function WorktreesSection({ worktrees, projects, onChanged }: {
  worktrees: WorktreeEntry[] | undefined;
  projects: ProjectRow[] | undefined;
  onChanged: () => void;
}) {
  const [cleaning,       setCleaning]       = useState(false);
  const [cleanResult,    setCleanResult]    = useState<string | null>(null);
  const [scopeProjectId, setScopeProjectId] = useState<string>("");

  const wt = worktrees ?? [];
  const ps = projects ?? [];

  const activeCount   = wt.filter((w) => w.is_active).length;
  const inactiveCount = wt.length - activeCount;
  const totalSize     = wt.reduce((s, w) => s + w.size_bytes, 0);

  const projectNames: Record<string, string> = {};
  for (const p of ps) {
    projectNames[p.id] = p.name || p.id.slice(0, 8);
  }

  const handleClean = async () => {
    setCleaning(true);
    const result = scopeProjectId
      ? await cleanWorktreesScoped(scopeProjectId, null)
      : await cleanWorktrees();
    setCleanResult(`Cleaned ${result.deleted_count} worktree(s), freed ${formatBytes(result.freed_bytes)}`);
    setCleaning(false);
    onChanged();
    setTimeout(() => setCleanResult(null), 5000);
  };

  const handleDelete = async (sessionId: string) => {
    await deleteWorktree(sessionId);
    onChanged();
  };

  return (
    <div>
      <PageHeader title="Worktrees" subtitle="Agent worktree pool — disk usage and cleanup." />

      <Card>
        <div style={{ padding: "18px 20px" }}>
          <div style={{ display: "grid", gap: 10, gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))" }}>
            <MetricTile label="Total" value={String(wt.length)} hint="All tracked worktrees" />
            <MetricTile label="Active" value={String(activeCount)} hint="Currently attached to active sessions" />
            <MetricTile label="Inactive" value={String(inactiveCount)} hint="Safe cleanup candidates" />
            <MetricTile label="Disk" value={formatBytes(totalSize)} hint="Current worktree footprint" />
          </div>
        </div>
      </Card>

      {/* Cleanup controls */}
      <Card>
        <div style={{ padding: "14px 20px", display: "flex", alignItems: "center", gap: 10 }}>
          {ps.length > 0 && (
            <select
              value={scopeProjectId}
              onChange={(e) => setScopeProjectId(e.target.value)}
              style={{
                padding: "6px 10px", borderRadius: 6, fontSize: 11,
                background: C.surfaceHover, border: `1px solid ${C.border}`,
                color: C.text2, outline: "none", cursor: "pointer",
              }}
            >
              <option value="">All projects</option>
              {ps.map((p) => (
                <option key={p.id} value={p.id}>{p.name || p.id.slice(0, 8)}</option>
              ))}
            </select>
          )}
          <Btn onClick={handleClean} accent disabled={cleaning || inactiveCount === 0}>
            {cleaning ? "Cleaning…" : `Clean ${inactiveCount} inactive`}
          </Btn>
          {cleanResult && (
            <span style={{ fontSize: 11, color: C.accent }}>{cleanResult}</span>
          )}
        </div>

        {wt.length > 0 && (
          <div style={{
            borderTop: `1px solid ${C.border}`,
            maxHeight: 280, overflowY: "auto",
          }}>
            {wt.map((w) => (
              <div key={w.session_id} style={{
                display: "flex", alignItems: "center", gap: 10,
                padding: "9px 20px",
                borderBottom: `1px solid ${C.border}`,
              }}>
                <span style={{
                  width: 7, height: 7, borderRadius: "50%", flexShrink: 0,
                  background: w.is_active ? "#31B97B" : C.text4,
                }} />
                <span style={{
                  flex: 1, fontSize: 11, color: C.text3, fontFamily: C.mono,
                  overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                }}>
                  {w.path}
                </span>
                <span style={{ fontSize: 10, color: C.text4, flexShrink: 0 }}>{w.size_display}</span>
                {w.agent_type && (
                  <Badge label={w.agent_type} variant="default" />
                )}
                {w.project_id && projectNames[w.project_id] && (
                  <Badge label={projectNames[w.project_id]} variant="green" />
                )}
                {!w.is_active && (
                  <button
                    onClick={() => handleDelete(w.session_id)}
                    style={{
                      padding: "3px 5px", borderRadius: 5, background: "transparent",
                      border: "none", color: C.text4, cursor: "pointer",
                    }}
                    onMouseEnter={(e) => ((e.currentTarget as HTMLButtonElement).style.color = C.danger)}
                    onMouseLeave={(e) => ((e.currentTarget as HTMLButtonElement).style.color = C.text4)}
                  >
                    <Trash size={10} />
                  </button>
                )}
              </div>
            ))}
          </div>
        )}
      </Card>
    </div>
  );
}

// ── Hooks section ─────────────────────────────────────────────────────────────

function HooksSection({ hooksConfig }: { hooksConfig: HookConfig | null | undefined }) {
  if (!hooksConfig) {
    return (
      <div>
        <PageHeader title="Hooks & Guards" subtitle="Shell commands that run on Grove events." />
        <div style={{ fontSize: 12, color: C.text4 }}>Loading…</div>
      </div>
    );
  }

  const hooks  = hooksConfig.hooks  as Record<string, unknown> | null;
  const guards = hooksConfig.guards as Record<string, unknown> | null;
  const hookEntries  = hooks  ? Object.entries(hooks)  : [];
  const guardEntries = guards ? Object.entries(guards) : [];

  const empty = hookEntries.length === 0 && guardEntries.length === 0;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
      <PageHeader title="Hooks & Guards" subtitle="Shell commands that run on Grove events." />

      <Card>
        <div style={{ padding: "18px 20px" }}>
          <div style={{ display: "grid", gap: 10, gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))" }}>
            <MetricTile label="Hooks" value={String(hookEntries.length)} hint="Configured event hooks" />
            <MetricTile label="Guards" value={String(guardEntries.length)} hint="Configured policy guards" />
            <MetricTile label="Status" value={empty ? "Empty" : "Configured"} hint={empty ? "No hook automation yet" : "Runtime automation is active"} />
          </div>
        </div>
      </Card>

      {empty ? (
        <div style={{
          padding: "40px 24px", textAlign: "center",
          background: C.surface, borderRadius: 10, border: `1px solid ${C.border}`,
        }}>
          <div style={{ fontSize: 12, color: C.text3 }}>No hooks or guards configured</div>
          <div style={{ fontSize: 11, color: C.text4, marginTop: 6 }}>
            Add hooks to <code style={{ fontFamily: C.mono }}>grove.yaml</code> to run commands on events.
          </div>
        </div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          {hookEntries.length > 0 && (
            <div>
              <div style={{
                fontSize: 10, fontWeight: 700, color: C.text4,
                textTransform: "uppercase", letterSpacing: "0.07em", marginBottom: 8,
              }}>
                Event Hooks
              </div>
              <Card>
                <div>
                  {hookEntries.map(([event, cfg], i) => (
                    <div key={event} style={{
                      display: "flex", alignItems: "center", gap: 12,
                      padding: "10px 20px",
                      borderBottom: i < hookEntries.length - 1 ? `1px solid ${C.border}` : undefined,
                    }}>
                      <span style={{
                        fontSize: 10, fontWeight: 700, color: C.accent,
                        fontFamily: C.mono, minWidth: 90, flexShrink: 0,
                      }}>
                        {event}
                      </span>
                      <span style={{
                        fontSize: 11, color: C.text3, fontFamily: C.mono,
                        overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                      }}>
                        {typeof cfg === "string" ? cfg : JSON.stringify(cfg)}
                      </span>
                    </div>
                  ))}
                </div>
              </Card>
            </div>
          )}
          {guardEntries.length > 0 && (
            <div>
              <div style={{
                fontSize: 10, fontWeight: 700, color: C.text4,
                textTransform: "uppercase", letterSpacing: "0.07em", marginBottom: 8,
              }}>
                Guards
              </div>
              <Card>
                <div>
                  {guardEntries.map(([name, cfg], i) => (
                    <div key={name} style={{
                      display: "flex", alignItems: "center", gap: 12,
                      padding: "10px 20px",
                      borderBottom: i < guardEntries.length - 1 ? `1px solid ${C.border}` : undefined,
                    }}>
                      <span style={{
                        fontSize: 10, fontWeight: 700, color: C.warn,
                        fontFamily: C.mono, minWidth: 90, flexShrink: 0,
                      }}>
                        {name}
                      </span>
                      <span style={{
                        fontSize: 11, color: C.text3, fontFamily: C.mono,
                        overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                      }}>
                        {typeof cfg === "string" ? cfg : JSON.stringify(cfg)}
                      </span>
                    </div>
                  ))}
                </div>
              </Card>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ── About section ──────────────────────────────────────────────────────────────

function AboutSection({ version }: { version: string | null }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
      <PageHeader title="About" subtitle="Application information, credits, and links." />

      {/* Hero card */}
      <Card>
        <div style={{
          padding: "40px 32px",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 16,
          background: `linear-gradient(180deg, rgba(49,185,123,0.04) 0%, transparent 100%)`,
        }}>
          <div style={{
            width: 64, height: 64,
            borderRadius: 16,
            background: C.accentDim,
            border: `1px solid ${C.accentBorder}`,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}>
            <GroveLogo size={36} color={C.accent} />
          </div>
          <div style={{ textAlign: "center" }}>
            <div style={{ fontSize: 22, fontWeight: 700, color: C.text1, letterSpacing: "-0.02em" }}>
              Grove
            </div>
            <div style={{ fontSize: 12, color: C.text4, marginTop: 4, lineHeight: 1.5 }}>
              Local orchestration engine for coordinating coding agents
              <br />in isolated git worktrees.
            </div>
          </div>
          {version && (
            <span style={{
              fontSize: 11,
              fontWeight: 600,
              padding: "4px 12px",
              borderRadius: 20,
              background: C.accentDim,
              color: C.accent,
              border: `1px solid ${C.accentBorder}`,
              fontFamily: C.mono,
            }}>
              v{version}
            </span>
          )}
        </div>
      </Card>

      {/* Details */}
      <Card>
        <div style={{ padding: "8px 20px 4px" }}>
          <Row label="Application" value="Grove" />
          <Row label="Version" value={version ? `v${version}` : "..."} mono />
          <Row label="License" value="Apache License 2.0" />
          <Row label="Platform" value="macOS / Linux / Windows" />
          <Row label="Runtime" value="Tauri + React" />
          <Row label="Engine" value="Rust" />
          <div style={{ height: 4 }} />
        </div>
      </Card>


      {/* Links */}
      <Card>
        <div style={{ padding: "16px 20px" }}>
          <div style={{ ...lbl, marginBottom: 14 }}>Links</div>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
            {[
              { label: "GitHub Repository", url: "https://github.com/farooqarahim/grove", icon: "Code" },
              { label: "Report an Issue", url: "https://github.com/farooqarahim/grove/issues", icon: "Bug" },
              { label: "Documentation", url: "https://github.com/farooqarahim/grove#readme", icon: "Docs" },
              { label: "License", url: "https://github.com/farooqarahim/grove/blob/main/LICENSE", icon: "Legal" },
            ].map((link) => (
              <a
                key={link.label}
                href={link.url}
                target="_blank"
                rel="noopener noreferrer"
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 10,
                  padding: "12px 14px",
                  borderRadius: 8,
                  background: C.surfaceHover,
                  border: `1px solid ${C.border}`,
                  textDecoration: "none",
                  color: C.text2,
                  fontSize: 12,
                  fontWeight: 500,
                  transition: "border-color 0.15s, background 0.15s",
                }}
                onMouseEnter={(e) => {
                  (e.currentTarget as HTMLAnchorElement).style.borderColor = C.borderHover;
                  (e.currentTarget as HTMLAnchorElement).style.background = C.surfaceActive;
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLAnchorElement).style.borderColor = C.border;
                  (e.currentTarget as HTMLAnchorElement).style.background = C.surfaceHover;
                }}
              >
                <span style={{ fontSize: 10, color: C.text4, fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.05em", minWidth: 36 }}>
                  {link.icon}
                </span>
                {link.label}
                <span style={{ marginLeft: "auto", color: C.text4, fontSize: 11 }}>&rarr;</span>
              </a>
            ))}
          </div>
        </div>
      </Card>

      {/* Footer */}
      <div style={{
        textAlign: "center",
        padding: "8px 0 24px",
        fontSize: 11,
        color: C.text4,
        lineHeight: 1.6,
      }}>
        Built with Rust, Tauri, and React.
        <br />
        Licensed under Apache 2.0.
      </div>
    </div>
  );
}
