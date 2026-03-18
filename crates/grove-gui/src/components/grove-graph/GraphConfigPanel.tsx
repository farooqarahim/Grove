import { C, lbl } from "@/lib/theme";
import type { GraphConfig } from "@/types";

interface GraphConfigPanelProps {
  config: GraphConfig;
  onChange: (key: keyof GraphConfig, value: boolean) => void;
  /** When true, renders in a compact single-column layout */
  compact?: boolean;
}

interface CheckboxGroupProps {
  label: string;
  items: Array<{ key: keyof GraphConfig; label: string }>;
  config: GraphConfig;
  onChange: (key: keyof GraphConfig, value: boolean) => void;
  columns: number;
}

function CheckboxGroup({ label, items, config, onChange, columns }: CheckboxGroupProps) {
  return (
    <div>
      <div style={lbl}>{label}</div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: `repeat(${columns}, 1fr)`,
          gap: "6px 10px",
        }}
      >
        {items.map(({ key, label: itemLabel }) => (
          <label
            key={key}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 7,
              cursor: "pointer",
            }}
          >
            <input
              type="checkbox"
              checked={config[key]}
              onChange={(e) => onChange(key, e.target.checked)}
              style={{ accentColor: C.accent, width: 13, height: 13, cursor: "pointer" }}
            />
            <span style={{ fontSize: 12, color: "rgba(255,255,255,0.70)" }}>{itemLabel}</span>
          </label>
        ))}
      </div>
    </div>
  );
}

const DOC_ITEMS: Array<{ key: keyof GraphConfig; label: string }> = [
  { key: "doc_prd", label: "PRD" },
  { key: "doc_system_design", label: "System Design" },
  { key: "doc_guidelines", label: "Guidelines" },
];

const PLATFORM_ITEMS: Array<{ key: keyof GraphConfig; label: string }> = [
  { key: "platform_frontend", label: "Frontend" },
  { key: "platform_backend", label: "Backend" },
  { key: "platform_desktop", label: "Desktop" },
  { key: "platform_mobile", label: "Mobile" },
];

const ARCH_ITEMS: Array<{ key: keyof GraphConfig; label: string }> = [
  { key: "arch_tech_stack", label: "Tech Stack" },
  { key: "arch_saas", label: "SaaS" },
  { key: "arch_multiuser", label: "Multi-user" },
  { key: "arch_dlib", label: "DLib" },
];

const GIT_ITEMS: Array<{ key: keyof GraphConfig; label: string }> = [
  { key: "git_push", label: "Push to remote" },
  { key: "git_create_pr", label: "Create PR" },
];

export function GraphConfigPanel({ config, onChange, compact = false }: GraphConfigPanelProps) {
  const cols = compact ? 1 : 2;

  return (
    <div
      style={{
        background: C.surfaceHover,
        border: `1px solid ${C.border}`,
        borderRadius: 8,
        padding: "12px 14px",
        display: "flex",
        flexDirection: "column",
        gap: 14,
      }}
    >
      <CheckboxGroup label="Documents" items={DOC_ITEMS} config={config} onChange={onChange} columns={cols} />
      <div style={{ borderTop: `1px solid ${C.border}`, paddingTop: 12 }}>
        <CheckboxGroup label="Platforms" items={PLATFORM_ITEMS} config={config} onChange={onChange} columns={cols} />
      </div>
      <div style={{ borderTop: `1px solid ${C.border}`, paddingTop: 12 }}>
        <CheckboxGroup label="Architecture" items={ARCH_ITEMS} config={config} onChange={onChange} columns={cols} />
      </div>
      <div style={{ borderTop: `1px solid ${C.border}`, paddingTop: 12 }}>
        <CheckboxGroup label="Git" items={GIT_ITEMS} config={config} onChange={onChange} columns={cols} />
      </div>
    </div>
  );
}
