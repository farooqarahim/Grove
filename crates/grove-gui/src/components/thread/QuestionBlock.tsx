import { useState, useCallback } from "react";
import { C } from "@/lib/theme";

interface QuestionBlockProps {
    agentName: string;
    question: string;
    options: string[];
    blocking: boolean;
    onAnswer: (text: string) => void;
}

export function QuestionBlock({ agentName, question, options, blocking, onAnswer }: QuestionBlockProps) {
    const [selectedOption, setSelectedOption] = useState<string | null>(null);
    const [freeText, setFreeText] = useState("");
    const [submitted, setSubmitted] = useState(false);

    const handleSubmit = useCallback(() => {
        const answer = selectedOption ?? freeText.trim();
        if (!answer) return;
        setSubmitted(true);
        onAnswer(answer);
    }, [selectedOption, freeText, onAnswer]);

    return (
        <div style={{ background: "rgba(59,130,246,0.04)", borderLeft: `2px solid ${C.blue}`, padding: "7px 10px" }}>
            <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 6 }}>
                <span style={{ fontSize: 10, fontWeight: 600, color: C.blue }}>{agentName}</span>
                {blocking && (
                    <span style={{ fontSize: 9, fontWeight: 700, color: C.warn, letterSpacing: "0.04em" }}>BLOCKING</span>
                )}
            </div>

            <div style={{ fontSize: 12, color: C.text1, marginBottom: 8, lineHeight: 1.5 }}>{question}</div>

            {options.length > 0 && (
                <div style={{ display: "flex", flexDirection: "column", gap: 3, marginBottom: 7 }}>
                    {options.map((opt, i) => (
                        <label key={i} style={{
                            display: "flex", alignItems: "center", gap: 7, padding: "4px 7px",
                            cursor: submitted ? "default" : "pointer",
                            background: selectedOption === opt ? "rgba(255,255,255,0.07)" : "transparent",
                            borderRadius: 2, opacity: submitted ? 0.6 : 1,
                        }}>
                            <input
                                type="radio" name="question-option"
                                checked={selectedOption === opt}
                                onChange={() => { setSelectedOption(opt); setFreeText(""); }}
                                disabled={submitted}
                                style={{ accentColor: C.accent }}
                            />
                            <span style={{ fontSize: 12, color: C.text2 }}>{opt}</span>
                        </label>
                    ))}
                </div>
            )}

            <div style={{ display: "flex", gap: 6 }}>
                <input
                    type="text" value={freeText}
                    onChange={e => { setFreeText(e.target.value); setSelectedOption(null); }}
                    placeholder="Custom answer..."
                    disabled={submitted}
                    style={{ flex: 1, padding: "5px 8px", borderRadius: 2, border: "none", background: "rgba(0,0,0,0.25)", color: C.text1, fontSize: 12, outline: "none", fontFamily: "inherit" }}
                    onKeyDown={e => { if (e.key === "Enter") handleSubmit(); }}
                />
                <button
                    onClick={handleSubmit}
                    disabled={submitted || (!selectedOption && !freeText.trim())}
                    style={{
                        padding: "5px 12px", borderRadius: 2, border: "none",
                        background: C.accent, color: "#fff", fontSize: 11, fontWeight: 600, cursor: "pointer",
                        opacity: (submitted || (!selectedOption && !freeText.trim())) ? 0.5 : 1,
                    }}
                >
                    {submitted ? "Sent" : "Send"}
                </button>
            </div>
        </div>
    );
}
