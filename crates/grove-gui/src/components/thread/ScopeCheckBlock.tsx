import { C } from "@/lib/theme";

interface ScopeViolation {
    file: string;
    kind: string;
    pattern?: string;
}

interface ScopeCheckBlockProps {
    agent: string;
    passed: boolean;
    violations: ScopeViolation[];
    action: string;
    attempt: number;
}

export function ScopeCheckBlock({ agent, passed, violations, action, attempt }: ScopeCheckBlockProps) {
    if (passed) {
        return (
            <div style={{ display: "flex", alignItems: "center", gap: 6, padding: "3px 0", fontSize: 11, color: C.accent }}>
                <span style={{ fontFamily: C.mono }}>✓</span>
                <span style={{ color: C.text4 }}>{agent}</span>
                <span>scope ok</span>
            </div>
        );
    }

    const isFail = action === "hard_fail" || attempt > 1;
    const color = isFail ? C.danger : C.warn;
    const bg = isFail ? C.dangerDim : C.warnDim;

    return (
        <div style={{ padding: "7px 10px", background: bg, borderLeft: `2px solid ${color}` }}>
            <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 4 }}>
                <span style={{ fontSize: 11, fontWeight: 600, color }}>{agent} scope violation</span>
                {action === "retry_once" && attempt <= 1 && (
                    <span style={{ fontSize: 9, fontWeight: 600, color, background: `${color}1A`, padding: "1px 5px", borderRadius: 2 }}>
                        retry {attempt + 1}/2
                    </span>
                )}
            </div>
            <div style={{ fontSize: 11, color: C.text2, fontFamily: C.mono }}>
                {violations.map((v, i) => (
                    <div key={i}>
                        - {v.file} <span style={{ color: C.text4 }}>({v.kind}{v.pattern ? `: ${v.pattern}` : ""})</span>
                    </div>
                ))}
            </div>
            {action === "retry_once" && attempt <= 1 && (
                <div style={{ fontSize: 10, color, marginTop: 4 }}>Changes reverted, re-running...</div>
            )}
        </div>
    );
}
