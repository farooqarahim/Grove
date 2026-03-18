import { useState, useCallback } from "react";
import { C } from "@/lib/theme";
import { getArtifactContent } from "@/lib/api";

interface ArtifactSummaryProps {
    runId: string;
    agent: string;
    filename: string;
    sizeBytes: number;
}

function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1048576).toFixed(1)} MB`;
}

export function ArtifactSummary({ runId, agent, filename, sizeBytes }: ArtifactSummaryProps) {
    const [expanded, setExpanded] = useState(false);
    const [content, setContent] = useState<string | null>(null);
    const [loading, setLoading] = useState(false);

    const handleToggle = useCallback(async () => {
        if (expanded) { setExpanded(false); return; }
        if (content === null) {
            setLoading(true);
            try { setContent(await getArtifactContent(runId, filename)); }
            catch { setContent("[Failed to load]"); }
            finally { setLoading(false); }
        }
        setExpanded(true);
    }, [expanded, content, runId, filename]);

    return (
        <div>
            <button onClick={handleToggle} style={{
                display: "flex", alignItems: "center", gap: 8,
                width: "100%", padding: "2px 0",
                background: "transparent", border: "none", cursor: "pointer", textAlign: "left",
            }}>
                <span style={{ fontSize: 10, color: C.purple, fontFamily: C.mono, flexShrink: 0 }}>artifact</span>
                <span style={{ fontSize: 12, color: C.text1, flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {filename}
                </span>
                <span style={{ fontSize: 10, color: C.text4, fontFamily: C.mono, flexShrink: 0 }}>
                    {agent} · {formatBytes(sizeBytes)}
                </span>
                <span style={{ fontSize: 10, color: C.text4, flexShrink: 0, marginLeft: 4 }}>
                    {expanded ? "▲" : "▼"}
                </span>
            </button>
            {expanded && (
                <div style={{ padding: "6px 0 4px", maxHeight: 280, overflowY: "auto" }}>
                    {loading ? (
                        <div style={{ fontSize: 11, color: C.text4 }}>Loading...</div>
                    ) : (
                        <pre style={{ margin: 0, fontSize: 11, lineHeight: 1.5, fontFamily: C.mono, color: C.text2, whiteSpace: "pre-wrap", wordBreak: "break-all", background: "rgba(0,0,0,0.18)", padding: "8px 10px" }}>
                            {content}
                        </pre>
                    )}
                </div>
            )}
        </div>
    );
}
