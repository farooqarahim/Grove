import { C } from "@/lib/theme";

interface VerdictBlockProps {
    outcome: string;
    summary: string;
    runId: string;
}

function verdictStyle(outcome: string): { color: string; bg: string; label: string } {
    const upper = outcome.toUpperCase();
    if (upper === "APPROVED" || upper === "PASS") {
        return { color: C.accent, bg: C.accentMuted, label: "APPROVED" };
    }
    if (upper === "NEEDS_WORK" || upper === "WARN") {
        return { color: C.warn, bg: C.warnDim, label: "NEEDS WORK" };
    }
    return { color: C.danger, bg: C.dangerDim, label: "REJECTED" };
}

export function VerdictBlock({ outcome, summary }: VerdictBlockProps) {
    const style = verdictStyle(outcome);
    return (
        <div style={{
            display: "flex", alignItems: "flex-start", gap: 8,
            margin: "3px 14px", padding: "7px 10px",
            background: style.bg,
            borderLeft: `2px solid ${style.color}`,
        }}>
            <span style={{ fontSize: 10, fontWeight: 700, color: style.color, flexShrink: 0, marginTop: 1 }}>
                {style.label}
            </span>
            <span style={{ fontSize: 12, color: C.text2, lineHeight: 1.5 }}>{summary}</span>
        </div>
    );
}
