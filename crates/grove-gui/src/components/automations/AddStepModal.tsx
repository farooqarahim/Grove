import { useState } from "react";
import { addAutomationStep } from "@/lib/api";
import { C, lbl } from "@/lib/theme";
import { XIcon } from "@/components/ui/icons";
import { useQueryClient } from "@tanstack/react-query";
import { qk } from "@/lib/queryKeys";

interface Props {
  open: boolean;
  automationId: string;
  existingStepKeys: string[];
  nextOrdinal: number;
  onClose: () => void;
}

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

const textareaStyle: React.CSSProperties = {
  ...inputStyle,
  height: 80,
  padding: "10px 12px",
  resize: "vertical",
  lineHeight: 1.5,
};

export function AddStepModal({ open, automationId, existingStepKeys, nextOrdinal, onClose }: Props) {
  const queryClient = useQueryClient();

  const [stepKey, setStepKey] = useState("");
  const [objective, setObjective] = useState("");
  const [dependsOn, setDependsOn] = useState<string[]>([]);
  const [provider, setProvider] = useState("");
  const [model, setModel] = useState("");
  const [condition, setCondition] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!open) return null;

  const canSubmit =
    stepKey.trim().length > 0 &&
    objective.trim().length > 0 &&
    !submitting;

  function reset() {
    setStepKey("");
    setObjective("");
    setDependsOn([]);
    setProvider("");
    setModel("");
    setCondition("");
    setSubmitting(false);
    setError(null);
  }

  function handleClose() {
    reset();
    onClose();
  }

  function toggleDep(key: string) {
    setDependsOn(prev =>
      prev.includes(key) ? prev.filter(d => d !== key) : [...prev, key],
    );
  }

  async function handleSubmit() {
    if (!canSubmit) return;
    setSubmitting(true);
    setError(null);

    try {
      await addAutomationStep({
        automationId,
        stepKey: stepKey.trim(),
        objective: objective.trim(),
        ordinal: nextOrdinal,
        dependsOnJson: dependsOn.length > 0 ? JSON.stringify(dependsOn) : undefined,
        provider: provider.trim() || undefined,
        model: model.trim() || undefined,
        condition: condition.trim() || undefined,
      });
      queryClient.invalidateQueries({ queryKey: qk.automationSteps(automationId) });
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
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 480,
          maxHeight: "85vh",
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
            Add Step
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
        <div style={{ padding: "20px 24px", display: "flex", flexDirection: "column", gap: 16 }}>
          {/* Step Key */}
          <div>
            <div style={lbl}>Step Key</div>
            <input
              value={stepKey}
              onChange={(e) => setStepKey(e.target.value.replace(/[^a-zA-Z0-9_-]/g, ""))}
              placeholder="scan"
              style={inputStyle}
              onFocus={(e) => { e.currentTarget.style.borderColor = C.accent; }}
              onBlur={(e) => { e.currentTarget.style.borderColor = C.border; }}
            />
            <div style={{ fontSize: 10, color: "#52575F", marginTop: 4 }}>
              Alphanumeric, hyphens, underscores only
            </div>
          </div>

          {/* Objective */}
          <div>
            <div style={lbl}>Objective</div>
            <textarea
              value={objective}
              onChange={(e) => setObjective(e.target.value)}
              placeholder="What should this step accomplish?"
              style={textareaStyle}
              onFocus={(e) => { e.currentTarget.style.borderColor = C.accent; }}
              onBlur={(e) => { e.currentTarget.style.borderColor = C.border; }}
            />
          </div>

          {/* Dependencies */}
          {existingStepKeys.length > 0 && (
            <div>
              <div style={lbl}>Depends On</div>
              <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
                {existingStepKeys.map(key => {
                  const active = dependsOn.includes(key);
                  return (
                    <button
                      key={key}
                      onClick={() => toggleDep(key)}
                      style={{
                        padding: "5px 12px",
                        borderRadius: 6,
                        border: `1px solid ${active ? "rgba(49,185,123,0.35)" : C.border}`,
                        background: active ? C.accentDim : C.surfaceHover,
                        color: active ? C.accent : "#64748b",
                        fontSize: 11,
                        fontWeight: 600,
                        cursor: "pointer",
                        fontFamily: "inherit",
                        transition: "all .12s",
                      }}
                      onMouseEnter={(e) => {
                        if (!active) e.currentTarget.style.borderColor = C.borderHover;
                      }}
                      onMouseLeave={(e) => {
                        e.currentTarget.style.borderColor = active ? "rgba(49,185,123,0.35)" : C.border;
                      }}
                    >
                      {key}
                    </button>
                  );
                })}
              </div>
            </div>
          )}

          {/* Condition */}
          <div>
            <div style={lbl}>Condition (optional)</div>
            <input
              value={condition}
              onChange={(e) => setCondition(e.target.value)}
              placeholder="steps.scan.state == 'completed'"
              style={{ ...inputStyle, fontFamily: C.mono, fontSize: 12 }}
              onFocus={(e) => { e.currentTarget.style.borderColor = C.accent; }}
              onBlur={(e) => { e.currentTarget.style.borderColor = C.border; }}
            />
          </div>

          {/* Provider + Model row */}
          <div style={{ display: "flex", gap: 12 }}>
            <div style={{ flex: 1 }}>
              <div style={lbl}>Provider (optional)</div>
              <input
                value={provider}
                onChange={(e) => setProvider(e.target.value)}
                placeholder="claude_code"
                style={inputStyle}
                onFocus={(e) => { e.currentTarget.style.borderColor = C.accent; }}
                onBlur={(e) => { e.currentTarget.style.borderColor = C.border; }}
              />
            </div>
            <div style={{ flex: 1 }}>
              <div style={lbl}>Model (optional)</div>
              <input
                value={model}
                onChange={(e) => setModel(e.target.value)}
                placeholder="claude-sonnet-4-6"
                style={inputStyle}
                onFocus={(e) => { e.currentTarget.style.borderColor = C.accent; }}
                onBlur={(e) => { e.currentTarget.style.borderColor = C.border; }}
              />
            </div>
          </div>
        </div>

        {/* Footer */}
        <div style={{ padding: "0 24px 20px", display: "flex", flexDirection: "column", gap: 12 }}>
          {error && (
            <div
              style={{
                padding: "8px 12px",
                borderRadius: 6,
                background: C.dangerDim,
                border: "1px solid rgba(239,68,68,0.3)",
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
              {submitting ? "Adding..." : "Add Step"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
