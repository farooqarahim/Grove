import { useRef, useEffect, useCallback, useMemo } from "react";
import { C } from "@/lib/theme";
import { useConversationThread } from "@/hooks/useConversationThread";
import { useStreamingBuffer } from "@/hooks/useStreamingBuffer";
import { sendAgentMessage } from "@/lib/api";
import { AgentActivityFeed } from "./AgentActivityFeed";
import { QuestionBlock } from "./QuestionBlock";
import { ArtifactSummary } from "./ArtifactSummary";
import { PhaseGateBlock } from "@/components/runs/PhaseGateBlock";
import { GraphCard } from "@/components/grove-graph/GraphCard";
import { StatusIcon } from "@/components/ui/icons";
import type { ThreadItem, ActivityEntry } from "@/types/thread";
import type { AgentOutputPayload, StreamOutputEvent } from "@/types/thread";

// ─────────────────────────────────────────────────────────────────────────────
// Design tokens — single source of truth for spacing
// ─────────────────────────────────────────────────────────────────────────────

/** Horizontal padding inside a run box body. */
const PH = 12;
/** Vertical padding for a compact single-line body row. */
const PV = 3;
/** Vertical padding for a block-level body item (question, pending gate, etc.) */
const PV_BLOCK = 6;

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

interface ConversationThreadProps {
    conversationId: string;
}

type RunBlock      = { kind: "run";        runId: string; items: ThreadItem[] };
type StandaloneBlock = { kind: "standalone"; item: ThreadItem };
type Block         = RunBlock | StandaloneBlock;

// ─────────────────────────────────────────────────────────────────────────────
// Shared layout helpers
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Every compact body row uses this layout:
 *   [14px indicator] · [content flex-1] · [optional time]
 *
 * The fixed-width indicator column keeps all rows left-aligned.
 */
function BodyRow({
    indicator,
    indicatorColor = C.text4,
    content,
    time,
    alignItems = "center",
}: {
    indicator: React.ReactNode;
    indicatorColor?: string;
    content: React.ReactNode;
    time?: string;
    alignItems?: React.CSSProperties["alignItems"];
}) {
    return (
        <div style={{ display: "flex", alignItems, gap: 8, padding: `${PV}px ${PH}px` }}>
            <span style={{
                width: 14, display: "inline-flex", justifyContent: "center",
                alignItems: "center", flexShrink: 0, color: indicatorColor,
            }}>
                {indicator}
            </span>
            <span style={{ flex: 1, minWidth: 0 }}>{content}</span>
            {time && (
                <span style={{ fontSize: 10, color: C.text4, fontFamily: C.mono, flexShrink: 0 }}>
                    {time}
                </span>
            )}
        </div>
    );
}

/** Block-level item (interactive / multi-line). Same horizontal gutter, more vertical room. */
function BlockRow({ children }: { children: React.ReactNode }) {
    return <div style={{ padding: `${PV_BLOCK}px ${PH}px` }}>{children}</div>;
}

function Dot({ color }: { color: string }) {
    return <span style={{ width: 6, height: 6, borderRadius: "50%", background: color, display: "inline-block" }} />;
}

