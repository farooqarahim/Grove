import { useState, useCallback } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { C } from "@/lib/theme";
import {
  listAgentConfigs, saveAgentConfig, deleteAgentConfig,
  listPipelineConfigs, savePipelineConfig, deletePipelineConfig,
  listSkillConfigs, saveSkillConfig, deleteSkillConfig,
  previewAgentPrompt,
  type AgentConfigDto, type PipelineConfigDto, type SkillConfigDto,
} from "@/lib/api";

// ── Query keys ──────────────────────────────────────────────────────────────

const QK = {
  agents: ["studio", "agents"] as const,
  pipelines: ["studio", "pipelines"] as const,
  skills: ["studio", "skills"] as const,
};

// ── Shared primitives ───────────────────────────────────────────────────────

function StudioTabs({ active, onChange }: {
  active: "agents" | "pipelines" | "skills";
  onChange: (t: "agents" | "pipelines" | "skills") => void;
}) {
  const tabs = [
    { id: "agents" as const, label: "Agents" },
    { id: "pipelines" as const, label: "Pipelines" },
    { id: "skills" as const, label: "Skills" },
  ];
  return (
    <div style={{ display: "flex", gap: 2, marginBottom: 20 }}>
      {tabs.map((t) => (
        <button
          key={t.id}
          onClick={() => onChange(t.id)}
          style={{
            padding: "6px 16px",
            fontSize: 12,
            fontWeight: active === t.id ? 600 : 400,
            color: active === t.id ? C.text1 : C.text3,
            background: active === t.id ? C.surface : "transparent",
            border: `1px solid ${active === t.id ? C.border : "transparent"}`,
            borderRadius: 6,
            cursor: "pointer",
          }}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}

function Badge({ children, color }: { children: React.ReactNode; color?: string }) {
  return (
    <span style={{
      display: "inline-block",
      padding: "2px 8px",
      fontSize: 10,
      fontWeight: 500,
      borderRadius: 4,
      background: color ?? C.surface,
      color: C.text3,
      border: `1px solid ${C.border}`,
    }}>
      {children}
    </span>
  );
}

function TextArea({
  value, onChange, rows = 20, placeholder, mono = false,
}: {
  value: string;
  onChange: (v: string) => void;
  rows?: number;
  placeholder?: string;
  mono?: boolean;
}) {
  return (
    <textarea
      value={value}
      onChange={(e) => onChange(e.target.value)}
      rows={rows}
      placeholder={placeholder}
      spellCheck={false}
      style={{
        width: "100%",
        padding: 12,
        fontSize: 12,
        fontFamily: mono ? "monospace" : "inherit",
        lineHeight: 1.6,
        background: C.base,
        color: C.text1,
        border: `1px solid ${C.border}`,
        borderRadius: 6,
        resize: "vertical",
        outline: "none",
      }}
    />
  );
}

function Input({
  value, onChange, placeholder,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  return (
    <input
      type="text"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      style={{
        width: "100%",
        padding: "6px 10px",
        fontSize: 12,
        background: C.base,
        color: C.text1,
        border: `1px solid ${C.border}`,
        borderRadius: 6,
        outline: "none",
      }}
    />
  );
}

function FieldLabel({ children }: { children: React.ReactNode }) {
  return (
    <label style={{ display: "block", fontSize: 11, fontWeight: 600, color: C.text3, marginBottom: 4 }}>
      {children}
    </label>
  );
}

function ActionBtn({
  children, onClick, variant = "default", disabled = false,
}: {
  children: React.ReactNode;
  onClick: () => void;
  variant?: "default" | "primary" | "danger";
  disabled?: boolean;
}) {
  const bg = variant === "primary" ? C.accent : variant === "danger" ? "#e53e3e" : C.surface;
  const fg = variant === "primary" || variant === "danger" ? "#fff" : C.text2;
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      style={{
        padding: "5px 14px",
        fontSize: 11,
        fontWeight: 500,
        color: fg,
        background: bg,
        border: `1px solid ${variant === "default" ? C.border : bg}`,
        borderRadius: 6,
        cursor: disabled ? "not-allowed" : "pointer",
        opacity: disabled ? 0.5 : 1,
      }}
    >
      {children}
    </button>
  );
}

// ── Agents tab ──────────────────────────────────────────────────────────────

function AgentsTab() {
  const qc = useQueryClient();
  const { data: agents = [] } = useQuery({ queryKey: QK.agents, queryFn: listAgentConfigs });
  const [editing, setEditing] = useState<AgentConfigDto | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [previewObjective, setPreviewObjective] = useState("Fix the login page bug");
  const [saving, setSaving] = useState(false);

  const handleSave = useCallback(async () => {
    if (!editing) return;
    setSaving(true);
    try {
      await saveAgentConfig(editing);
      await qc.invalidateQueries({ queryKey: QK.agents });
      setEditing(null);
    } finally {
      setSaving(false);
    }
  }, [editing, qc]);

  const handlePreview = useCallback(async () => {
    if (!editing) return;
    try {
      // Save first so preview uses latest content
      await saveAgentConfig(editing);
      await qc.invalidateQueries({ queryKey: QK.agents });
      const result = await previewAgentPrompt(editing.id, previewObjective);
      setPreview(result);
    } catch (e) {
      setPreview(`Error: ${e}`);
    }
  }, [editing, previewObjective, qc]);

  const handleDelete = useCallback(async (id: string) => {
    await deleteAgentConfig(id);
    await qc.invalidateQueries({ queryKey: QK.agents });
    if (editing?.id === id) setEditing(null);
  }, [editing, qc]);

  if (editing) {
    return (
      <div>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 16 }}>
          <div style={{ fontSize: 14, fontWeight: 600, color: C.text1 }}>
            Editing: {editing.name}
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <ActionBtn onClick={() => { setEditing(null); setPreview(null); }}>Cancel</ActionBtn>
            <ActionBtn onClick={handleSave} variant="primary" disabled={saving}>
              {saving ? "Saving..." : "Save"}
            </ActionBtn>
          </div>
        </div>

        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12, marginBottom: 16 }}>
          <div>
            <FieldLabel>Name</FieldLabel>
            <Input value={editing.name} onChange={(v) => setEditing({ ...editing, name: v })} />
          </div>
          <div>
            <FieldLabel>ID</FieldLabel>
            <Input value={editing.id} onChange={(v) => setEditing({ ...editing, id: v })} />
          </div>
        </div>

        <div style={{ marginBottom: 12 }}>
          <FieldLabel>Description</FieldLabel>
          <Input
            value={editing.description}
            onChange={(v) => setEditing({ ...editing, description: v })}
          />
        </div>

        <div style={{ display: "flex", gap: 16, marginBottom: 12 }}>
          <label style={{ fontSize: 11, color: C.text2, display: "flex", alignItems: "center", gap: 4 }}>
            <input
              type="checkbox"
              checked={editing.can_write}
              onChange={(e) => setEditing({ ...editing, can_write: e.target.checked })}
            />
            Can Write
          </label>
          <label style={{ fontSize: 11, color: C.text2, display: "flex", alignItems: "center", gap: 4 }}>
            <input
              type="checkbox"
              checked={editing.can_run_commands}
              onChange={(e) => setEditing({ ...editing, can_run_commands: e.target.checked })}
            />
            Can Run Commands
          </label>
        </div>

        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12, marginBottom: 12 }}>
          <div>
            <FieldLabel>Artifact Template</FieldLabel>
            <Input
              value={editing.artifact ?? ""}
              onChange={(v) => setEditing({ ...editing, artifact: v || null })}
              placeholder="e.g. GROVE_PRD_{run_id}.md (or leave empty)"
            />
          </div>
          <div>
            <FieldLabel>Skills (comma-separated IDs)</FieldLabel>
            <Input
              value={editing.skills.join(", ")}
              onChange={(v) =>
                setEditing({ ...editing, skills: v.split(",").map((s) => s.trim()).filter(Boolean) })
              }
              placeholder="e.g. tdd, code-review"
            />
          </div>
        </div>

        <div style={{ marginBottom: 12 }}>
          <FieldLabel>Prompt (Markdown body)</FieldLabel>
          <TextArea
            value={editing.prompt}
            onChange={(v) => setEditing({ ...editing, prompt: v })}
            rows={24}
            mono
          />
        </div>

        {/* Preview section */}
        <div style={{
          padding: 12,
          background: C.base,
          borderRadius: 8,
          border: `1px solid ${C.border}`,
        }}>
          <div style={{ display: "flex", gap: 8, alignItems: "center", marginBottom: 8 }}>
            <FieldLabel>Preview with objective:</FieldLabel>
            <input
              type="text"
              value={previewObjective}
              onChange={(e) => setPreviewObjective(e.target.value)}
              style={{
                flex: 1,
                padding: "4px 8px",
                fontSize: 11,
                background: C.surface,
                color: C.text1,
                border: `1px solid ${C.border}`,
                borderRadius: 4,
                outline: "none",
              }}
            />
            <ActionBtn onClick={handlePreview} variant="primary">Preview Prompt</ActionBtn>
          </div>
          {preview && (
            <pre style={{
              maxHeight: 400,
              overflow: "auto",
              fontSize: 11,
              lineHeight: 1.5,
              color: C.text2,
              whiteSpace: "pre-wrap",
              wordBreak: "break-word",
              margin: 0,
              padding: 8,
              background: C.surface,
              borderRadius: 4,
            }}>
              {preview}
            </pre>
          )}
        </div>
      </div>
    );
  }

  return (
    <div>
      <div style={{ fontSize: 11, color: C.text4, marginBottom: 16 }}>
        Agent definitions loaded from <code>skills/agents/*.md</code>. Click to edit prompts, permissions, and skills.
      </div>
      {agents.map((a) => (
        <div
          key={a.id}
          onClick={() => { setEditing({ ...a }); setPreview(null); }}
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            padding: "10px 14px",
            marginBottom: 6,
            background: C.surface,
            border: `1px solid ${C.border}`,
            borderRadius: 8,
            cursor: "pointer",
          }}
        >
          <div>
            <div style={{ fontSize: 13, fontWeight: 600, color: C.text1 }}>{a.name}</div>
            <div style={{ fontSize: 11, color: C.text4, marginTop: 2 }}>{a.description}</div>
            <div style={{ display: "flex", gap: 6, marginTop: 6 }}>
              {a.can_write && <Badge>write</Badge>}
              {a.can_run_commands && <Badge>commands</Badge>}
              {a.artifact && <Badge color={C.base}>artifact: {a.artifact.split("_").slice(1, -1).join("_")}</Badge>}
              {a.skills.length > 0 && <Badge>{a.skills.length} skill{a.skills.length > 1 ? "s" : ""}</Badge>}
            </div>
          </div>
          <div onClick={(e) => e.stopPropagation()}>
            <ActionBtn onClick={() => void handleDelete(a.id)} variant="danger">
              Delete
            </ActionBtn>
          </div>
        </div>
      ))}
      {agents.length === 0 && (
        <div style={{ fontSize: 12, color: C.text4, padding: 20, textAlign: "center" }}>
          No agent configs found in <code>skills/agents/</code>
        </div>
      )}
    </div>
  );
}

// ── Pipelines tab ───────────────────────────────────────────────────────────

function PipelinesTab() {
  const qc = useQueryClient();
  const { data: pipelines = [] } = useQuery({ queryKey: QK.pipelines, queryFn: listPipelineConfigs });
  const [editing, setEditing] = useState<PipelineConfigDto | null>(null);
  const [saving, setSaving] = useState(false);

  const handleSave = useCallback(async () => {
    if (!editing) return;
    setSaving(true);
    try {
      await savePipelineConfig(editing);
      await qc.invalidateQueries({ queryKey: QK.pipelines });
      setEditing(null);
    } finally {
      setSaving(false);
    }
  }, [editing, qc]);

  const handleDelete = useCallback(async (id: string) => {
    await deletePipelineConfig(id);
    await qc.invalidateQueries({ queryKey: QK.pipelines });
    if (editing?.id === id) setEditing(null);
  }, [editing, qc]);

  if (editing) {
    return (
      <div>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 16 }}>
          <div style={{ fontSize: 14, fontWeight: 600, color: C.text1 }}>
            Editing: {editing.name}
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <ActionBtn onClick={() => setEditing(null)}>Cancel</ActionBtn>
            <ActionBtn onClick={handleSave} variant="primary" disabled={saving}>
              {saving ? "Saving..." : "Save"}
            </ActionBtn>
          </div>
        </div>

        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12, marginBottom: 12 }}>
          <div>
            <FieldLabel>Name</FieldLabel>
            <Input value={editing.name} onChange={(v) => setEditing({ ...editing, name: v })} />
          </div>
          <div>
            <FieldLabel>ID</FieldLabel>
            <Input value={editing.id} onChange={(v) => setEditing({ ...editing, id: v })} />
          </div>
        </div>

        <div style={{ marginBottom: 12 }}>
          <FieldLabel>Description</FieldLabel>
          <Input
            value={editing.description}
            onChange={(v) => setEditing({ ...editing, description: v })}
          />
        </div>

        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12, marginBottom: 12 }}>
          <div>
            <FieldLabel>Agents (comma-separated IDs, in order)</FieldLabel>
            <Input
              value={editing.agents.join(", ")}
              onChange={(v) =>
                setEditing({ ...editing, agents: v.split(",").map((s) => s.trim()).filter(Boolean) })
              }
              placeholder="e.g. builder, reviewer, judge"
            />
          </div>
          <div>
            <FieldLabel>Gates (agent IDs to pause after)</FieldLabel>
            <Input
              value={editing.gates.join(", ")}
              onChange={(v) =>
                setEditing({ ...editing, gates: v.split(",").map((s) => s.trim()).filter(Boolean) })
              }
              placeholder="e.g. build_prd, plan_system_design"
            />
          </div>
        </div>

        <div style={{ display: "flex", gap: 16, marginBottom: 12 }}>
          <label style={{ fontSize: 11, color: C.text2, display: "flex", alignItems: "center", gap: 4 }}>
            <input
              type="checkbox"
              checked={editing.default}
              onChange={(e) => setEditing({ ...editing, default: e.target.checked })}
            />
            Default Pipeline
          </label>
        </div>

        <div style={{ marginBottom: 12 }}>
          <FieldLabel>Content (Markdown description)</FieldLabel>
          <TextArea
            value={editing.content}
            onChange={(v) => setEditing({ ...editing, content: v })}
            rows={16}
            mono
          />
        </div>
      </div>
    );
  }

  return (
    <div>
      <div style={{ fontSize: 11, color: C.text4, marginBottom: 16 }}>
        Pipeline definitions loaded from <code>skills/pipelines/*.md</code>. Defines agent sequences and review gates.
      </div>
      {pipelines.map((p) => (
        <div
          key={p.id}
          onClick={() => setEditing({ ...p })}
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            padding: "10px 14px",
            marginBottom: 6,
            background: C.surface,
            border: `1px solid ${C.border}`,
            borderRadius: 8,
            cursor: "pointer",
          }}
        >
          <div>
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span style={{ fontSize: 13, fontWeight: 600, color: C.text1 }}>{p.name}</span>
              {p.default && <Badge color="#2b6cb0">default</Badge>}
            </div>
            <div style={{ fontSize: 11, color: C.text4, marginTop: 2 }}>{p.description}</div>
            <div style={{ display: "flex", gap: 6, marginTop: 6 }}>
              <Badge>{p.agents.length} agent{p.agents.length > 1 ? "s" : ""}</Badge>
              {p.gates.length > 0 && <Badge>{p.gates.length} gate{p.gates.length > 1 ? "s" : ""}</Badge>}
              <Badge color={C.base}>{p.agents.join(" → ")}</Badge>
            </div>
          </div>
          <div onClick={(e) => e.stopPropagation()}>
            <ActionBtn onClick={() => void handleDelete(p.id)} variant="danger">
              Delete
            </ActionBtn>
          </div>
        </div>
      ))}
      {pipelines.length === 0 && (
        <div style={{ fontSize: 12, color: C.text4, padding: 20, textAlign: "center" }}>
          No pipeline configs found in <code>skills/pipelines/</code>
        </div>
      )}
    </div>
  );
}

// ── Skills tab ──────────────────────────────────────────────────────────────

function SkillsTab() {
  const qc = useQueryClient();
  const { data: skills = [] } = useQuery({ queryKey: QK.skills, queryFn: listSkillConfigs });
  const [editing, setEditing] = useState<SkillConfigDto | null>(null);
  const [saving, setSaving] = useState(false);

  const handleSave = useCallback(async () => {
    if (!editing) return;
    setSaving(true);
    try {
      await saveSkillConfig(editing);
      await qc.invalidateQueries({ queryKey: QK.skills });
      setEditing(null);
    } finally {
      setSaving(false);
    }
  }, [editing, qc]);

  const handleDelete = useCallback(async (id: string) => {
    await deleteSkillConfig(id);
    await qc.invalidateQueries({ queryKey: QK.skills });
    if (editing?.id === id) setEditing(null);
  }, [editing, qc]);

  if (editing) {
    return (
      <div>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 16 }}>
          <div style={{ fontSize: 14, fontWeight: 600, color: C.text1 }}>
            Editing: {editing.name}
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <ActionBtn onClick={() => setEditing(null)}>Cancel</ActionBtn>
            <ActionBtn onClick={handleSave} variant="primary" disabled={saving}>
              {saving ? "Saving..." : "Save"}
            </ActionBtn>
          </div>
        </div>

        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12, marginBottom: 12 }}>
          <div>
            <FieldLabel>Name</FieldLabel>
            <Input value={editing.name} onChange={(v) => setEditing({ ...editing, name: v })} />
          </div>
          <div>
            <FieldLabel>ID</FieldLabel>
            <Input value={editing.id} onChange={(v) => setEditing({ ...editing, id: v })} />
          </div>
        </div>

        <div style={{ marginBottom: 12 }}>
          <FieldLabel>Description</FieldLabel>
          <Input
            value={editing.description}
            onChange={(v) => setEditing({ ...editing, description: v })}
          />
        </div>

        <div style={{ marginBottom: 12 }}>
          <FieldLabel>Applies To (agent IDs, comma-separated)</FieldLabel>
          <Input
            value={editing.applies_to.join(", ")}
            onChange={(v) =>
              setEditing({ ...editing, applies_to: v.split(",").map((s) => s.trim()).filter(Boolean) })
            }
            placeholder="e.g. builder, reviewer (empty = manual assignment only)"
          />
        </div>

        <div style={{ marginBottom: 12 }}>
          <FieldLabel>Content (Markdown)</FieldLabel>
          <TextArea
            value={editing.content}
            onChange={(v) => setEditing({ ...editing, content: v })}
            rows={24}
            mono
          />
        </div>
      </div>
    );
  }

  return (
    <div>
      <div style={{ fontSize: 11, color: C.text4, marginBottom: 16 }}>
        Skills loaded from <code>skills/*/SKILL.md</code>. Injected into agent prompts based on <code>applies_to</code> or agent's skill list.
      </div>
      {skills.map((s) => (
        <div
          key={s.id}
          onClick={() => setEditing({ ...s })}
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            padding: "10px 14px",
            marginBottom: 6,
            background: C.surface,
            border: `1px solid ${C.border}`,
            borderRadius: 8,
            cursor: "pointer",
          }}
        >
          <div>
            <div style={{ fontSize: 13, fontWeight: 600, color: C.text1 }}>{s.name}</div>
            <div style={{ fontSize: 11, color: C.text4, marginTop: 2 }}>{s.description}</div>
            <div style={{ display: "flex", gap: 6, marginTop: 6 }}>
              {s.applies_to.length > 0 ? (
                s.applies_to.map((a) => <Badge key={a}>{a}</Badge>)
              ) : (
                <Badge color={C.base}>manual</Badge>
              )}
              <Badge>{s.content.length} chars</Badge>
            </div>
          </div>
          <div onClick={(e) => e.stopPropagation()}>
            <ActionBtn onClick={() => void handleDelete(s.id)} variant="danger">
              Delete
            </ActionBtn>
          </div>
        </div>
      ))}
      {skills.length === 0 && (
        <div style={{ fontSize: 12, color: C.text4, padding: 20, textAlign: "center" }}>
          No skill configs found in <code>skills/</code>
        </div>
      )}
    </div>
  );
}

// ── Main export ─────────────────────────────────────────────────────────────

export function AgentStudioPanel() {
  const [tab, setTab] = useState<"agents" | "pipelines" | "skills">("agents");

  return (
    <div style={{ padding: "0 32px 32px" }}>
      <div style={{ marginBottom: 28 }}>
        <div style={{ fontSize: 18, fontWeight: 700, color: C.text1 }}>Agent Studio</div>
        <div style={{ fontSize: 12, color: C.text4, marginTop: 4 }}>
          Edit agent prompts, pipeline workflows, and skills. All configs are Markdown files in <code>skills/</code>.
        </div>
      </div>

      <StudioTabs active={tab} onChange={setTab} />

      {tab === "agents" && <AgentsTab />}
      {tab === "pipelines" && <PipelinesTab />}
      {tab === "skills" && <SkillsTab />}
    </div>
  );
}
