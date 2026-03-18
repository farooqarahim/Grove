import type React from "react";
import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { C, lbl } from "@/lib/theme";
import type { GraphDetail, AgentCatalogEntry } from "@/types";
import { createGraphSimple, getAgentCatalog, getDefaultProvider } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { XIcon } from "@/components/ui/icons";

interface CreateGraphModalProps {
  open: boolean;
  onClose: () => void;
  conversationId: string;
  onCreated: (detail: GraphDetail) => void;
}

export function CreateGraphModal({
  open,
  onClose,
  conversationId,
  onCreated,
}: CreateGraphModalProps) {
  const [objective, setObjective] = useState("");
  const [hasDocs, setHasDocs] = useState(false);
  const [docPaths, setDocPaths] = useState("");
  const [selectedAgent, setSelectedAgent] = useState<string>("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

  // Pre-select default agent when modal opens (if not already set).
  useEffect(() => {
    if (!open || selectedAgent) return;
    if (defaultProviderValue) {
      const isAvailable = (agentCatalog as AgentCatalogEntry[]).some(
        a => a.id === defaultProviderValue && a.detected && a.enabled,
      );
      if (isAvailable) {
        setSelectedAgent(defaultProviderValue);
      } else if (availableAgents.length > 0) {
        setSelectedAgent(availableAgents[0].id);
      }
    } else if (availableAgents.length > 0) {
      setSelectedAgent(availableAgents[0].id);
    }
  }, [open, defaultProviderValue, selectedAgent, agentCatalog, availableAgents]);

  // Reset form when modal closes
  useEffect(() => {
    if (!open) {
      setObjective("");
      setHasDocs(false);
      setDocPaths("");
      setSelectedAgent("");
      setError(null);
      setSubmitting(false);
    }
  }, [open]);

  // Escape to close
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open) return null;

  async function handleSubmit() {
    if (!objective.trim() || !selectedAgent) return;
    setSubmitting(true);
    setError(null);
    try {
      const detail = await createGraphSimple(
        conversationId,
        objective.trim(),
        hasDocs,
        hasDocs ? docPaths.trim() || null : null,
        selectedAgent,
      );
      onCreated(detail);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSubmitting(false);
    }
  }

  const canSubmit = !!objective.trim() && !!selectedAgent && !submitting;

  const inputStyle: React.CSSProperties = {
    width: "100%",
    background: C.base,
    border: `1px solid ${C.border}`,
    borderRadius: 6,
    padding: "8px 10px",
    color: C.text1,
    fontSize: 12,
    outline: "none",
    boxSizing: "border-box",
  };

  const selectStyle: React.CSSProperties = {
    width: "100%",
    background: C.base,
    border: `1px solid ${C.border}`,
    borderRadius: 6,
    padding: "7px 10px",
    color: C.text2,
    fontSize: 11,
    appearance: "none",
    cursor: "pointer",
    outline: "none",
  };

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 300,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.55)",
        backdropFilter: "blur(8px)",
        WebkitBackdropFilter: "blur(8px)",
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Create Graph"
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 480,
          background: C.surface,
          borderRadius: 10,
          overflow: "hidden",
          border: `1px solid ${C.border}`,
        }}
      >
        {/* Header */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "16px 20px",
            background: C.surfaceHover,
            borderBottom: `1px solid ${C.border}`,
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <div
              style={{
                width: 28,
                height: 28,
                borderRadius: 6,
                background: C.accentDim,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
              }}
            >
              <svg
                width={14}
                height={14}
                viewBox="0 0 16 16"
                fill="none"
                stroke={C.accent}
                strokeWidth={1.8}
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <circle cx={8} cy={4} r={2} />
                <circle cx={2} cy={12} r={2} />
                <circle cx={14} cy={12} r={2} />
                <line x1={8} y1={6} x2={8} y2={10} />
                <line x1={8} y1={10} x2={3} y2={10} />
                <line x1={8} y1={10} x2={13} y2={10} />
                <line x1={3} y1={10} x2={3} y2={12} />
                <line x1={13} y1={10} x2={13} y2={12} />
              </svg>
            </div>
            <div>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span style={{ fontSize: 14, fontWeight: 700, color: C.text1 }}>Create Graph</span>
                <span style={{
                  fontSize: 9, fontWeight: 700, letterSpacing: "0.04em",
                  textTransform: "uppercase", padding: "2px 6px", borderRadius: 999,
                  background: "rgba(248,113,113,0.1)", color: "#f87171",
                }}>
                  Experimental
                </span>
              </div>
              <div style={{ fontSize: 10, color: "rgba(255,255,255,0.45)" }}>
                Build an execution plan from an objective
              </div>
            </div>
          </div>
          <button
            onClick={onClose}
            aria-label="Close"
            style={{
              background: "none",
              border: "none",
              color: "rgba(255,255,255,0.45)",
              cursor: "pointer",
              padding: 4,
            }}
          >
            <XIcon size={12} />
          </button>
        </div>

        {/* Body */}
        <div
          style={{
            padding: 20,
            display: "flex",
            flexDirection: "column",
            gap: 18,
          }}
        >
          {/* Objective */}
          <div>
            <div style={lbl}>Objective</div>
            <textarea
              value={objective}
              onChange={(e) => setObjective(e.target.value)}
              placeholder="Describe what you want the graph to accomplish…"
              rows={5}
              autoFocus
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) void handleSubmit();
              }}
              style={{
                ...inputStyle,
                resize: "vertical",
                lineHeight: 1.6,
              }}
            />
          </div>

          {/* Coding Agent */}
          <div>
            <div style={lbl}>
              Coding Agent
              {!selectedAgent && (
                <span style={{ color: "#EF4444", marginLeft: 4 }}>*</span>
              )}
            </div>
            <select
              value={selectedAgent}
              onChange={e => setSelectedAgent(e.target.value)}
              style={{
                ...selectStyle,
                border: !selectedAgent ? "1px solid rgba(239,68,68,0.5)" : `1px solid ${C.border}`,
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
            {availableAgents.length === 0 && (
              <div style={{ fontSize: 10, color: "#EF4444", marginTop: 3 }}>
                No coding agents detected — install a CLI and enable it in Settings › Editors
              </div>
            )}
            {availableAgents.length > 0 && !selectedAgent && (
              <div style={{ fontSize: 10, color: "#EF4444", marginTop: 3 }}>
                Required — choose an agent to run this graph
              </div>
            )}
          </div>

          {/* Has existing docs toggle */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
            }}
          >
            <div>
              <div style={{ fontSize: 12, fontWeight: 600, color: C.text1 }}>
                Has existing docs?
              </div>
              <div style={{ fontSize: 11, color: "rgba(255,255,255,0.40)", marginTop: 2 }}>
                Provide paths to existing documentation files
              </div>
            </div>
            <button
              role="switch"
              aria-checked={hasDocs}
              onClick={() => setHasDocs((v) => !v)}
              style={{
                width: 36,
                height: 20,
                borderRadius: 10,
                border: "none",
                background: hasDocs ? C.accent : C.surfaceHover,
                cursor: "pointer",
                position: "relative",
                transition: "background 0.2s",
                flexShrink: 0,
                padding: 0,
              }}
            >
              <span
                style={{
                  position: "absolute",
                  top: 3,
                  left: hasDocs ? 19 : 3,
                  width: 14,
                  height: 14,
                  borderRadius: "50%",
                  background: "#fff",
                  transition: "left 0.2s",
                }}
              />
            </button>
          </div>

          {/* Document paths — only shown when hasDocs is true */}
          {hasDocs && (
            <div>
              <div style={lbl}>Document path(s)</div>
              <input
                type="text"
                value={docPaths}
                onChange={(e) => setDocPaths(e.target.value)}
                placeholder="/path/to/doc.md, /path/to/another.md"
                style={inputStyle}
              />
              <div
                style={{
                  fontSize: 10,
                  color: "rgba(255,255,255,0.35)",
                  marginTop: 5,
                }}
              >
                Comma-separated paths to documentation files
              </div>
            </div>
          )}

          {/* Error */}
          {error && (
            <div
              style={{
                fontSize: 11,
                color: "#F87171",
                background: "rgba(239,68,68,0.08)",
                border: "1px solid rgba(239,68,68,0.18)",
                borderRadius: 6,
                padding: "7px 10px",
              }}
            >
              {error}
            </div>
          )}
        </div>

        {/* Footer */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "flex-end",
            padding: "14px 20px",
            background: C.base,
            borderTop: `1px solid ${C.border}`,
            gap: 8,
          }}
        >
          <button
            onClick={onClose}
            style={{
              padding: "7px 16px",
              borderRadius: 6,
              background: "transparent",
              border: "none",
              color: "rgba(255,255,255,0.55)",
              fontSize: 11,
              fontWeight: 500,
              cursor: "pointer",
            }}
          >
            Cancel
          </button>
          <button
            onClick={() => void handleSubmit()}
            disabled={!canSubmit}
            style={{
              padding: "7px 16px",
              borderRadius: 6,
              background: canSubmit ? C.accent : C.surfaceHover,
              border: "none",
              color: canSubmit ? "#fff" : "rgba(255,255,255,0.30)",
              fontSize: 11,
              fontWeight: 700,
              cursor: canSubmit ? "pointer" : "default",
              display: "flex",
              alignItems: "center",
              gap: 6,
              transition: "background 0.15s",
            }}
          >
            {submitting ? (
              <span
                className="spinner"
                style={{ width: 11, height: 11, borderWidth: 1.5, borderTopColor: "#fff" }}
              />
            ) : (
              <svg
                width={11}
                height={11}
                viewBox="0 0 16 16"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <line x1={8} y1={1} x2={8} y2={15} />
                <line x1={1} y1={8} x2={15} y2={8} />
              </svg>
            )}
            {submitting ? "Creating…" : "Create Graph"}
          </button>
        </div>
      </div>
    </div>
  );
}
