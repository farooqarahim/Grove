import { useState } from "react";
import { deleteAutomation } from "@/lib/api";
import { C, lbl } from "@/lib/theme";
import { Trash } from "@/components/ui/icons";
import type { AutomationDef, NotificationTarget, TriggerConfig } from "@/types";

interface Props {
  automation: AutomationDef;
  onUpdate: () => void;
  onBack: () => void;
}

// ── Helpers ──────────────────────────────────────────────────────────

const TRIGGER_BADGE: Record<string, { label: string; color: string; bg: string }> = {
  cron:    { label: "Cron",    color: C.blue,   bg: C.blueDim },
  webhook: { label: "Webhook", color: C.purple, bg: C.purpleDim },
  manual:  { label: "Manual",  color: C.accent, bg: C.accentDim },
  event:   { label: "Event",   color: C.warn,   bg: C.warnDim },
  issue:   { label: "Issue",   color: "#f97316", bg: "rgba(249,115,22,0.1)" },
};

function humanCron(expr: string): string {
  // Basic human-readable cron descriptions
  const parts = expr.trim().split(/\s+/);
  if (parts.length < 5) return expr;
  const [min, hour, dom, mon, dow] = parts;

  if (min === "0" && hour === "*" && dom === "*" && mon === "*" && dow === "*") return "Every hour";
  if (dom === "*" && mon === "*" && dow === "*") {
    if (hour === "*") return `Every hour at :${min.padStart(2, "0")}`;
    return `Daily at ${hour}:${min.padStart(2, "0")}`;
  }
  if (dow !== "*" && dom === "*" && mon === "*") return `Weekly (${dow}) at ${hour}:${min.padStart(2, "0")}`;
  return expr;
}

function triggerDescription(trigger: TriggerConfig): string {
  switch (trigger.type) {
    case "cron":    return humanCron(trigger.schedule);
    case "event":   return `On event: ${trigger.event_type}`;
    case "webhook": return "On incoming webhook";
    case "manual":  return "Manual trigger only";
    case "issue":   return `When issues match: ${trigger.statuses.join(", ")}`;
    default:        return "Unknown trigger";
  }
}

function notificationTargetLabel(target: NotificationTarget): string {
  switch (target.type) {
    case "slack":   return `Slack${target.channel ? ` #${target.channel}` : ""}`;
    case "webhook": return `Webhook: ${target.url}`;
    case "system":  return "System notification";
    default:        return "Unknown";
  }
}

function notificationTargetBadge(target: NotificationTarget): { label: string; color: string; bg: string } {
  switch (target.type) {
    case "slack":   return { label: "Slack",   color: C.blue,   bg: C.blueDim };
    case "webhook": return { label: "Webhook", color: C.purple, bg: C.purpleDim };
    case "system":  return { label: "System",  color: C.accent, bg: C.accentDim };
    default:        return { label: "Other", color: "#64748b", bg: "rgba(100,116,139,0.1)" };
  }
}

// ── Section card ─────────────────────────────────────────────────────

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div
      style={{
        background: C.surface,
        border: `1px solid ${C.border}`,
        borderRadius: 8,
        padding: "18px 20px",
        marginBottom: 16,
      }}
    >
      <div style={{ ...lbl, marginBottom: 12 }}>{title}</div>
      {children}
    </div>
  );
}

function ConfigRow({ label, value, mono }: { label: string; value: string | null; mono?: boolean }) {
  return (
    <div
      style={{
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        padding: "6px 0",
        borderBottom: `1px solid ${C.border}`,
      }}
    >
      <span style={{ fontSize: 12, color: "#64748b" }}>{label}</span>
      <span
        style={{
          color: value ? C.text2 : "#52575F",
          fontFamily: mono ? C.mono : "inherit",
          fontSize: mono ? 11 : 12,
        }}
      >
        {value ?? "Default"}
      </span>
    </div>
  );
}

// ── Main component ───────────────────────────────────────────────────

