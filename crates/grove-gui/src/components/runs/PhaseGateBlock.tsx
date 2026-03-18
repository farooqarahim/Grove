import { useState, useEffect } from "react";
import { C } from "@/lib/theme";
import { submitGateDecision, getArtifactContent, type PhaseCheckpointDto } from "@/lib/api";
import { formatRunAgentLabel } from "@/lib/runLabels";
import { useQueryClient } from "@tanstack/react-query";
import { qk } from "@/lib/queryKeys";

interface PhaseGateBlockProps {
  checkpoint: PhaseCheckpointDto;
  runId: string;
  pipeline?: string | null;
}

export function PhaseGateBlock({ checkpoint, runId, pipeline }: PhaseGateBlockProps) {
  const queryClient = useQueryClient();
  const [loading, setLoading] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showNote, setShowNote] = useState(false);
  const [noteText, setNoteText] = useState("");
  const [noteTarget, setNoteTarget] = useState<string>("approved_with_note");
  const [artifactContent, setArtifactContent] = useState<string | null>(null);
  const [artifactLoading, setArtifactLoading] = useState(false);

  const isPending = checkpoint.status === "pending";

  useEffect(() => {
    if (isPending && checkpoint.artifact_path && artifactContent === null) {
      setArtifactLoading(true);
      getArtifactContent(runId, checkpoint.artifact_path)
        .then(setArtifactContent)
        .catch(() => setArtifactContent("[Failed to load]"))
        .finally(() => setArtifactLoading(false));
    }
  }, [isPending, checkpoint.artifact_path, runId, artifactContent]);

  const agentLabel = formatRunAgentLabel(checkpoint.agent, pipeline);
  const note = checkpoint.decision?.trim() ?? "";
  const isAutoContinued = note === "auto-continued for this run";

  const handleDecision = async (decision: string, notes?: string) => {
    setLoading(decision);
    setError(null);
    try {
      await submitGateDecision(checkpoint.id, decision, notes);
      queryClient.invalidateQueries({ queryKey: qk.checkpoints(runId) });
      queryClient.invalidateQueries({ queryKey: qk.sessions(runId) });
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(null);
    }
  };

  const handleNoteSubmit = () => {
    const n = noteText.trim();
    if (n) handleDecision(noteTarget, n);
    else setShowNote(false);
  };

  const dis = loading !== null;

  // ── Decided: single compact row ──────────────────────────────────────────
  if (!isPending) {
    const decided = checkpoint.status;
    const color =
      decided === "approved" || decided === "approved_with_note" ? C.accent :
      decided === "rejected"     ? C.danger :
      decided === "retry"        ? C.blue :
      decided === "retry_resume" ? "#A855F7" : C.text4;

    const label =
      decided === "approved"           ? (isAutoContinued ? "auto-continued" : "continued") :
      decided === "approved_with_note" ? "continued with note" :
      decided === "rejected"           ? "aborted" :
      decided === "retry"              ? "retried" :
      decided === "retry_resume"       ? "revised" :
      decided === "skipped"            ? "skipped" : decided;

    return (
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{ width: 14, display: "inline-flex", justifyContent: "center", alignItems: "center", flexShrink: 0 }}>
          <span style={{ width: 5, height: 5, borderRadius: "50%", background: color, display: "inline-block" }} />
        </span>
        <span style={{ fontSize: 11, color: C.text4 }}>gate · {agentLabel}</span>
        <span style={{ fontSize: 10, fontWeight: 600, color, background: `${color}1A`, padding: "1px 5px", borderRadius: 2, letterSpacing: "0.04em", textTransform: "lowercase" }}>
          {label}
        </span>
        {note && !isAutoContinued && (
          <span style={{ fontSize: 11, color: C.text4, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {note}
          </span>
        )}
        {checkpoint.decided_at && (
          <span style={{ fontSize: 10, color: C.text4, marginLeft: "auto", flexShrink: 0, fontFamily: C.mono }}>
            {new Date(checkpoint.decided_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
          </span>
        )}
      </div>
    );
  }

  // ── Pending: interactive flat block ──────────────────────────────────────
  return (
    <div style={{ background: "rgba(245,158,11,0.05)", borderLeft: "2px solid rgba(245,158,11,0.5)" }}>

      {/* Header */}
      <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "6px 10px" }}>
        <span style={{ width: 6, height: 6, borderRadius: "50%", background: C.warn, flexShrink: 0, animation: "pulse 2s infinite" }} />
        <span style={{ fontSize: 12, fontWeight: 600, color: C.warn, flex: 1 }}>review {agentLabel} output</span>
        <span style={{ fontSize: 9, fontWeight: 700, color: C.warn, background: "rgba(245,158,11,0.15)", padding: "1px 5px", borderRadius: 2, letterSpacing: "0.06em" }}>
          WAITING
        </span>
      </div>

      {/* Artifact preview */}
      {checkpoint.artifact_path && (
        <div style={{ padding: "0 10px 6px" }}>
          <div style={{ fontSize: 10, color: C.text4, fontFamily: C.mono, marginBottom: 4 }}>
            {checkpoint.artifact_path}
          </div>
          <div style={{ padding: "8px 10px", background: "rgba(0,0,0,0.22)", maxHeight: 320, overflowY: "auto" }}>
            {artifactLoading ? (
              <span style={{ fontSize: 11, color: C.text4 }}>Loading...</span>
            ) : (
              <pre style={{ margin: 0, fontSize: 11, lineHeight: 1.5, fontFamily: C.mono, color: C.text2, whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
                {artifactContent ?? ""}
              </pre>
            )}
          </div>
        </div>
      )}

      {/* Note input */}
      {showNote && (
        <div style={{ padding: "0 10px 6px" }}>
          <div style={{ display: "flex", gap: 6 }}>
            <input
              value={noteText}
              onChange={e => setNoteText(e.target.value)}
              onKeyDown={e => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); handleNoteSubmit(); } }}
              placeholder="Feedback note..."
              disabled={dis}
              autoFocus
              style={{ flex: 1, padding: "5px 8px", borderRadius: 2, border: "none", background: "rgba(0,0,0,0.28)", color: C.text1, fontSize: 12, fontFamily: "inherit", outline: "none" }}
            />
            <button onClick={handleNoteSubmit} disabled={dis || !noteText.trim()} style={{ padding: "5px 10px", borderRadius: 2, border: "none", background: C.accent, color: "#fff", fontSize: 11, fontWeight: 600, cursor: "pointer", opacity: dis || !noteText.trim() ? 0.5 : 1 }}>
              {loading === noteTarget ? "..." : noteTarget === "approved_with_note" ? "Continue" : noteTarget === "retry" ? "Retry" : "Revise"}
            </button>
            <button onClick={() => { setShowNote(false); setNoteText(""); }} disabled={dis} style={{ padding: "5px 8px", borderRadius: 2, border: "none", background: "rgba(255,255,255,0.06)", color: C.text4, fontSize: 11, cursor: "pointer" }}>
              Cancel
            </button>
          </div>
        </div>
      )}

      {/* Action buttons */}
      <div style={{ display: "flex", flexWrap: "wrap", gap: 5, padding: "4px 10px 8px" }}>
        <GateBtn onClick={() => handleDecision("approved")} disabled={dis} loading={loading === "approved"} color={C.accent} colorAlpha="rgba(49,185,123,0.1)">
          {loading === "approved" ? "..." : "Continue"}
        </GateBtn>
        <GateBtn onClick={() => { setNoteTarget("approved_with_note"); setShowNote(true); }} disabled={dis} color={C.blue} colorAlpha="rgba(59,130,246,0.1)">
          Add Note
        </GateBtn>
        <GateBtn onClick={() => { setNoteTarget("retry"); setShowNote(true); }} disabled={dis} color={C.warn} colorAlpha="rgba(245,158,11,0.1)">
          Retry
        </GateBtn>
        <GateBtn onClick={() => { setNoteTarget("retry_resume"); setShowNote(true); }} disabled={dis} color="#A855F7" colorAlpha="rgba(168,85,247,0.1)">
          Revise
        </GateBtn>
        <GateBtn onClick={() => handleDecision("rejected")} disabled={dis} loading={loading === "rejected"} color={C.danger} colorAlpha="rgba(239,68,68,0.1)">
          {loading === "rejected" ? "..." : "Abort"}
        </GateBtn>
      </div>

      {error && (
        <div style={{ padding: "0 10px 6px", fontSize: 11, color: C.danger }}>{error}</div>
      )}
    </div>
  );
}

function GateBtn({ onClick, disabled, loading, color, colorAlpha, children }: {
  onClick: () => void;
  disabled: boolean;
  loading?: boolean;
  color: string;
  colorAlpha: string;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      style={{
        padding: "4px 10px", borderRadius: 2, border: "none",
        background: loading ? color + "55" : colorAlpha,
        color, fontSize: 11, fontWeight: 600,
        cursor: disabled ? "default" : "pointer",
        opacity: disabled && !loading ? 0.5 : 1,
      }}
    >
      {children}
    </button>
  );
}