function fmtTime(ts: string): string {
    return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

// ─────────────────────────────────────────────────────────────────────────────
// Grouping
// ─────────────────────────────────────────────────────────────────────────────

function buildBlocks(items: ThreadItem[]): Block[] {
    const blocks: Block[] = [];
    const runMap = new Map<string, ThreadItem[]>();

    for (const item of items) {
        if (
            item.kind === "user_message" ||
            item.kind === "system_message" ||
            item.kind === "graph_event"
        ) {
            blocks.push({ kind: "standalone", item });
            continue;
        }
        if ("runId" in item) {
            if (!runMap.has(item.runId)) {
                const list: ThreadItem[] = [];
                runMap.set(item.runId, list);
                blocks.push({ kind: "run", runId: item.runId, items: list });
            }
            runMap.get(item.runId)!.push(item);
        }
    }

    return blocks;
}

// ─────────────────────────────────────────────────────────────────────────────
// Verdict helper
// ─────────────────────────────────────────────────────────────────────────────

function verdictStyle(outcome: string) {
    const u = outcome.toUpperCase();
    if (u === "APPROVED" || u === "PASS")   return { color: C.accent,  label: "APPROVED"   };
    if (u === "NEEDS_WORK" || u === "WARN") return { color: C.warn,    label: "NEEDS WORK" };
    return { color: C.danger, label: "REJECTED" };
}

// ─────────────────────────────────────────────────────────────────────────────
// Run body row — all cases
// ─────────────────────────────────────────────────────────────────────────────

interface RunBodyRowProps {
    item: ThreadItem;
    runId: string;
    onSendAnswer: (runId: string, msg: string) => void;
}

function RunBodyRow({ item, runId, onSendAnswer }: RunBodyRowProps) {
    switch (item.kind) {

        // ── Agent session stub (no activities loaded yet) ──────────────────
        case "agent_activity": {
            if (item.activities.length === 0 && !item.isStreaming) {
                return (
                    <BodyRow
                        indicator={<Dot color={C.accent} />}
                        indicatorColor={C.accent}
                        content={
                            <span style={{ display: "flex", alignItems: "center", gap: 8 }}>
                                <span style={{ fontSize: 11, color: C.text3 }}>{item.agentName}</span>
                                {item.costUsd != null && item.costUsd > 0 && (
                                    <span style={{ fontSize: 10, color: C.text4, fontFamily: C.mono }}>
                                        ${item.costUsd.toFixed(4)}
                                    </span>
                                )}
                            </span>
                        }
                    />
                );
            }
            return (
                <AgentActivityFeed
                    agentName={item.agentName}
                    activities={item.activities}
                    isStreaming={item.isStreaming}
                    costUsd={item.costUsd}
                />
            );
        }

        // ── Phase gate ─────────────────────────────────────────────────────
        case "phase_gate": {
            const isPending = item.checkpointStatus === "pending";
            return (
                <div style={{ padding: isPending ? `${PV_BLOCK}px ${PH}px` : `${PV}px ${PH}px` }}>
                    <PhaseGateBlock
                        checkpoint={{
                            id: item.checkpointId,
                            run_id: runId,
                            agent: item.phase,
                            status: item.checkpointStatus,
                            decision: null,
                            decided_at: item.decidedAt,
                            artifact_path: item.artifactPath,
                            created_at: item.timestamp,
                        }}
                        runId={runId}
                    />
                </div>
            );
        }

        // ── Artifact ───────────────────────────────────────────────────────
        case "artifact":
            return (
                <BlockRow>
                    <ArtifactSummary
                        runId={runId}
                        agent={item.agent}
                        filename={item.filename}
                        sizeBytes={item.sizeBytes}
                    />
                </BlockRow>
            );

        // ── Agent question ─────────────────────────────────────────────────
        case "agent_question":
            return (
                <BlockRow>
                    <QuestionBlock
                        agentName={item.agentName}
                        question={item.question}
                        options={item.options}
                        blocking={item.blocking}
                        onAnswer={(text) => onSendAnswer(runId, text)}
                    />
                </BlockRow>
            );

        // ── User answer ────────────────────────────────────────────────────
        case "user_answer":
            return (
                <BodyRow
                    indicator={<span style={{ fontSize: 11, fontFamily: C.mono }}>▸</span>}
                    indicatorColor={C.blue}
                    content={
                        <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
                            <span style={{ fontSize: 10, fontWeight: 600, color: C.text4 }}>answer:</span>
                            <span style={{ fontSize: 12, color: C.text1 }}>{item.text}</span>
                        </span>
                    }
                    time={fmtTime(item.timestamp)}
                />
            );

        // ── Scope check ────────────────────────────────────────────────────
        case "scope_check": {
            if (item.passed) {
                return (
                    <BodyRow
                        indicator={<span style={{ fontSize: 11, fontFamily: C.mono }}>✓</span>}
                        indicatorColor={C.accent}
                        content={
                            <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
                                <span style={{ fontSize: 11, color: C.text4 }}>{item.agent}</span>
                                <span style={{ fontSize: 11, color: C.accent }}>scope ok</span>
                            </span>
                        }
                        time={fmtTime(item.timestamp)}
                    />
                );
            }
            const isFail = item.action === "hard_fail" || item.attempt > 1;
            const sc = isFail ? C.danger : C.warn;
            return (
                <BlockRow>
                    <div style={{ padding: "5px 8px", background: isFail ? "rgba(239,68,68,0.06)" : "rgba(245,158,11,0.06)" }}>
                        <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 3 }}>
                            <span style={{ fontSize: 11, fontWeight: 600, color: sc }}>{item.agent} scope violation</span>
                            {item.action === "retry_once" && item.attempt <= 1 && (
                                <span style={{ fontSize: 9, fontWeight: 600, color: sc, background: `${sc}1A`, padding: "1px 4px", borderRadius: 2 }}>
                                    retry {item.attempt + 1}/2
                                </span>
                            )}
                        </div>
                        {item.violations.map((v, vi) => (
                            <div key={vi} style={{ fontSize: 11, fontFamily: C.mono, color: C.text3 }}>
                                {v.file} <span style={{ color: C.text4 }}>({v.kind}{v.pattern ? `: ${v.pattern}` : ""})</span>
                            </div>
                        ))}
                    </div>
                </BlockRow>
            );
        }

        // ── Verdict ────────────────────────────────────────────────────────
        case "verdict": {
            const vs = verdictStyle(item.outcome);
            return (
                <BodyRow
                    indicator={
                        <span style={{
                            fontSize: 8, fontWeight: 700, color: vs.color,
                            background: `${vs.color}1A`, padding: "1px 4px",
                            borderRadius: 2, letterSpacing: "0.04em", whiteSpace: "nowrap",
                        }}>
                            {vs.label}
                        </span>
                    }
                    content={<span style={{ fontSize: 12, color: C.text2, lineHeight: 1.5 }}>{item.summary}</span>}
                    alignItems="flex-start"
                />
            );
        }

        case "run_start":
        case "run_complete":
            return null;

        default:
            return null;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Run box
// ─────────────────────────────────────────────────────────────────────────────

interface RunBoxProps {
    runId: string;
    items: ThreadItem[];
    runNumber: number;
    isActive: boolean;
    streamingActivities: ActivityEntry[];
    onSendAnswer: (runId: string, msg: string) => void;
}

function RunBox({ runId, items, runNumber, isActive, streamingActivities, onSendAnswer }: RunBoxProps) {
    const startItem   = items.find((i): i is Extract<ThreadItem, { kind: "run_start"    }> => i.kind === "run_start");
    const completeItem = items.find((i): i is Extract<ThreadItem, { kind: "run_complete" }> => i.kind === "run_complete");
    const bodyItems   = items.filter(i => i.kind !== "run_start" && i.kind !== "run_complete");

    const isFailed = completeItem?.state === "failed";
    const isDone   = !!completeItem;
    const hasBody  = bodyItems.length > 0 || streamingActivities.length > 0;

    const dotColor = isActive ? C.blue : isFailed ? C.danger : C.accent;

    const boxStyle: React.CSSProperties = isActive
        ? { background: "rgba(59,130,246,0.04)", borderLeft: `2px solid ${C.blue}` }
        : isFailed
            ? { background: "rgba(239,68,68,0.03)", borderLeft: `2px solid ${C.danger}` }
            : { background: "rgba(255,255,255,0.025)" };

    return (
        <div style={{ borderRadius: 3, ...boxStyle }}>

            {/* ── Header ── */}
            <div style={{
                display: "flex", alignItems: "center", gap: 8,
                padding: `8px ${PH}px`,
                background: "rgba(255,255,255,0.02)",
            }}>
                {/* State indicator — same 14px-wide column as body rows */}
                <span style={{ width: 14, display: "inline-flex", justifyContent: "center", alignItems: "center", flexShrink: 0, color: dotColor }}>
                    {isActive
                        ? <span style={{ display: "inline-block", animation: "spin 1s linear infinite" }}><StatusIcon status="executing" size={12} /></span>
                        : <Dot color={dotColor} />
                    }
                </span>

                {/* Run number */}
                <span style={{ fontSize: 11, fontFamily: C.mono, color: C.text4, flexShrink: 0 }}>#{runNumber}</span>

                {/* Objective */}
                <span style={{ fontSize: 13, color: C.text1, flex: 1, minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {startItem?.objective ?? runId.slice(0, 8)}
                </span>

                {/* Pipeline */}
                {startItem?.pipeline && (
                    <span style={{
                        fontSize: 9, fontWeight: 700, color: C.text4,
                        background: "rgba(255,255,255,0.05)", padding: "1px 5px",
                        borderRadius: 2, letterSpacing: "0.06em", textTransform: "uppercase", flexShrink: 0,
                    }}>
                        {startItem.pipeline}
                    </span>
                )}

                {/* LIVE */}
                {isActive && (
                    <span style={{ fontSize: 9, fontWeight: 700, color: C.blue, background: C.blueDim, padding: "1px 5px", borderRadius: 2, letterSpacing: "0.04em", flexShrink: 0 }}>
                        LIVE
                    </span>
                )}

                {/* Cost */}
                {completeItem && completeItem.costUsd > 0 && (
                    <span style={{ fontSize: 10, color: C.text4, fontFamily: C.mono, flexShrink: 0 }}>
                        ${completeItem.costUsd.toFixed(4)}
                    </span>
                )}

                {/* State badge */}
                {isDone && (
                    <span style={{
                        fontSize: 9, fontWeight: 700, flexShrink: 0,
                        color: isFailed ? C.danger : C.accent,
                        background: isFailed ? C.dangerDim : C.accentDim,
                        padding: "1px 5px", borderRadius: 2, letterSpacing: "0.04em", textTransform: "uppercase",
                    }}>
                        {completeItem.state}
                    </span>
                )}

                {/* Time */}
                {startItem && (
                    <span style={{ fontSize: 10, color: C.text4, fontFamily: C.mono, flexShrink: 0 }}>
                        {fmtTime(startItem.timestamp)}
                    </span>
                )}
            </div>

            {/* ── Body ── */}
            {hasBody && (
                <div style={{ paddingTop: 2, paddingBottom: 4 }}>
                    {bodyItems.map((item, i) => (
                        <RunBodyRow key={i} item={item} runId={runId} onSendAnswer={onSendAnswer} />
                    ))}
                    {streamingActivities.length > 0 && (
                        <AgentActivityFeed
                            agentName="Agent"
                            activities={streamingActivities}
                            isStreaming={true}
                            costUsd={null}
                        />
                    )}
                </div>
            )}
        </div>
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Standalone items
// ─────────────────────────────────────────────────────────────────────────────

function StandaloneRow({ item }: { item: ThreadItem }) {
    switch (item.kind) {
        case "user_message":
            return (
                <div style={{ display: "flex", alignItems: "flex-start", gap: 8, padding: "4px 0" }}>
                    <span style={{ fontSize: 13, color: C.accent, flexShrink: 0, fontFamily: C.mono, lineHeight: "20px" }}>›</span>
                    <span style={{ fontSize: 13, color: C.text1, lineHeight: "20px", whiteSpace: "pre-wrap", flex: 1 }}>
                        {item.content}
                    </span>
                    <span style={{ fontSize: 10, color: C.text4, flexShrink: 0, fontFamily: C.mono, lineHeight: "20px" }}>
                        {fmtTime(item.timestamp)}
                    </span>
                </div>
            );

        case "system_message": {
            const colorMap = { info: C.text4, warn: C.warn, error: C.danger };
            return (
                <div style={{ fontSize: 11, color: colorMap[item.level], fontFamily: C.mono, padding: "2px 0" }}>
                    {item.level !== "info" && (
                        <span style={{ fontWeight: 700, marginRight: 6 }}>[{item.level.toUpperCase()}]</span>
                    )}
                    {item.content}
                </div>
            );
        }

        case "graph_event":
            return (
                <GraphCard
                    title={item.title}
                    status={item.status}
                    runtimeStatus={item.runtimeStatus}
                    closedSteps={item.closedSteps}
                    totalSteps={item.totalSteps}
                    timestamp={item.timestamp}
                />
            );

        default:
            return null;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Main component
// ─────────────────────────────────────────────────────────────────────────────

export function ConversationThread({ conversationId }: ConversationThreadProps) {
    const { items, isLoading, activeRunId } = useConversationThread(conversationId);
    const streamingBuffer = useStreamingBuffer();
    const scrollRef = useRef<HTMLDivElement>(null);

    const runIdSet = useMemo(() => {
        const s = new Set<string>();
        if (activeRunId) s.add(activeRunId);
        return s;
    }, [activeRunId]);

    useEffect(() => {
        const handler = (e: Event) => {
            const detail = (e as CustomEvent<AgentOutputPayload>).detail;
            if (!detail || !runIdSet.has(detail.run_id)) return;
            streamingBuffer.append(detail.run_id, detail.event as StreamOutputEvent);
        };
        window.addEventListener("grove-agent-output", handler);
        return () => window.removeEventListener("grove-agent-output", handler);
    }, [runIdSet, streamingBuffer]);

    useEffect(() => {
        const el = scrollRef.current;
        if (!el) return;
        if (el.scrollHeight - el.scrollTop - el.clientHeight < 80) {
            el.scrollTop = el.scrollHeight;
        }
    }, [items, streamingBuffer.version]);

    const handleSendAnswer = useCallback(async (runId: string, message: string) => {
        await sendAgentMessage(runId, message);
    }, []);

    const blocks = useMemo(() => buildBlocks(items), [items]);
    const streamingActivities = activeRunId ? streamingBuffer.getActivities(activeRunId) : [];

    const runNumbers = useMemo(() => {
        const map = new Map<string, number>();
        let n = 0;
        for (const block of blocks) {
            if (block.kind === "run") map.set(block.runId, ++n);
        }
        return map;
    }, [blocks]);

    if (isLoading) {
        return (
            <div style={{ display: "flex", height: "100%", alignItems: "center", justifyContent: "center", background: C.base }}>
                <span style={{ fontSize: 11, color: C.text4 }}>Loading...</span>
            </div>
        );
    }

    return (
        <div
            ref={scrollRef}
            className="smooth-scroll"
            style={{
                height: "100%", overflowY: "auto",
                padding: "12px 16px",
                display: "flex", flexDirection: "column", gap: 8,
                background: C.base,
            }}
        >
                {blocks.length === 0 && streamingActivities.length === 0 && (
                    <div style={{ textAlign: "center", padding: "48px 0", fontSize: 12, color: C.text4 }}>
                        No activity yet.
                    </div>
                )}

                {blocks.map((block, i) => {
                    if (block.kind === "standalone") {
                        return <StandaloneRow key={i} item={block.item} />;
                    }
                    const isActive = block.runId === activeRunId;
                    return (
                        <RunBox
                            key={block.runId}
                            runId={block.runId}
                            items={block.items}
                            runNumber={runNumbers.get(block.runId) ?? 0}
                            isActive={isActive}
                            streamingActivities={isActive ? streamingActivities : []}
                            onSendAnswer={handleSendAnswer}
                        />
                    );
                })}
        </div>
    );
}
