import { useState, useEffect, useRef, useCallback } from "react";
import { C } from "@/lib/theme";
import { sendAgentMessage } from "@/lib/api";

interface QaCardProps {
  runId: string;
  question: string;
  options: string[];
  blocking: boolean;
  answered?: { text: string; by: "human" | "gatekeeper" };
  permissionMode: "skip_all" | "human_gate" | "autonomous_gate";
  gatekeeperSuggestion?: string;
  isRunActive: boolean;
}

const COUNTDOWN_S = 5;
const dot = (bg: string): React.CSSProperties => ({ width: 6, height: 6, borderRadius: "50%", background: bg, flexShrink: 0 });
const row: React.CSSProperties = { display: "flex", alignItems: "center", gap: 10, padding: "8px 14px", borderRadius: 8 };
const pill = (bg: string, fg: string): React.CSSProperties => ({
  padding: "3px 10px", borderRadius: 4, background: bg, color: fg, fontSize: 11, fontWeight: 600, cursor: "pointer",
});

export function QaCard({ runId, question, options, blocking, answered, permissionMode, gatekeeperSuggestion, isRunActive }: QaCardProps) {
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [countdown, setCountdown] = useState(COUNTDOWN_S);
  const [overridden, setOverridden] = useState(false);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const isAutoGate = permissionMode === "autonomous_gate" && !overridden;
  const showCountdown = isAutoGate && !answered && isRunActive && !!gatekeeperSuggestion;

  const clearTimer = () => { if (timerRef.current) { clearInterval(timerRef.current); timerRef.current = null; } };

  const send = useCallback(async (content: string) => {
    if (!content.trim() || sending) return;
    clearTimer();
    setSending(true);
    setError(null);
    try { await sendAgentMessage(runId, content.trim()); }
    catch (e) { setError(String(e)); }
    finally { setSending(false); }
  }, [runId, sending]);

  useEffect(() => {
    if (!showCountdown) return;
    setCountdown(COUNTDOWN_S);
    timerRef.current = setInterval(() => {
      setCountdown((p) => { if (p <= 1) { clearTimer(); return 0; } return p - 1; });
    }, 1000);
    return clearTimer;
  }, [showCountdown]);

  useEffect(() => {
    if (countdown === 0 && showCountdown && gatekeeperSuggestion) send(gatekeeperSuggestion);
  }, [countdown, showCountdown, gatekeeperSuggestion, send]);

  // Answered: collapsed green line
  if (answered) {
    const byLabel = answered.by === "human" ? "human" : "auto";
    return (
      <div style={{ ...row, background: C.surfaceHover, border: `1px solid ${C.accent}` }}>
        <div style={dot(C.accent)} />
        <span style={{ fontSize: 12, color: C.text3 }}>Answered: <strong style={{ color: C.text2 }}>{answered.text}</strong></span>
        <span style={{ marginLeft: "auto", fontSize: 10, fontWeight: 600, color: C.text4 }}>({byLabel})</span>
      </div>
    );
  }

  // Agent not running: greyed out
  if (!isRunActive) {
    return (
      <div style={{ ...row, background: C.surfaceHover, border: `1px solid ${C.border}`, opacity: 0.5 }}>
        <div style={dot(C.text4)} />
        <span style={{ fontSize: 12, color: C.text4, flex: 1 }}>{question}</span>
        <span style={{ fontSize: 10, fontWeight: 600, color: C.text4 }}>Unanswered</span>
      </div>
    );
  }

  // Active question card
  const hdrLabel = blocking ? "AGENT WAITING" : "AGENT ASKING";
  const hdrColor = blocking ? C.warn : C.blue;
  const bdr = blocking ? "rgba(245,158,11,0.3)" : "rgba(59,130,246,0.3)";
  const bg = blocking ? "rgba(245,158,11,0.04)" : "rgba(59,130,246,0.04)";
  const pct = showCountdown ? (countdown / COUNTDOWN_S) * 100 : 0;
  const dis = sending;

  return (
    <div style={{ borderRadius: 10, border: `1px solid ${bdr}`, background: bg, overflow: "hidden" }}>
      {/* Header */}
      <div style={{ padding: "10px 16px", display: "flex", alignItems: "center", gap: 10, borderBottom: `1px solid ${bdr}` }}>
        <div style={{ width: 8, height: 8, borderRadius: "50%", background: hdrColor, flexShrink: 0 }} />
        <span style={{ fontSize: 11, fontWeight: 700, letterSpacing: "0.06em", color: hdrColor }}>{hdrLabel}</span>
        <button onClick={() => send("[dismissed]")} disabled={dis}
          style={{ ...pill("rgba(255,255,255,0.06)", C.text4), marginLeft: "auto", fontSize: 10, fontWeight: 500 }}>
          Dismiss
        </button>
      </div>

      {/* Question */}
      <div style={{ padding: "10px 16px", fontSize: 13, color: C.text2, lineHeight: 1.5 }}>{question}</div>

      {/* Autonomous gate countdown */}
      {showCountdown && (
        <div style={{ padding: "0 16px 10px" }}>
          <div style={{ height: 3, borderRadius: 2, background: "rgba(255,255,255,0.06)", overflow: "hidden", marginBottom: 8 }}>
            <div style={{ width: `${pct}%`, height: "100%", background: C.blue, transition: "width 1s linear" }} />
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ flex: 1, fontSize: 12, color: C.text3 }}>
              Suggestion: <strong style={{ color: C.text2 }}>{gatekeeperSuggestion}</strong>
            </span>
            <span style={{ fontSize: 10, color: C.blue, fontWeight: 600 }}>Auto-sending in {countdown}s</span>
            <button onClick={() => { clearTimer(); setOverridden(true); }} style={pill("rgba(239,68,68,0.08)", C.danger)}>
              Override
            </button>
          </div>
        </div>
      )}

      {/* Option buttons */}
      {options.length > 0 && (
        <div style={{ padding: "0 16px 10px", display: "flex", flexWrap: "wrap", gap: 6 }}>
          {options.map((opt) => (
            <button key={opt} onClick={() => send(opt)} disabled={dis}
              style={{ padding: "6px 12px", borderRadius: 6, background: "rgba(255,255,255,0.06)",
                color: C.text2, fontSize: 12, fontWeight: 500, cursor: dis ? "default" : "pointer",
                opacity: dis ? 0.5 : 1, transition: "opacity 0.15s" }}>
              {opt}
            </button>
          ))}
        </div>
      )}

      {/* Custom input */}
      <div style={{ padding: "0 16px 12px", display: "flex", gap: 6 }}>
        <input value={input} onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(input); } }}
          placeholder="Type a reply..." disabled={dis}
          style={{ flex: 1, padding: "7px 10px", borderRadius: 6, border: `1px solid ${C.border}`,
            background: C.surface, color: C.text2, fontSize: 12, fontFamily: "inherit", outline: "none" }} />
        <button onClick={() => send(input)} disabled={dis || !input.trim()}
          style={{ padding: "7px 14px", borderRadius: 6, color: "#fff", fontSize: 12, fontWeight: 600,
            background: dis || !input.trim() ? C.accentDim : C.accent,
            cursor: dis || !input.trim() ? "default" : "pointer", transition: "background 0.15s" }}>
          {sending ? "Sending..." : "Send"}
        </button>
      </div>

      {error && <div style={{ padding: "0 16px 10px", fontSize: 11, color: C.danger }}>{error}</div>}
    </div>
  );
}
