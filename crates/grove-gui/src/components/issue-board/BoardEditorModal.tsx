import { useEffect, useState } from "react";
import type { CanonicalStatus, IssueBoardColumnConfig, IssueBoardConfig, IssueTrackerStatus } from "@/types";
import { listProviderStatuses } from "@/lib/api";
import { CANONICAL_SEQUENCE, COLUMN_CONFIGS } from "./constants";
import { CloseIcon, PlusIcon } from "./Icons";

const EDIT_PROVIDERS = ["github", "jira", "linear", "grove"] as const;
type EditProvider = typeof EDIT_PROVIDERS[number];

const PROVIDER_COLORS: Record<EditProvider, string> = {
  github: "#94a3b8",
  jira: "#60a5fa",
  linear: "#a78bfa",
  grove: "#4ade80",
};

interface BoardEditorModalProps {
  open: boolean;
  config: IssueBoardConfig;
  saving: boolean;
  projectId: string | null;
  onClose: () => void;
  onChange: (config: IssueBoardConfig) => void;
  onSave: () => void;
}

export function BoardEditorModal({
  open, config, saving, projectId, onClose, onChange, onSave,
}: BoardEditorModalProps) {
  const [providerStatuses, setProviderStatuses] = useState<Record<string, IssueTrackerStatus[]>>({});
  const [loadingProvider, setLoadingProvider] = useState<string | null>(null);
  const [openPicker, setOpenPicker] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);

  useEffect(() => {
    if (!open) return;
    setProviderStatuses({});
    const load = async () => {
      for (const p of EDIT_PROVIDERS) {
        try {
          setLoadingProvider(p);
          const statuses = await listProviderStatuses(p, projectId ?? undefined);
          setProviderStatuses(prev => ({ ...prev, [p]: statuses }));
        } catch { /* provider not connected */ }
      }
      setLoadingProvider(null);
    };
    void load();
  }, [open, projectId]);

  // Close picker on outside click
  useEffect(() => {
    if (!openPicker) return;
    const handler = () => setOpenPicker(null);
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [openPicker]);

  if (!open) return null;

  const updateColumn = (index: number, updater: (col: IssueBoardColumnConfig) => IssueBoardColumnConfig) => {
    onChange({ columns: config.columns.map((c, i) => i === index ? updater(c) : c) });
  };

  const moveColumn = (index: number, dir: -1 | 1) => {
    const target = index + dir;
    if (target < 0 || target >= config.columns.length) return;
    const cols = [...config.columns];
    const [col] = cols.splice(index, 1);
    cols.splice(target, 0, col);
    onChange({ columns: cols });
  };

  const removeColumn = (index: number) => {
    onChange({ columns: config.columns.filter((_, i) => i !== index) });
  };

  const addColumn = () => {
    onChange({
      columns: [...config.columns, {
        id: `column_${Date.now()}`,
        label: "New Column",
        canonical_status: "open" as CanonicalStatus,
        match_rules: {},
        provider_targets: {},
      }],
    });
  };

  const addMatch = (colIndex: number, provider: string, statusId: string) => {
    updateColumn(colIndex, col => ({
      ...col,
      match_rules: {
        ...col.match_rules,
        [provider]: [...(col.match_rules?.[provider] ?? []), statusId],
      },
    }));
    setOpenPicker(null);
  };

  const removeMatch = (colIndex: number, provider: string, statusId: string) => {
    updateColumn(colIndex, col => ({
      ...col,
      match_rules: {
        ...col.match_rules,
        [provider]: (col.match_rules?.[provider] ?? []).filter(s => s !== statusId),
      },
    }));
  };

  const importFromProvider = async (provider: EditProvider) => {
    const statuses = providerStatuses[provider];
    if (!statuses || statuses.length === 0) return;
    setImporting(true);
    try {
      const newColumns: IssueBoardColumnConfig[] = statuses.map(s => ({
        id: `${provider}_${s.id}_${Date.now()}`,
        label: s.name,
        canonical_status: (
          s.category === "in_progress" ? "in_progress"
          : s.category === "done" ? "done"
          : s.category === "cancelled" ? "cancelled"
          : s.category === "backlog" ? "open"
          : "open"
        ) as CanonicalStatus,
        match_rules: { [provider]: [s.id] },
        provider_targets: { [provider]: s.id },
      }));
      onChange({ columns: newColumns });
    } finally {
      setImporting(false);
    }
  };

  const cellStyle: React.CSSProperties = {
    padding: "10px 12px",
    borderRight: "1px solid rgba(51,65,85,0.15)",
    verticalAlign: "top",
  };
  const headCellStyle: React.CSSProperties = {
    ...cellStyle,
    fontSize: 10, fontWeight: 700, color: "#475569", letterSpacing: "0.06em",
    textTransform: "uppercase" as const, background: "rgba(2,6,23,0.4)",
  };
  const inputStyle: React.CSSProperties = {
    background: "rgba(2,6,23,0.55)", border: "1px solid rgba(51,65,85,0.3)",
    borderRadius: 6, padding: "6px 9px", color: "#e2e8f0", fontSize: 12,
    fontFamily: "inherit", outline: "none", width: "100%", boxSizing: "border-box" as const,
  };

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed", inset: 0, zIndex: 1100,
        background: "rgba(2,6,23,0.72)", backdropFilter: "blur(6px)",
        display: "flex", alignItems: "center", justifyContent: "center", padding: 24,
      }}
    >
      <div
        onClick={e => e.stopPropagation()}
        style={{
          width: "min(1200px, 100%)", maxHeight: "88vh",
          background: "#0c1222", border: "1px solid rgba(51,65,85,0.35)",
          borderRadius: 16, boxShadow: "0 30px 80px rgba(0,0,0,0.45)",
          display: "flex", flexDirection: "column",
        }}
      >
        {/* Header */}
        <div style={{
          padding: "18px 24px", borderBottom: "1px solid rgba(51,65,85,0.2)",
          display: "flex", alignItems: "center", justifyContent: "space-between", gap: 16, flexShrink: 0,
        }}>
          <div>
            <div style={{ fontSize: 16, fontWeight: 700, color: "#f8fafc" }}>Edit Board</div>
            <div style={{ fontSize: 12, color: "#64748b", marginTop: 3 }}>
              Map provider statuses to columns. Click <strong style={{ color: "#94a3b8" }}>+</strong> to pick from real provider statuses.
              {loadingProvider && <span style={{ marginLeft: 8, color: "#475569" }}>Loading {loadingProvider}…</span>}
            </div>
          </div>
          <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
            <span style={{ fontSize: 11, color: "#475569", fontWeight: 600 }}>Import from:</span>
            {EDIT_PROVIDERS.map(p => (
              <button
                key={p}
                onClick={() => void importFromProvider(p)}
                disabled={importing || !providerStatuses[p]?.length}
                title={`Seed columns from ${p} statuses`}
                style={{
                  padding: "5px 10px", borderRadius: 7, fontSize: 11, fontWeight: 600,
                  border: "1px solid rgba(51,65,85,0.2)",
                  background: "rgba(51,65,85,0.1)",
                  color: providerStatuses[p]?.length ? PROVIDER_COLORS[p] : "#334155",
                  cursor: (providerStatuses[p]?.length && !importing) ? "pointer" : "default",
                  fontFamily: "inherit", opacity: importing ? 0.5 : 1,
                }}
              >{p}</button>
            ))}
            <button
              onClick={onClose}
              className="ib-close-btn"
              style={{
                background: "rgba(51,65,85,0.2)", border: "1px solid rgba(51,65,85,0.25)",
                borderRadius: 8, width: 32, height: 32, display: "flex", alignItems: "center",
                justifyContent: "center", color: "#64748b", cursor: "pointer", marginLeft: 8,
              }}
            ><CloseIcon /></button>
          </div>
        </div>

        {/* Table */}
        <div style={{ overflowY: "auto", flex: 1 }}>
          <table style={{ width: "100%", borderCollapse: "collapse", tableLayout: "fixed" as const }}>
            <colgroup>
              <col style={{ width: 48 }} />
              <col style={{ width: 150 }} />
              <col style={{ width: 130 }} />
              <col />
              <col />
              <col />
              <col />
              <col style={{ width: 48 }} />
            </colgroup>
            <thead>
              <tr style={{ borderBottom: "1px solid rgba(51,65,85,0.2)" }}>
                <th style={headCellStyle}>#</th>
                <th style={headCellStyle}>Column Name</th>
                <th style={headCellStyle}>Canonical</th>
                {EDIT_PROVIDERS.map(p => (
                  <th key={p} style={{ ...headCellStyle, color: PROVIDER_COLORS[p] }}>
                    {p.charAt(0).toUpperCase() + p.slice(1)}
                  </th>
                ))}
                <th style={{ ...headCellStyle, borderRight: "none" }}></th>
              </tr>
            </thead>
            <tbody>
              {config.columns.map((col, index) => (
                <tr key={`${col.id}-${index}`} style={{ borderBottom: "1px solid rgba(51,65,85,0.1)" }}>

                  {/* Index + move arrows */}
                  <td style={{ ...cellStyle, textAlign: "center" }}>
                    <div style={{ display: "flex", flexDirection: "column", gap: 1, alignItems: "center" }}>
                      <button
                        onClick={() => moveColumn(index, -1)}
                        disabled={index === 0}
                        style={{
                          background: "none", border: "none",
                          color: index === 0 ? "#1e293b" : "#475569",
                          cursor: index === 0 ? "default" : "pointer", fontSize: 11, lineHeight: 1, padding: "1px 4px",
                        }}
                      >▲</button>
                      <span style={{ fontSize: 11, color: "#334155", fontWeight: 600 }}>{index + 1}</span>
                      <button
                        onClick={() => moveColumn(index, 1)}
                        disabled={index === config.columns.length - 1}
                        style={{
                          background: "none", border: "none",
                          color: index === config.columns.length - 1 ? "#1e293b" : "#475569",
                          cursor: index === config.columns.length - 1 ? "default" : "pointer", fontSize: 11, lineHeight: 1, padding: "1px 4px",
                        }}
                      >▼</button>
                    </div>
                  </td>

                  {/* Column name */}
                  <td style={cellStyle}>
                    <input
                      value={col.label}
                      onChange={e => updateColumn(index, c => ({ ...c, label: e.target.value }))}
                      style={inputStyle}
                    />
                  </td>

                  {/* Canonical status */}
                  <td style={cellStyle}>
                    <select
                      value={col.canonical_status}
                      onChange={e => updateColumn(index, c => ({ ...c, canonical_status: e.target.value as CanonicalStatus }))}
                      style={inputStyle}
                    >
                      {CANONICAL_SEQUENCE.map(s => (
                        <option key={s} value={s}>{COLUMN_CONFIGS[s].label}</option>
                      ))}
                    </select>
                  </td>

                  {/* Provider match cells */}
                  {EDIT_PROVIDERS.map(provider => {
                    const matches = col.match_rules?.[provider] ?? [];
                    const available = (providerStatuses[provider] ?? []).filter(s => !matches.includes(s.id));
                    const pickerKey = `${index}:${provider}`;
                    const isPickerOpen = openPicker === pickerKey;

                    return (
                      <td key={provider} style={{ ...cellStyle, position: "relative" }}>
                        <div style={{ display: "flex", gap: 4, flexWrap: "wrap", alignItems: "center" }}>
                          {matches.map(m => {
                            const status = (providerStatuses[provider] ?? []).find(s => s.id === m);
                            return (
                              <span key={m} style={{
                                fontSize: 11, padding: "2px 7px", borderRadius: 5,
                                display: "flex", alignItems: "center", gap: 3,
                                background: `${PROVIDER_COLORS[provider]}15`,
                                border: `1px solid ${PROVIDER_COLORS[provider]}30`,
                                color: PROVIDER_COLORS[provider],
                              }}>
                                {status?.name ?? m}
                                <button
                                  onClick={() => removeMatch(index, provider, m)}
                                  style={{ background: "none", border: "none", color: "inherit", cursor: "pointer", fontSize: 11, lineHeight: 1, padding: 0, opacity: 0.7 }}
                                >×</button>
                              </span>
                            );
                          })}

                          {/* Picker trigger */}
                          <div style={{ position: "relative" }} onMouseDown={e => e.stopPropagation()}>
                            <button
                              onClick={() => setOpenPicker(isPickerOpen ? null : pickerKey)}
                              style={{
                                fontSize: 11, padding: "2px 7px", borderRadius: 5,
                                background: "rgba(51,65,85,0.15)", border: "1px dashed rgba(51,65,85,0.35)",
                                color: "#475569", cursor: "pointer", fontFamily: "inherit",
                              }}
                            >+</button>
                            {isPickerOpen && (
                              <div
                                onMouseDown={e => e.stopPropagation()}
                                style={{
                                  position: "absolute", top: "100%", left: 0, zIndex: 200,
                                  background: "#0f172a", border: "1px solid rgba(51,65,85,0.3)",
                                  borderRadius: 8, overflow: "auto", maxHeight: 200, minWidth: 180,
                                  boxShadow: "0 8px 24px rgba(0,0,0,0.4)",
                                }}
                              >
                                {available.length === 0 ? (
                                  <div style={{ padding: "10px 12px", fontSize: 12, color: "#475569" }}>
                                    {(providerStatuses[provider] ?? []).length === 0 ? "Not connected" : "All statuses added"}
                                  </div>
                                ) : available.map(s => (
                                  <button
                                    key={s.id}
                                    onClick={() => addMatch(index, provider, s.id)}
                                    style={{
                                      display: "flex", alignItems: "center", gap: 7, width: "100%",
                                      padding: "8px 12px", background: "transparent",
                                      border: "none", cursor: "pointer", fontFamily: "inherit",
                                      color: "#cbd5e1", fontSize: 12, textAlign: "left",
                                    }}
                                    onMouseEnter={e => { (e.currentTarget as HTMLButtonElement).style.background = "rgba(99,102,241,0.1)"; }}
                                    onMouseLeave={e => { (e.currentTarget as HTMLButtonElement).style.background = "transparent"; }}
                                  >
                                    {s.color && <div style={{ width: 7, height: 7, borderRadius: "50%", background: `#${s.color}`, flexShrink: 0 }} />}
                                    <span>{s.name}</span>
                                    <span style={{ fontSize: 10, color: "#475569", marginLeft: "auto" }}>{s.category}</span>
                                  </button>
                                ))}
                              </div>
                            )}
                          </div>
                        </div>
                      </td>
                    );
                  })}

                  {/* Delete */}
                  <td style={{ ...cellStyle, borderRight: "none", textAlign: "center" }}>
                    <button
                      onClick={() => removeColumn(index)}
                      disabled={config.columns.length === 1}
                      style={{
                        background: "none", border: "none",
                        color: config.columns.length === 1 ? "#1e293b" : "#ef4444",
                        cursor: config.columns.length === 1 ? "default" : "pointer",
                        fontSize: 16, lineHeight: 1, padding: "2px 4px",
                      }}
                    >×</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        {/* Footer */}
        <div style={{
          padding: "14px 20px", borderTop: "1px solid rgba(51,65,85,0.2)",
          display: "flex", justifyContent: "space-between", gap: 12, flexShrink: 0,
          background: "rgba(2,6,23,0.3)",
        }}>
          <button
            onClick={addColumn}
            style={{
              display: "flex", alignItems: "center", gap: 6, padding: "8px 14px",
              borderRadius: 9, border: "1px solid rgba(59,130,246,0.18)",
              background: "rgba(59,130,246,0.08)", color: "#93c5fd",
              fontSize: 12, fontWeight: 600, cursor: "pointer", fontFamily: "inherit",
            }}
          ><PlusIcon /> Add Column</button>
          <div style={{ display: "flex", gap: 8 }}>
            <button onClick={onClose} style={{
              padding: "8px 14px", borderRadius: 9,
              border: "1px solid rgba(51,65,85,0.25)", background: "rgba(15,23,42,0.6)",
              color: "#cbd5e1", fontSize: 12, fontWeight: 600, cursor: "pointer", fontFamily: "inherit",
            }}>Cancel</button>
            <button
              onClick={onSave}
              disabled={saving}
              style={{
                padding: "8px 16px", borderRadius: 9,
                border: "1px solid rgba(49,185,123,0.28)",
                background: "linear-gradient(135deg,#31B97B,#269962)", color: "#fff",
                fontSize: 12, fontWeight: 700, cursor: saving ? "default" : "pointer",
                opacity: saving ? 0.6 : 1, fontFamily: "inherit",
              }}
            >{saving ? "Saving…" : "Save Board"}</button>
          </div>
        </div>
      </div>
    </div>
  );
}
