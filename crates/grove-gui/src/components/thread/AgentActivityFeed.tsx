import { useState, useMemo } from "react";
import { C } from "@/lib/theme";
import type { ActivityEntry } from "@/types/thread";

interface AgentActivityFeedProps {
    agentName: string;
    activities: ActivityEntry[];
    isStreaming: boolean;
    costUsd: number | null;
}

function activityPrefix(type: string): string {
    switch (type) {
        case "tool_use":    return "→";
        case "tool_result": return "←";
        case "result":      return "✓";
        case "raw_line":    return "·";
        default:            return " ";
    }
}

function activityColor(type: string): string {
    switch (type) {
        case "assistant_text": return C.text2;
        case "tool_use":       return C.blue;
        case "tool_result":    return C.accent;
        case "result":         return C.accent;
        case "skill_loaded":   return C.warn;
        default:               return C.text4;
    }
}

function activityLabel(entry: ActivityEntry): string {
    switch (entry.type) {
        case "system":         return entry.text ?? "System";
        case "assistant_text": return entry.text ?? "";
        case "tool_use":       return entry.tool ?? "tool";
        case "tool_result":    return `${entry.tool ?? "tool"} done`;
        case "result":         return entry.text ?? "Done";
        case "raw_line":       return entry.text ?? "";
        case "skill_loaded":   return `skill: ${entry.skillName ?? ""}`;
        default:               return entry.text ?? "";
    }
}

function formatTime(ts: number): string {
    return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

export function AgentActivityFeed({ agentName, activities, isStreaming, costUsd }: AgentActivityFeedProps) {
    const [mode, setMode] = useState<"structured" | "raw">("structured");

    const rawLines = useMemo(() => activities.filter(a => a.type === "raw_line").map(a => a.text ?? ""), [activities]);

    return (
        <div>
            {/* Agent header */}
            <div style={{ display: "flex", alignItems: "center", gap: 7, padding: "4px 12px 2px" }}>
                <span style={{ fontSize: 11, fontWeight: 600, color: C.text3 }}>{agentName}</span>
                {isStreaming && (
                    <span style={{ fontSize: 9, fontWeight: 700, color: C.blue, background: C.blueDim, padding: "1px 5px", borderRadius: 2, letterSpacing: "0.04em" }}>
                        LIVE
                    </span>
                )}
                {costUsd != null && costUsd > 0 && (
                    <span style={{ fontSize: 10, color: C.text4, fontFamily: C.mono }}>${costUsd.toFixed(4)}</span>
                )}
                <div style={{ flex: 1 }} />
                {(["structured", "raw"] as const).map(m => (
                    <button key={m} onClick={() => setMode(m)} style={{
                        padding: "1px 5px", borderRadius: 2, border: "none", fontSize: 9, fontWeight: 600,
                        cursor: "pointer", background: mode === m ? "rgba(255,255,255,0.07)" : "transparent",
                        color: mode === m ? C.text2 : C.text4, textTransform: "uppercase", letterSpacing: "0.04em",
                    }}>
                        {m}
                    </button>
                ))}
            </div>

            {/* Activity rows — no max-height, continuous scroll with thread */}
            {mode === "structured" ? (
                activities.length === 0 ? (
                    <div style={{ padding: "2px 12px", fontSize: 11, color: C.text4 }}>waiting...</div>
                ) : (
                    activities.map((entry, i) => (
                        <div key={i} style={{ display: "flex", alignItems: "baseline", gap: 6, padding: "1px 12px" }}>
                            <span style={{ fontSize: 11, fontFamily: C.mono, color: activityColor(entry.type), flexShrink: 0, width: 14, textAlign: "center" }}>
                                {activityPrefix(entry.type)}
                            </span>
                            <span style={{ flex: 1, fontSize: 11, color: entry.isError ? C.danger : C.text3, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                                {activityLabel(entry)}
                            </span>
                            <span style={{ fontSize: 9, color: C.text4, flexShrink: 0, fontFamily: C.mono }}>
                                {formatTime(entry.timestamp)}
                            </span>
                        </div>
                    ))
                )
            ) : (
                <pre style={{ margin: 0, padding: "4px 12px", fontSize: 11, lineHeight: 1.5, fontFamily: C.mono, color: C.text3, whiteSpace: "pre-wrap", wordBreak: "break-all" }}>
                    {rawLines.length > 0 ? rawLines.join("\n") : "No raw output."}
                </pre>
            )}
        </div>
    );
}