export function ConfigTab({ automation, onUpdate: _onUpdate, onBack }: Props) {
  const [deleting, setDeleting] = useState(false);

  const trigger = automation.trigger;
  const badge = TRIGGER_BADGE[trigger.type] ?? TRIGGER_BADGE.manual;
  const defaults = automation.defaults;
  const notifications = automation.notifications;

  async function handleDelete() {
    const confirmed = window.confirm(
      `Delete automation "${automation.name}"? This cannot be undone.`,
    );
    if (!confirmed) return;
    setDeleting(true);
    try {
      await deleteAutomation(automation.id);
      onBack();
    } catch {
      setDeleting(false);
    }
  }

  return (
    <div style={{ padding: "4px 0", maxWidth: 640 }}>
      {/* Trigger section */}
      <Section title="Trigger">
        <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 10 }}>
          <span
            style={{
              fontSize: 10,
              fontWeight: 700,
              color: badge.color,
              background: badge.bg,
              padding: "3px 8px",
              borderRadius: 4,
              letterSpacing: "0.03em",
            }}
          >
            {badge.label}
          </span>
          <span style={{ fontSize: 12, color: C.text2 }}>
            {triggerDescription(trigger)}
          </span>
        </div>
        {trigger.type === "cron" && (
          <div style={{ fontSize: 11, fontFamily: C.mono, color: "#64748b", padding: "4px 0" }}>
            {trigger.schedule}
          </div>
        )}
        {trigger.type === "event" && trigger.filter != null && (
          <div style={{ fontSize: 11, fontFamily: C.mono, color: "#64748b", padding: "4px 0" }}>
            Filter: {String(JSON.stringify(trigger.filter))}
          </div>
        )}
        {trigger.type === "webhook" && trigger.filter != null && (
          <div style={{ fontSize: 11, fontFamily: C.mono, color: "#64748b", padding: "4px 0" }}>
            Filter: {String(JSON.stringify(trigger.filter))}
          </div>
        )}
        {trigger.type === "issue" && (
          <div style={{ display: "flex", flexDirection: "column", gap: 4, marginTop: 4 }}>
            <div style={{ fontSize: 11, fontFamily: C.mono, color: "#64748b", padding: "4px 0" }}>
              {trigger.schedule}
            </div>
            <ConfigRow label="Schedule" value={humanCron(trigger.schedule)} />
            <ConfigRow label="Statuses" value={trigger.statuses.join(", ")} />
            {trigger.labels.length > 0 && (
              <ConfigRow label="Labels" value={trigger.labels.join(", ")} />
            )}
          </div>
        )}
      </Section>

      {/* Defaults section */}
      <Section title="Defaults">
        <ConfigRow label="Provider" value={defaults.provider} mono />
        <ConfigRow label="Model" value={defaults.model} mono />
        <ConfigRow label="Pipeline" value={defaults.pipeline} mono />
        <ConfigRow label="Permission Mode" value={defaults.permission_mode} />
      </Section>

      {/* Session mode */}
      <Section title="Session">
        <ConfigRow
          label="Mode"
          value={automation.session_mode === "dedicated" ? "Dedicated conversation" : "New conversation per run"}
        />
        {automation.dedicated_conversation_id && (
          <ConfigRow label="Conversation ID" value={automation.dedicated_conversation_id} mono />
        )}
        {automation.source_path && (
          <ConfigRow label="Source File" value={automation.source_path} mono />
        )}
      </Section>

      {/* Notifications section */}
      <Section title="Notifications">
        {(!notifications || (notifications.on_success.length === 0 && notifications.on_failure.length === 0)) ? (
          <div style={{ fontSize: 12, color: "#64748b", padding: "4px 0" }}>
            No notifications configured.
          </div>
        ) : (
          <>
            {notifications.on_success.length > 0 && (
              <div style={{ marginBottom: 12 }}>
                <div style={{ fontSize: 11, color: C.accent, fontWeight: 600, marginBottom: 6 }}>
                  On Success
                </div>
                {notifications.on_success.map((target, i) => {
                  const tb = notificationTargetBadge(target);
                  return (
                    <div
                      key={i}
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 8,
                        padding: "5px 0",
                        borderBottom: `1px solid ${C.border}`,
                      }}
                    >
                      <span
                        style={{
                          fontSize: 9,
                          fontWeight: 700,
                          color: tb.color,
                          background: tb.bg,
                          padding: "2px 6px",
                          borderRadius: 3,
                        }}
                      >
                        {tb.label}
                      </span>
                      <span style={{ fontSize: 11, color: C.text2 }}>
                        {notificationTargetLabel(target)}
                      </span>
                    </div>
                  );
                })}
              </div>
            )}
            {notifications.on_failure.length > 0 && (
              <div>
                <div style={{ fontSize: 11, color: C.danger, fontWeight: 600, marginBottom: 6 }}>
                  On Failure
                </div>
                {notifications.on_failure.map((target, i) => {
                  const tb = notificationTargetBadge(target);
                  return (
                    <div
                      key={i}
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 8,
                        padding: "5px 0",
                        borderBottom: `1px solid ${C.border}`,
                      }}
                    >
                      <span
                        style={{
                          fontSize: 9,
                          fontWeight: 700,
                          color: tb.color,
                          background: tb.bg,
                          padding: "2px 6px",
                          borderRadius: 3,
                        }}
                      >
                        {tb.label}
                      </span>
                      <span style={{ fontSize: 11, color: C.text2 }}>
                        {notificationTargetLabel(target)}
                      </span>
                    </div>
                  );
                })}
              </div>
            )}
          </>
        )}
      </Section>

      {/* Actions (danger zone) */}
      <Section title="Actions">
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <div>
            <button
              onClick={handleDelete}
              disabled={deleting}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
                padding: "7px 14px",
                borderRadius: 6,
                border: `1px solid rgba(239,68,68,0.3)`,
                background: C.dangerDim,
                color: C.danger,
                fontSize: 12,
                fontWeight: 600,
                cursor: deleting ? "default" : "pointer",
                opacity: deleting ? 0.6 : 1,
                fontFamily: "inherit",
                transition: "background .12s",
              }}
              onMouseEnter={(e) => {
                if (!deleting) e.currentTarget.style.background = "rgba(239,68,68,0.18)";
              }}
              onMouseLeave={(e) => {
                if (!deleting) e.currentTarget.style.background = C.dangerDim;
              }}
            >
              <Trash size={12} />
              {deleting ? "Deleting..." : "Delete Automation"}
            </button>
          </div>
        </div>
      </Section>
    </div>
  );
}
