import { useState, useCallback, useRef, useEffect } from "react";
import { C } from "@/lib/theme";

interface ThreadInputProps {
    activeRunId: string | null;
    pendingQuestion: string | null;
    onNewRun: () => void;
    onSendAnswer: (runId: string, message: string) => void;
}

export function ThreadInput({ activeRunId, pendingQuestion, onNewRun, onSendAnswer }: ThreadInputProps) {
    const [text, setText] = useState("");
    const textareaRef = useRef<HTMLTextAreaElement>(null);

    const isAnswerMode = !!activeRunId && !!pendingQuestion;

    useEffect(() => {
        if (textareaRef.current) {
            textareaRef.current.style.height = "auto";
            textareaRef.current.style.height = Math.min(textareaRef.current.scrollHeight, 100) + "px";
        }
    }, [text]);

    const handleSubmit = useCallback(() => {
        const trimmed = text.trim();
        if (!trimmed) return;
        if (isAnswerMode && activeRunId) {
            onSendAnswer(activeRunId, trimmed);
        } else {
            onNewRun();
        }
        setText("");
    }, [text, isAnswerMode, activeRunId, onSendAnswer, onNewRun]);

    const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
        if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            handleSubmit();
        }
    }, [handleSubmit]);

    return (
        <div style={{
            padding: "10px 14px",
            borderTop: `1px solid ${C.border}`,
            background: C.surface,
        }}>
            {isAnswerMode && (
                <div style={{ fontSize: 10, fontWeight: 600, color: C.blue, marginBottom: 5, display: "flex", alignItems: "center", gap: 5 }}>
                    <span style={{ width: 5, height: 5, borderRadius: "50%", background: C.blue, display: "inline-block" }} />
                    agent is waiting
                </div>
            )}
            <div style={{ display: "flex", gap: 6, alignItems: "flex-end" }}>
                <textarea
                    ref={textareaRef}
                    value={text}
                    onChange={e => setText(e.target.value)}
                    onKeyDown={handleKeyDown}
                    placeholder={isAnswerMode ? "Answer..." : "Describe what to build..."}
                    rows={1}
                    style={{
                        flex: 1, resize: "none",
                        padding: "6px 10px", borderRadius: 3,
                        border: `1px solid ${C.border}`,
                        background: C.base, color: C.text1,
                        fontSize: 12, fontFamily: "inherit",
                        lineHeight: 1.5, outline: "none",
                    }}
                />
                <button
                    onClick={handleSubmit}
                    disabled={!text.trim() && isAnswerMode}
                    style={{
                        padding: "6px 12px", borderRadius: 3, border: "none",
                        background: isAnswerMode ? C.blue : C.accent,
                        color: "#fff", fontSize: 11, fontWeight: 600,
                        cursor: (!text.trim() && isAnswerMode) ? "not-allowed" : "pointer",
                        opacity: (!text.trim() && isAnswerMode) ? 0.5 : 1,
                        flexShrink: 0,
                    }}
                >
                    {isAnswerMode ? "Send" : "New Run"}
                </button>
            </div>
        </div>
    );
}
