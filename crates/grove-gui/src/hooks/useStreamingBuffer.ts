import { useRef, useState, useCallback, useEffect } from "react";
import type { ActivityEntry, StreamOutputEvent } from "@/types/thread";

function toActivity(event: StreamOutputEvent): ActivityEntry {
    const ts = Date.now();
    switch (event.kind) {
        case "system":
            return { type: "system", timestamp: ts, text: event.message, sessionId: event.session_id };
        case "assistant_text":
            return { type: "assistant_text", timestamp: ts, text: event.text };
        case "tool_use":
            return { type: "tool_use", timestamp: ts, tool: event.tool };
        case "tool_result":
            return { type: "tool_result", timestamp: ts, tool: event.tool };
        case "result":
            return { type: "result", timestamp: ts, text: event.text, costUsd: event.cost_usd, isError: event.is_error, sessionId: event.session_id };
        case "raw_line":
            return { type: "raw_line", timestamp: ts, text: event.line };
        case "skill_loaded":
            return { type: "skill_loaded", timestamp: ts, skillName: event.skill_name, text: event.skill_path };
        case "question":
            return { type: "question", timestamp: ts, question: event.question, options: event.options, blocking: event.blocking, text: event.question };
        case "user_answer":
            return { type: "user_answer", timestamp: ts, text: event.text };
        case "scope_check_passed":
            return { type: "system", timestamp: ts, text: `${event.agent} scope check passed (${event.artifact_count} artifacts)` };
        case "scope_violation":
            return { type: "system", timestamp: ts, text: `${event.agent} scope violation: ${event.violations.length} file(s) [${event.action}]`, isError: true };
        case "scope_retry":
            return { type: "system", timestamp: ts, text: `${event.agent} retrying after scope violation (attempt ${event.attempt})` };
        default:
            return { type: "system", timestamp: ts, text: JSON.stringify(event) };
    }
}

export function useStreamingBuffer() {
    const bufferRef = useRef<Map<string, ActivityEntry[]>>(new Map());
    const [version, setVersion] = useState(0);

    useEffect(() => {
        const id = setInterval(() => setVersion(v => v + 1), 500);
        return () => clearInterval(id);
    }, []);

    const append = useCallback((runId: string, event: StreamOutputEvent) => {
        const list = bufferRef.current.get(runId) ?? [];
        list.push(toActivity(event));
        bufferRef.current.set(runId, list);
    }, []);

    const getActivities = useCallback((runId: string): ActivityEntry[] => {
        return bufferRef.current.get(runId) ?? [];
    }, []);

    const clear = useCallback((runId: string) => {
        bufferRef.current.delete(runId);
    }, []);

    return { append, getActivities, clear, version };
}
