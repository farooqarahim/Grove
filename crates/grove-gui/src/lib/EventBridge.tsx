/**
 * EventBridge — mounts once at the app root and subscribes to all Tauri push
 * events emitted by the Rust backend.
 *
 * When an event arrives, it calls queryClient.invalidateQueries() for the
 * relevant cache keys so TanStack Query refetches immediately — no polling lag.
 *
 * Polling intervals on queries are kept as a fallback safety net (e.g. 60 s)
 * but in the normal flow all UI updates are event-driven.
 */
import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { qk } from "@/lib/queryKeys";

export function EventBridge() {
  const queryClient = useQueryClient();

  useEffect(() => {
    const invalidateConversationCaches = (conversationId?: string) => {
      void queryClient.invalidateQueries({ queryKey: ["conversations"] });
      void queryClient.invalidateQueries({ queryKey: ["recentRuns"] });
      void queryClient.invalidateQueries({ queryKey: ["panelData"] });
      void queryClient.invalidateQueries({ queryKey: ["prStatus"] });

      if (conversationId) {
        void queryClient.invalidateQueries({
          queryKey: qk.runsForConversation(conversationId),
        });
        void queryClient.invalidateQueries({
          queryKey: qk.conversation(conversationId),
        });
        void queryClient.invalidateQueries({
          queryKey: qk.tasks(conversationId),
        });
      }
    };

    // grove://run-changed — a run started, progressed, completed, or was aborted.
    // Payload: { conversation_id?: string; run_id?: string; change_type?: string }
    //
    // When change_type is "progress", only run-level detail queries are invalidated.
    // For state transitions (started/completed/failed) or when change_type is absent
    // (backward compat with old events), broader list queries are also invalidated.
    const p1 = listen<{ conversation_id?: string; run_id?: string; change_type?: string }>(
      "grove://run-changed",
      ({ payload }) => {
        const isStateTransition = !payload.change_type || payload.change_type !== "progress";

        // Run-level detail panels — always invalidate on any change.
        if (payload.run_id) {
          void queryClient.invalidateQueries({ queryKey: ["sessions", payload.run_id] });
          void queryClient.invalidateQueries({ queryKey: ["subtasks", payload.run_id] });
          void queryClient.invalidateQueries({ queryKey: ["signals", payload.run_id] });
          void queryClient.invalidateQueries({ queryKey: ["events", payload.run_id] });
          void queryClient.invalidateQueries({ queryKey: ["checkpoints", payload.run_id] });
          void queryClient.invalidateQueries({ queryKey: ["panelData", "run", payload.run_id] });
        }

        // Broader list views — only on state transitions (started/completed/failed),
        // not on every progress tick.
        if (isStateTransition) {
          void queryClient.invalidateQueries({ queryKey: ["recentRuns"] });
          void queryClient.invalidateQueries({ queryKey: ["conversations"] });

          if (payload.conversation_id) {
            void queryClient.invalidateQueries({
              queryKey: ["runsForConversation", payload.conversation_id],
            });
            void queryClient.invalidateQueries({
              queryKey: ["conversation", payload.conversation_id],
            });
            void queryClient.invalidateQueries({
              queryKey: ["tasks", payload.conversation_id],
            });
          }
        }
      },
    );

    // grove://tasks-changed — a task was queued or cancelled.
    // Payload: { conversation_id?: string; task_id?: string }
    const p2 = listen<{ conversation_id?: string; task_id?: string }>(
      "grove://tasks-changed",
      ({ payload }) => {
        if (payload.conversation_id) {
          void queryClient.invalidateQueries({
            queryKey: ["tasks", payload.conversation_id],
          });
        } else {
          // Unknown conversation — invalidate all task queries
          void queryClient.invalidateQueries({ queryKey: ["tasks"] });
        }
      },
    );

    // grove://signals-changed — a signal was marked read (or new signal arrived).
    // Payload: { signal_id?: string; run_id?: string }
    const p3 = listen<{ signal_id?: string; run_id?: string }>(
      "grove://signals-changed",
      ({ payload }) => {
        if (payload.run_id) {
          void queryClient.invalidateQueries({
            queryKey: ["signals", payload.run_id],
          });
        } else {
          void queryClient.invalidateQueries({ queryKey: ["signals"] });
        }
      },
    );

    const p4 = listen<{ conversation_id?: string }>(
      "grove://conv-merged",
      ({ payload }) => invalidateConversationCaches(payload.conversation_id),
    );

    const p5 = listen<{ conversation_id?: string }>(
      "grove://conv-rebased",
      ({ payload }) => invalidateConversationCaches(payload.conversation_id),
    );

    // grove://agent-output — streaming output from an agent session.
    // Dispatched as a window CustomEvent so streaming buffer hooks can subscribe.
    // Phase gate events also trigger an immediate checkpoint cache invalidation
    // so the GUI shows the pending gate without waiting for a polling cycle.
    const p6 = listen<{ run_id: string; conversation_id: string; event: { kind: string; [key: string]: unknown } }>(
      "grove://agent-output",
      ({ payload }) => {
        window.dispatchEvent(new CustomEvent("grove-agent-output", { detail: payload }));

        // Fast-path: phase gate or phase end → invalidate checkpoints immediately.
        if (payload.event?.kind === "phase_gate" || payload.event?.kind === "phase_end") {
          void queryClient.invalidateQueries({ queryKey: ["checkpoints", payload.run_id] });
        }
      },
    );

    // grove://qa-message — a Q&A message was inserted (question detected or answer sent).
    const p7 = listen<{ run_id: string; direction: string }>(
      "grove://qa-message",
      ({ payload }) => {
        if (payload.run_id) {
          void queryClient.invalidateQueries({
            queryKey: ["qaMessages", payload.run_id],
          });
        }
      },
    );

    // grove://phase-gate-decided — user submitted a gate decision from PhaseGateBlock.
    // Invalidate checkpoints so the UI flips from pending → decided instantly.
    const p8 = listen<{ checkpoint_id: number; decision: string; run_id?: string }>(
      "grove://phase-gate-decided",
      ({ payload }) => {
        // Invalidate all checkpoint queries (we may not have run_id in the payload).
        if (payload.run_id) {
          void queryClient.invalidateQueries({ queryKey: ["checkpoints", payload.run_id] });
        } else {
          void queryClient.invalidateQueries({ queryKey: ["checkpoints"] });
        }
        // Also refresh sessions since pipeline progression may have changed.
        void queryClient.invalidateQueries({ queryKey: ["sessions"] });
      },
    );

    // grove://graphs-changed — a graph was created, updated, or loop state changed.
    // Payload: { graph_id?: string; conversation_id?: string }
    const p9 = listen<{ graph_id?: string; conversation_id?: string }>(
      "grove://graphs-changed",
      ({ payload }) => {
        if (payload.graph_id) {
          void queryClient.invalidateQueries({ queryKey: ["graphs", "detail", payload.graph_id] });
          void queryClient.invalidateQueries({ queryKey: ["graphs", "gitStatus", payload.graph_id] });
          void queryClient.invalidateQueries({ queryKey: ["graphs", "config", payload.graph_id] });
        }
        if (payload.conversation_id) {
          void queryClient.invalidateQueries({ queryKey: ["graphs", "list", payload.conversation_id] });
          void queryClient.invalidateQueries({ queryKey: ["graphs", "active", payload.conversation_id] });
        }
        // When only graph_id is present (no conversation_id), still invalidate active
        // graph queries so the UI picks up status changes.
        if (payload.graph_id && !payload.conversation_id) {
          void queryClient.invalidateQueries({ queryKey: ["graphs", "active"] });
          void queryClient.invalidateQueries({ queryKey: ["graphs", "list"] });
        }
        // Fallback: if no IDs provided, invalidate all graph queries.
        if (!payload.graph_id && !payload.conversation_id) {
          void queryClient.invalidateQueries({ queryKey: ["graphs"] });
        }
      },
    );

    // grove://graph-pipeline-complete — the graph planning pipeline finished
    // (phases/steps created). Invalidate detail + active graph queries.
    const p10 = listen<{ graph_id: string }>(
      "grove://graph-pipeline-complete",
      ({ payload }) => {
        const { graph_id } = payload;
        void queryClient.invalidateQueries({ queryKey: qk.graphDetail(graph_id) });
        void queryClient.invalidateQueries({ queryKey: qk.graphActive("") });
        // Also invalidate all active/list queries in case conversation_id is unknown
        void queryClient.invalidateQueries({ queryKey: ["graphs", "active"] });
        void queryClient.invalidateQueries({ queryKey: ["graphs", "list"] });
      },
    );

    // Cleanup: unsubscribe all listeners when the component unmounts (never in
    // practice since EventBridge is mounted at app root for the app lifetime).
    return () => {
      void Promise.all([p1, p2, p3, p4, p5, p6, p7, p8, p9, p10]).then((fns) => fns.forEach((fn) => fn()));
    };
  }, [queryClient]);

  return null;
}
