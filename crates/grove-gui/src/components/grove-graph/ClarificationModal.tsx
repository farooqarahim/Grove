import { useState, useCallback, useEffect } from "react";
import { C } from "@/lib/theme";
import {
  checkGraphReadiness,
  submitClarificationAnswer,
  startGraphLoop,
} from "@/lib/api";

interface ClarificationQuestion {
  id: string;
  graph_id: string;
  question: string;
  answer: string | null;
  answered: boolean;
}

interface ClarificationModalProps {
  graphId: string;
  open: boolean;
  onClose: () => void;
  onStarted: () => void;
}

type ModalState =
  | { kind: "checking" }
  | { kind: "ready" }
  | { kind: "questions"; missingDocs: string[]; questions: ClarificationQuestion[]; answers: Record<string, string> }
  | { kind: "submitting" }
  | { kind: "error"; message: string };

export function ClarificationModal({ graphId, open, onClose, onStarted }: ClarificationModalProps) {
  const [state, setState] = useState<ModalState>({ kind: "checking" });

  const runCheck = useCallback(async () => {
    setState({ kind: "checking" });
    try {
      const result = await checkGraphReadiness(graphId);
      // Serde externally-tagged: unit variant is plain string "Ready",
      // struct variant is { NeedsClarification: { ... } }
      if (result === "Ready") {
        setState({ kind: "ready" });
        await startGraphLoop(graphId);
        onStarted();
      } else if (typeof result === "object" && result !== null && "NeedsClarification" in result) {
        const { missing_docs, questions } = result.NeedsClarification;
        const answers: Record<string, string> = {};
        for (const q of questions) {
          answers[q.id] = q.answer ?? "";
        }
        setState({ kind: "questions", missingDocs: missing_docs, questions, answers });
      } else {
        setState({ kind: "error", message: `Unexpected readiness result: ${JSON.stringify(result)}` });
      }
    } catch (e) {
      setState({ kind: "error", message: e instanceof Error ? e.message : String(e) });
    }
  }, [graphId, onStarted]);

  useEffect(() => {
    if (open) {
      void runCheck();
    }
  }, [open, runCheck]);

  const handleAnswerChange = useCallback((questionId: string, value: string) => {
    setState((prev) => {
      if (prev.kind !== "questions") return prev;
      return { ...prev, answers: { ...prev.answers, [questionId]: value } };
    });
  }, []);

  const handleSubmitAnswers = useCallback(async () => {
    if (state.kind !== "questions") return;
    const unanswered = state.questions.filter((q) => !q.answered);
    const allFilled = unanswered.every((q) => state.answers[q.id]?.trim());
    if (!allFilled) return;

    setState({ kind: "submitting" });
    try {
      for (const q of unanswered) {
        const answer = state.answers[q.id]?.trim();
        if (answer) {
          await submitClarificationAnswer(q.id, answer);
        }
      }
      await runCheck();
    } catch (e) {
      setState({ kind: "error", message: e instanceof Error ? e.message : String(e) });
    }
  }, [state, runCheck]);

  if (!open) return null;

  const isQuestions = state.kind === "questions";
  const unansweredQuestions = isQuestions
    ? state.questions.filter((q) => !q.answered)
    : [];
  const allFilled = isQuestions
    ? unansweredQuestions.every((q) => (state as Extract<ModalState, { kind: "questions" }>).answers[q.id]?.trim())
    : false;

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
        backdropFilter: "blur(6px)",
        WebkitBackdropFilter: "blur(6px)",
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 500,
          maxHeight: "80vh",
          background: C.surface,
          borderRadius: 10,
          border: `1px solid ${C.border}`,
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
        }}
      >
        {/* Header */}
        <div
          style={{
            padding: "14px 18px",
            background: C.surfaceHover,
            borderBottom: `1px solid ${C.border}`,
            fontSize: 13,
            fontWeight: 700,
            color: C.text1,
            display: "flex",
            alignItems: "center",
            gap: 8,
          }}
        >
          <svg width={14} height={14} viewBox="0 0 16 16" fill="none" stroke="#FACC15" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round">
            <circle cx={8} cy={8} r={7} />
            <line x1={8} y1={5} x2={8} y2={8.5} />
            <circle cx={8} cy={11} r={0.5} fill="#FACC15" />
          </svg>
          Readiness Check
        </div>

        {/* Body */}
        <div style={{ padding: "16px 18px", overflowY: "auto", flex: 1 }}>
          {(state.kind === "checking" || state.kind === "submitting") && (
            <div style={{ textAlign: "center", padding: "24px 0", color: "rgba(255,255,255,0.50)", fontSize: 12 }}>
              {state.kind === "checking" ? "Checking readiness..." : "Submitting answers..."}
            </div>
          )}

          {state.kind === "error" && (
            <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
              <div style={{ fontSize: 12, color: "#F87171" }}>{state.message}</div>
              <button
                onClick={() => void runCheck()}
                style={{
                  alignSelf: "flex-start",
                  padding: "6px 14px",
                  borderRadius: 5,
                  background: C.surfaceHover,
                  border: `1px solid ${C.border}`,
                  color: C.text1,
                  fontSize: 11,
                  fontWeight: 600,
                  cursor: "pointer",
                }}
              >
                Retry
              </button>
            </div>
          )}

          {state.kind === "questions" && (
            <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
              {/* Missing docs notice */}
              {state.missingDocs.length > 0 && (
                <div
                  style={{
                    padding: "10px 12px",
                    background: "rgba(250,204,21,0.06)",
                    border: "1px solid rgba(250,204,21,0.15)",
                    borderRadius: 6,
                    fontSize: 11,
                    color: "rgba(255,255,255,0.65)",
                    lineHeight: 1.6,
                  }}
                >
                  <div style={{ fontWeight: 600, color: "#FACC15", marginBottom: 4 }}>
                    Missing documentation
                  </div>
                  {state.missingDocs.map((doc) => (
                    <div key={doc} style={{ paddingLeft: 8 }}>
                      {doc}
                    </div>
                  ))}
                </div>
              )}

              {/* Questions */}
              <div style={{ fontSize: 11, color: "rgba(255,255,255,0.45)", fontWeight: 500 }}>
                Please answer the following to proceed:
              </div>
              {unansweredQuestions.map((q) => (
                <div key={q.id} style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  <label style={{ fontSize: 12, color: C.text1, fontWeight: 500, lineHeight: 1.5 }}>
                    {q.question}
                  </label>
                  <textarea
                    value={state.answers[q.id] ?? ""}
                    onChange={(e) => handleAnswerChange(q.id, e.target.value)}
                    rows={2}
                    style={{
                      width: "100%",
                      background: C.base,
                      border: `1px solid ${C.border}`,
                      borderRadius: 6,
                      padding: "8px 10px",
                      color: C.text1,
                      fontSize: 12,
                      resize: "none",
                      outline: "none",
                      lineHeight: 1.6,
                      boxSizing: "border-box",
                    }}
                  />
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Footer */}
        <div
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 8,
            padding: "12px 18px",
            background: C.base,
            borderTop: `1px solid ${C.border}`,
          }}
        >
          <button
            onClick={onClose}
            style={{
              padding: "6px 14px",
              borderRadius: 5,
              background: "transparent",
              border: "none",
              color: "rgba(255,255,255,0.50)",
              fontSize: 11,
              cursor: "pointer",
            }}
          >
            Cancel
          </button>
          {state.kind === "questions" && (
            <button
              onClick={() => void handleSubmitAnswers()}
              disabled={!allFilled}
              style={{
                padding: "6px 14px",
                borderRadius: 5,
                background: allFilled ? C.accent : C.surfaceHover,
                border: "none",
                color: allFilled ? "#fff" : "rgba(255,255,255,0.28)",
                fontSize: 11,
                fontWeight: 600,
                cursor: allFilled ? "pointer" : "default",
              }}
            >
              Submit & Recheck
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
