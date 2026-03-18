export type ThreadItem =
    | { kind: "run_start"; runId: string; objective: string; pipeline: string; timestamp: string }
    | { kind: "user_message"; content: string; timestamp: string }
    | { kind: "agent_activity"; runId: string; sessionId: string; agentName: string; activities: ActivityEntry[]; isStreaming: boolean; costUsd: number | null }
    | { kind: "run_complete"; runId: string; state: string; costUsd: number; filesChanged: number; timestamp: string }
    | { kind: "phase_gate"; runId: string; phase: string; requiresApproval: boolean; timestamp: string; checkpointId: number; checkpointStatus: string; artifactPath: string | null; decidedAt: string | null }
    | { kind: "system_message"; content: string; level: "info" | "warn" | "error"; timestamp: string }
    | { kind: "agent_question"; runId: string; agentName: string; question: string; options: string[]; blocking: boolean; timestamp: string }
    | { kind: "user_answer"; runId: string; text: string; timestamp: string }
    | { kind: "artifact"; runId: string; agent: string; filename: string; sizeBytes: number; timestamp: string }
    | { kind: "verdict"; runId: string; outcome: string; summary: string; timestamp: string }
    | { kind: "scope_check"; runId: string; agent: string; passed: boolean;
        violations: { file: string; kind: string; pattern?: string }[];
        action: string; attempt: number; timestamp: string }
    | { kind: "graph_event"; graphId: string; title: string; status: string;
        runtimeStatus: string; closedSteps: number; totalSteps: number; timestamp: string };

export interface ActivityEntry {
    type: ActivityType;
    timestamp: number;
    tool?: string;
    text?: string;
    file?: string;
    linesAdded?: number;
    linesRemoved?: number;
    costUsd?: number;
    isError?: boolean;
    sessionId?: string;
    skillName?: string;
    question?: string;
    options?: string[];
    blocking?: boolean;
}

export type ActivityType = "system" | "assistant_text" | "tool_use" | "tool_result" | "result" | "raw_line" | "skill_loaded" | "question" | "user_answer";

export interface AgentOutputPayload {
    run_id: string;
    conversation_id?: string;
    event: StreamOutputEvent;
}

export type StreamOutputEvent =
    | { kind: "system"; message: string; session_id?: string }
    | { kind: "assistant_text"; text: string }
    | { kind: "tool_use"; tool: string }
    | { kind: "tool_result"; tool: string }
    | { kind: "result"; text: string; cost_usd?: number; is_error: boolean; session_id?: string }
    | { kind: "raw_line"; line: string }
    | { kind: "skill_loaded"; skill_name: string; skill_path: string }
    | { kind: "phase_start"; phase: string; run_id: string }
    | { kind: "phase_gate"; phase: string; run_id: string; requires_approval: boolean; checkpoint_id: number }
    | { kind: "phase_end"; phase: string; run_id: string; outcome: string }
    | { kind: "question"; question: string; options: string[]; blocking: boolean }
    | { kind: "user_answer"; text: string }
    | { kind: "scope_check_passed"; agent: string; artifact_count: number }
    | { kind: "scope_violation"; agent: string; violations: { file: string; kind: string; pattern?: string }[]; action: string; attempt: number }
    | { kind: "scope_retry"; agent: string; attempt: number; violation_summary: string };
