import { useMemo } from "react";
import { useQuery, useQueries } from "@tanstack/react-query";
import {
    listRunsForConversation,
    listMessages,
    listSessions,
    listPhaseCheckpoints,
    getRunArtifacts,
    listQaMessages,
    runEvents,
    listGraphs,
} from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { formatRunAgentLabel, formatRunPipelineLabel } from "@/lib/runLabels";
import type { ThreadItem } from "@/types/thread";
import type { SessionRecord } from "@/types";

const ACTIVE_STATES = ["executing", "waiting_for_gate", "planning", "verifying"];

/** Polling interval for per-run detail queries while a run is active. */
const ACTIVE_POLL_MS = 5000;

export function useConversationThread(conversationId: string | null) {
    const { data: runs } = useQuery({
        queryKey: qk.runsForConversation(conversationId),
        queryFn: () => listRunsForConversation(conversationId!),
        enabled: !!conversationId,
        refetchInterval: 10000,
    });

    const { data: messages } = useQuery({
        queryKey: qk.messages(conversationId ?? "", 200),
        queryFn: () => listMessages(conversationId!, 200),
        enabled: !!conversationId,
        refetchInterval: 10000,
    });

    // Derive active run early so per-run queries can use it for refetchInterval.
    const activeRunId = useMemo(() => {
        if (!runs) return null;
        const active = runs.find(r => ACTIVE_STATES.includes(r.state));
        return active?.id ?? null;
    }, [runs]);

    const hasActiveRun = activeRunId !== null;

    // Fetch per-run data for all runs in this conversation
    const runIds = useMemo(() => (runs ?? []).map(r => r.id), [runs]);

    const sessionQueries = useQueries({
        queries: runIds.map(runId => ({
            queryKey: qk.sessions(runId),
            queryFn: () => listSessions(runId),
            staleTime: hasActiveRun ? 3000 : 15000,
            refetchInterval: hasActiveRun ? ACTIVE_POLL_MS : false as const,
        })),
    });

    const checkpointQueries = useQueries({
        queries: runIds.map(runId => ({
            queryKey: qk.checkpoints(runId),
            queryFn: () => listPhaseCheckpoints(runId),
            staleTime: hasActiveRun ? 3000 : 15000,
            refetchInterval: hasActiveRun ? ACTIVE_POLL_MS : false as const,
        })),
    });

    const artifactQueries = useQueries({
        queries: runIds.map(runId => ({
            queryKey: qk.runArtifacts(runId),
            queryFn: () => getRunArtifacts(runId),
            staleTime: hasActiveRun ? 10000 : 30000,
            refetchInterval: hasActiveRun ? ACTIVE_POLL_MS : false as const,
        })),
    });

    const qaQueries = useQueries({
        queries: runIds.map(runId => ({
            queryKey: qk.qaMessages(runId),
            queryFn: () => listQaMessages(runId),
            staleTime: hasActiveRun ? 3000 : 15000,
            refetchInterval: hasActiveRun ? ACTIVE_POLL_MS : false as const,
        })),
    });

    const eventsQueries = useQueries({
        queries: runIds.map(runId => ({
            queryKey: qk.events(runId),
            queryFn: () => runEvents(runId),
            staleTime: hasActiveRun ? 3000 : 15000,
            refetchInterval: hasActiveRun ? ACTIVE_POLL_MS : false as const,
        })),
    });

    // Graph events for hive_loom conversations (lightweight — empty for non-graph conversations)
    const { data: graphs } = useQuery({
        queryKey: qk.graphs(conversationId ?? "__none__"),
        queryFn: () => listGraphs(conversationId!),
        enabled: !!conversationId,
        refetchInterval: 10_000,
        staleTime: 5_000,
    });

    const items = useMemo((): ThreadItem[] => {
        if (!runs) return [];

        const allItems: { ts: string; item: ThreadItem }[] = [];

        // User messages
        for (const msg of messages ?? []) {
            if (msg.role === "user") {
                allItems.push({
                    ts: msg.created_at,
                    item: { kind: "user_message", content: msg.content, timestamp: msg.created_at },
                });
            }
        }

        // Per-run items
        for (let ri = 0; ri < runs.length; ri++) {
            const run = runs[ri];
            const sessions: SessionRecord[] = sessionQueries[ri]?.data ?? [];
            const checkpoints = checkpointQueries[ri]?.data ?? [];
            const artifacts = artifactQueries[ri]?.data ?? [];
            const qaMessages = qaQueries[ri]?.data ?? [];

            // Run start
            allItems.push({
                ts: run.created_at,
                item: {
                    kind: "run_start",
                    runId: run.id,
                    objective: run.objective ?? "",
                    pipeline: formatRunPipelineLabel(run.pipeline),
                    timestamp: run.created_at,
                },
            });

            // Agent activity blocks from sessions
            for (const sess of sessions) {
                const agentLabel = formatRunAgentLabel(sess.agent_type, run.pipeline);
                allItems.push({
                    ts: sess.created_at,
                    item: {
                        kind: "agent_activity",
                        runId: run.id,
                        sessionId: sess.id,
                        agentName: agentLabel,
                        activities: [],
                        isStreaming: sess.state === "running",
                        costUsd: sess.cost_usd ?? null,
                    },
                });
            }

            // Phase gate checkpoints
            for (const cp of checkpoints) {
                allItems.push({
                    ts: cp.created_at,
                    item: {
                        kind: "phase_gate",
                        runId: run.id,
                        phase: formatRunAgentLabel(cp.agent, run.pipeline),
                        requiresApproval: cp.status === "pending",
                        timestamp: cp.created_at,
                        checkpointId: cp.id,
                        checkpointStatus: cp.status,
                        artifactPath: cp.artifact_path,
                        decidedAt: cp.decided_at,
                    },
                });
            }

            // Artifacts
            for (const art of artifacts) {
                allItems.push({
                    ts: art.created_at,
                    item: {
                        kind: "artifact",
                        runId: run.id,
                        agent: formatRunAgentLabel(art.agent, run.pipeline),
                        filename: art.filename,
                        sizeBytes: art.size_bytes,
                        timestamp: art.created_at,
                    },
                });
            }

            // Q&A messages
            for (const qa of qaMessages) {
                if (qa.direction === "question") {
                    allItems.push({
                        ts: qa.created_at,
                        item: {
                            kind: "agent_question",
                            runId: run.id,
                            agentName: "Agent",
                            question: qa.content,
                            options: qa.options_json ? JSON.parse(qa.options_json) : [],
                            blocking: true,
                            timestamp: qa.created_at,
                        },
                    });
                } else if (qa.direction === "answer") {
                    allItems.push({
                        ts: qa.created_at,
                        item: {
                            kind: "user_answer",
                            runId: run.id,
                            text: qa.content,
                            timestamp: qa.created_at,
                        },
                    });
                }
            }

            // Scope check events from run events table
            const events = eventsQueries[ri]?.data ?? [];
            for (const evt of events) {
                if (evt.event_type === "scope_check") {
                    const data = evt.payload as Record<string, unknown>;
                    allItems.push({
                        ts: evt.created_at,
                        item: {
                            kind: "scope_check",
                            runId: run.id,
                            agent: (data.agent as string) ?? "unknown",
                            passed: (data.passed as boolean) ?? false,
                            violations: (data.violations as { file: string; kind: string; pattern?: string }[]) ?? [],
                            action: (data.action as string) ?? "hard_fail",
                            attempt: (data.attempt as number) ?? 1,
                            timestamp: evt.created_at,
                        },
                    });
                }
            }

            // Run complete
            if (["completed", "failed"].includes(run.state)) {
                allItems.push({
                    ts: run.updated_at ?? run.created_at,
                    item: {
                        kind: "run_complete",
                        runId: run.id,
                        state: run.state,
                        costUsd: run.cost_used_usd ?? 0,
                        filesChanged: 0,
                        timestamp: run.updated_at ?? run.created_at,
                    },
                });
            }
        }

        // Graph events (creation/status changes)
        for (const g of graphs ?? []) {
            // Count closed steps from the graph record
            const closedSteps = g.steps_closed_count;
            allItems.push({
                ts: g.created_at,
                item: {
                    kind: "graph_event",
                    graphId: g.id,
                    title: g.title,
                    status: g.status,
                    runtimeStatus: g.runtime_status,
                    closedSteps,
                    totalSteps: g.steps_created_count,
                    timestamp: g.created_at,
                },
            });
        }

        allItems.sort((a, b) => a.ts.localeCompare(b.ts));
        return allItems.map(i => i.item);
    }, [runs, messages, sessionQueries, checkpointQueries, artifactQueries, qaQueries, eventsQueries, graphs]);

    return { items, isLoading: !runs, activeRunId };
}
