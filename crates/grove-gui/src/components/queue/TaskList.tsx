import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Tag } from "@/components/ui/badge";
import { statusColor, Dot, XIcon, Refresh } from "@/components/ui/icons";
import { truncate, relativeTime } from "@/lib/hooks";
import { qk } from "@/lib/queryKeys";
import {
  listTasksForConversation,
  cancelTask,
  deleteTask,
  retryTask,
  clearQueue,
  refreshQueue,
} from "@/lib/api";
import { C } from "@/lib/theme";
import type { TaskRecord } from "@/types";

interface TaskListProps {
  conversationId: string | null;
}

// ── Task Detail Modal ─────────────────────────────────────────────────────────

function TaskDetailModal({
  task,
  onClose,
  onRetry,
  onDelete,
}: {
  task: TaskRecord;
  onClose: () => void;
  onRetry: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const sc = statusColor(task.state);
  const isFailed = task.state === "failed";
  const isTerminal = ["failed", "completed", "cancelled"].includes(task.state);

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 9999,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.6)",
        backdropFilter: "blur(4px)",
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        style={{
          background: C.surface,
          borderRadius: 12,
          border: `1px solid ${C.border}`,
          width: 480,
          maxHeight: "80vh",
          overflow: "auto",
          padding: 24,
        }}
      >
        {/* Header */}
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "flex-start",
            marginBottom: 16,
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <Dot status={task.state} size={8} />
            <span
              style={{
                fontSize: 13,
                fontWeight: 600,
                color: sc.text,
                textTransform: "capitalize",
              }}
            >
              {task.state}
            </span>
          </div>
          <button
            onClick={onClose}
            style={{
              background: "none",
              border: "none",
              color: C.text4,
              cursor: "pointer",
              fontSize: 16,
              padding: "0 4px",
            }}
          >
            <XIcon size={14} />
          </button>
        </div>

        {/* Objective */}
        <div style={{ marginBottom: 16 }}>
          <div
            style={{
              fontSize: 10,
              color: C.text4,
              textTransform: "uppercase",
              letterSpacing: 0.5,
              marginBottom: 6,
            }}
          >
            Objective
          </div>
          <div
            style={{
              fontSize: 12,
              color: "#DDE0E7",
              lineHeight: 1.6,
              background: "rgba(255,255,255,0.02)",
              borderRadius: 6,
              padding: "10px 12px",
              whiteSpace: "pre-wrap",
              wordBreak: "break-word",
            }}
          >
            {task.objective}
          </div>
        </div>

        {/* Details grid */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "1fr 1fr",
            gap: "10px 16px",
            marginBottom: 16,
          }}
        >
          <Detail label="Task ID" value={task.id} mono />
          {task.run_id && <Detail label="Run ID" value={task.run_id} mono />}
          {task.model && (
            <Detail
              label="Model"
              value={task.model.replace("claude-", "").replace("-20251001", "")}
            />
          )}
          <Detail label="Queued" value={relativeTime(task.queued_at)} />
          {task.started_at && (
            <Detail label="Started" value={relativeTime(task.started_at)} />
          )}
          {task.completed_at && (
            <Detail label="Completed" value={relativeTime(task.completed_at)} />
          )}
          {task.priority > 0 && (
            <Detail label="Priority" value={`P${task.priority}`} />
          )}
          {task.publish_status && (
            <Detail label="Publish" value={task.publish_status} />
          )}
        </div>

        {/* Error details */}
        {task.publish_error && (
          <div style={{ marginBottom: 16 }}>
            <div
              style={{
                fontSize: 10,
                color: C.text4,
                textTransform: "uppercase",
                letterSpacing: 0.5,
                marginBottom: 6,
              }}
            >
              Error
            </div>
            <div
              style={{
                fontSize: 11,
                color: C.danger,
                background: "rgba(239,68,68,0.06)",
                borderRadius: 6,
                padding: "10px 12px",
                lineHeight: 1.5,
                fontFamily: C.mono,
                whiteSpace: "pre-wrap",
                wordBreak: "break-word",
              }}
            >
              {task.publish_error}
            </div>
          </div>
        )}

        {/* PR link */}
        {task.pr_url && (
          <div style={{ marginBottom: 16 }}>
            <div
              style={{
                fontSize: 10,
                color: C.text4,
                textTransform: "uppercase",
                letterSpacing: 0.5,
                marginBottom: 6,
              }}
            >
              Pull Request
            </div>
            <a
              href={task.pr_url}
              target="_blank"
              rel="noreferrer"
              style={{
                fontSize: 11,
                color: C.accent,
                textDecoration: "none",
              }}
            >
              {task.pr_url}
            </a>
          </div>
        )}

        {/* Actions */}
        <div
          style={{
            display: "flex",
            gap: 8,
            justifyContent: "flex-end",
            paddingTop: 12,
            borderTop: `1px solid ${C.border}`,
          }}
        >
          {isTerminal && (
            <button
              onClick={() => onDelete(task.id)}
              style={{
                padding: "7px 14px",
                borderRadius: 6,
                background: "rgba(239,68,68,0.08)",
                border: "none",
                color: "#EF4444",
                fontSize: 11,
                fontWeight: 500,
                cursor: "pointer",
              }}
            >
              Delete
            </button>
          )}
          {isFailed && (
            <button
              onClick={() => onRetry(task.id)}
              style={{
                padding: "7px 14px",
                borderRadius: 6,
                background: C.accent,
                border: "none",
                color: "#fff",
                fontSize: 11,
                fontWeight: 600,
                cursor: "pointer",
              }}
            >
              Retry
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function Detail({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div>
      <div
        style={{
          fontSize: 10,
          color: C.text4,
          textTransform: "uppercase",
          letterSpacing: 0.5,
          marginBottom: 2,
        }}
      >
        {label}
      </div>
      <div
        style={{
          fontSize: 11,
          color: "#DDE0E7",
          fontFamily: mono ? C.mono : undefined,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
        title={value}
      >
        {value}
      </div>
    </div>
  );
}

// ── Main TaskList ─────────────────────────────────────────────────────────────

export function TaskList({ conversationId }: TaskListProps) {
  const queryClient = useQueryClient();
  const { data: tasks, refetch } = useQuery({
    queryKey: qk.tasks(conversationId),
    queryFn: () => listTasksForConversation(conversationId!),
    enabled: !!conversationId,
    refetchInterval: 60000,
    staleTime: 30000,
  });

  const [confirmId, setConfirmId] = useState<string | null>(null);
  const [cancellingId, setCancellingId] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [refreshMsg, setRefreshMsg] = useState<string | null>(null);
  const [selectedTask, setSelectedTask] = useState<TaskRecord | null>(null);

  const handleRefresh = async () => {
    setRefreshing(true);
    setRefreshMsg(null);
    try {
      const reconciled = await refreshQueue();
      refetch();
      if (reconciled > 0) {
        setRefreshMsg(
          `Fixed ${reconciled} stale task${reconciled > 1 ? "s" : ""}`,
        );
      } else {
        setRefreshMsg("Queue is up to date");
      }
      setTimeout(() => setRefreshMsg(null), 3000);
    } catch {
      setRefreshMsg("Refresh failed");
      setTimeout(() => setRefreshMsg(null), 3000);
    } finally {
      setRefreshing(false);
    }
  };

  const handleCancel = async (id: string) => {
    if (confirmId !== id) {
      setConfirmId(id);
      setTimeout(() => setConfirmId(null), 3000);
      return;
    }
    setConfirmId(null);
    setCancellingId(id);
    try {
      await cancelTask(id);
      refetch();
    } catch {
      // Task may already be running
    } finally {
      setCancellingId(null);
    }
  };

  const handleClearQueue = async () => {
    try {
      const cleared = await clearQueue();
      refetch();
      if (cleared > 0) {
        setRefreshMsg(`Cleared ${cleared} task${cleared > 1 ? "s" : ""}`);
      } else {
        setRefreshMsg("Nothing to clear");
      }
      setTimeout(() => setRefreshMsg(null), 3000);
    } catch {
      setRefreshMsg("Clear failed");
      setTimeout(() => setRefreshMsg(null), 3000);
    }
  };

  const handleRetry = async (taskId: string) => {
    try {
      await retryTask(taskId);
      setSelectedTask(null);
      refetch();
      queryClient.invalidateQueries({ queryKey: qk.tasks(conversationId) });
    } catch {
      // error handled elsewhere
    }
  };

  const handleDelete = async (taskId: string) => {
    try {
      await deleteTask(taskId);
      setSelectedTask(null);
      refetch();
    } catch {
      // error handled elsewhere
    }
  };

  if (!conversationId) {
    return (
      <div
        style={{
          padding: "32px 12px",
          textAlign: "center",
          fontSize: 11,
          color: C.text4,
        }}
      >
        Select a conversation to see its queue
      </div>
    );
  }

  const hasStaleRunning = tasks?.some((t) => t.state === "running") ?? false;

  if (!tasks || tasks.length === 0) {
    return (
      <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
        <div
          style={{
            flex: 1,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <span style={{ fontSize: 11, color: C.text4 }}>
            No tasks in this conversation
          </span>
        </div>
        <QueueFooter
          refreshing={refreshing}
          refreshMsg={refreshMsg}
          onRefresh={handleRefresh}
          onClear={handleClearQueue}
          showClear={false}
        />
      </div>
    );
  }

  const hasTerminal = tasks.some((t) =>
    ["failed", "completed", "cancelled"].includes(t.state),
  );

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div style={{ flex: 1, overflowY: "auto", padding: "10px 10px" }}>
        {hasStaleRunning && (
          <div
            style={{
              fontSize: 10,
              color: "#F59E0B",
              textAlign: "center",
              marginBottom: 8,
              padding: "4px 0",
            }}
          >
            Tasks may be stale — hit Refresh to reconcile
          </div>
        )}
        {refreshMsg && (
          <div
            style={{
              fontSize: 10,
              color: "#31B97B",
              textAlign: "center",
              marginBottom: 6,
              padding: "4px 0",
            }}
          >
            {refreshMsg}
          </div>
        )}
        {tasks.map((task, i) => {
          const sc = statusColor(task.state);
          const isQueued = task.state === "queued";
          const isRunning = task.state === "running";
          const isCancelling = cancellingId === task.id;
          const publishTag = task.publish_status
            ? ({
                published: { label: "Published", color: C.accent },
                failed: { label: "Publish Failed", color: C.danger },
                skipped_no_changes: { label: "No Changes", color: C.text3 },
                pending_retry: {
                  label: "Pending Publish",
                  color: C.warn,
                },
              }[task.publish_status] ?? {
                label: task.publish_status,
                color: C.text3,
              })
            : null;

          return (
            <div
              key={task.id}
              onClick={() => setSelectedTask(task)}
              style={{
                borderRadius: 6,
                background: "rgba(255,255,255,0.02)",
                padding: "10px 12px",
                marginBottom: 4,
                cursor: "pointer",
                transition: "background 0.12s",
              }}
              onMouseEnter={(e) =>
                (e.currentTarget.style.background = "rgba(255,255,255,0.05)")
              }
              onMouseLeave={(e) =>
                (e.currentTarget.style.background = "rgba(255,255,255,0.02)")
              }
            >
              <div style={{ display: "flex", gap: 8 }}>
                {/* Number badge */}
                <div
                  style={{
                    width: 20,
                    height: 20,
                    borderRadius: 4,
                    background: isRunning ? sc.bg : C.surfaceHover,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    fontSize: 10,
                    fontWeight: 700,
                    color: isRunning ? sc.text : C.text4,
                    flexShrink: 0,
                  }}
                >
                  {i + 1}
                </div>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <p
                    style={{
                      margin: "0 0 6px",
                      fontSize: 11,
                      color: "#DDE0E7",
                      lineHeight: 1.5,
                    }}
                  >
                    {truncate(task.objective, 80)}
                  </p>
                  {/* Tags row */}
                  <div
                    style={{
                      display: "flex",
                      gap: 6,
                      alignItems: "center",
                      flexWrap: "wrap",
                    }}
                  >
                    <span
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 4,
                      }}
                    >
                      <Dot status={task.state} size={6} />
                      <span
                        style={{
                          fontSize: 10,
                          color: sc.text,
                          fontWeight: 500,
                        }}
                      >
                        {task.state}
                      </span>
                    </span>
                    {task.priority > 0 && <Tag color="#F59E0B">P{task.priority}</Tag>}
                    {task.model && (
                      <Tag color={C.accent}>
                        {task.model
                          .replace("claude-", "")
                          .replace("-20251001", "")}
                      </Tag>
                    )}
                    {publishTag && (
                      <Tag color={publishTag.color}>{publishTag.label}</Tag>
                    )}
                    <span
                      style={{
                        fontSize: 10,
                        color: C.text4,
                        marginLeft: "auto",
                      }}
                    >
                      {relativeTime(task.queued_at)}
                    </span>
                  </div>
                  {/* Run ID if running/completed */}
                  {task.run_id && (
                    <div
                      style={{
                        marginTop: 4,
                        fontSize: 10,
                        color: C.text4,
                        fontFamily: C.mono,
                      }}
                    >
                      run: {task.run_id.slice(0, 12)}
                    </div>
                  )}
                  {task.publish_error && (
                    <div
                      style={{ marginTop: 4, fontSize: 10, color: C.danger }}
                    >
                      {task.publish_error}
                    </div>
                  )}
                </div>
                {/* Cancel button */}
                {isQueued && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleCancel(task.id);
                    }}
                    disabled={isCancelling}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      width: confirmId === task.id ? "auto" : 22,
                      height: 22,
                      padding: confirmId === task.id ? "0 8px" : 0,
                      borderRadius: 6,
                      flexShrink: 0,
                      alignSelf: "flex-start",
                      background:
                        confirmId === task.id
                          ? "#EF4444"
                          : "rgba(239,68,68,0.08)",
                      color:
                        confirmId === task.id ? "#fff" : "#EF444480",
                      fontSize: 9,
                      fontWeight: 600,
                      cursor: isCancelling ? "default" : "pointer",
                      transition: "all 0.15s",
                      opacity: isCancelling ? 0.5 : 1,
                      border: "none",
                    }}
                  >
                    {confirmId === task.id ? "Confirm" : <XIcon size={9} />}
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>

      <QueueFooter
        refreshing={refreshing}
        refreshMsg={refreshMsg}
        onRefresh={handleRefresh}
        onClear={handleClearQueue}
        showClear={hasTerminal}
      />

      {/* Task Detail Modal */}
      {selectedTask && (
        <TaskDetailModal
          task={selectedTask}
          onClose={() => setSelectedTask(null)}
          onRetry={handleRetry}
          onDelete={handleDelete}
        />
      )}
    </div>
  );
}

// ── Footer with Refresh + Clear buttons ───────────────────────────────────────

function QueueFooter({
  refreshing,
  refreshMsg,
  onRefresh,
  onClear,
  showClear,
}: {
  refreshing: boolean;
  refreshMsg: string | null;
  onRefresh: () => void;
  onClear: () => void;
  showClear: boolean;
}) {
  return (
    <div style={{ padding: "8px 10px", flexShrink: 0 }}>
      <div style={{ display: "flex", gap: 6 }}>
        <button
          onClick={onRefresh}
          disabled={refreshing}
          style={{
            flex: 1,
            padding: "8px 0",
            borderRadius: 6,
            background: "rgba(255,255,255,0.04)",
            border: "none",
            color: C.text3,
            fontSize: 11,
            fontWeight: 500,
            cursor: refreshing ? "default" : "pointer",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 5,
            opacity: refreshing ? 0.5 : 1,
            transition: "background 0.12s",
          }}
        >
          <Refresh size={11} />{" "}
          {refreshing ? "Refreshing..." : "Refresh"}
        </button>
        {showClear && (
          <button
            onClick={onClear}
            style={{
              padding: "8px 12px",
              borderRadius: 6,
              background: "rgba(239,68,68,0.06)",
              border: "none",
              color: "#EF4444",
              fontSize: 11,
              fontWeight: 500,
              cursor: "pointer",
              transition: "background 0.12s",
            }}
          >
            Clear
          </button>
        )}
      </div>
      {refreshMsg && (
        <div
          style={{
            fontSize: 10,
            color: "#31B97B",
            textAlign: "center",
            marginTop: 4,
          }}
        >
          {refreshMsg}
        </div>
      )}
    </div>
  );
}
