import { C } from "@/lib/theme";
import type { AutomationDef } from "@/types";

interface Props {
  automation: AutomationDef;
  onClick: () => void;
}

const TRIGGER_BADGE: Record<string, { label: string; color: string; bg: string }> = {
  cron:    { label: "Cron",    color: C.blue,   bg: C.blueDim },
  webhook: { label: "Webhook", color: C.purple, bg: C.purpleDim },
  manual:  { label: "Manual",  color: C.accent, bg: C.accentDim },
  event:   { label: "Event",   color: C.warn,   bg: C.warnDim },
  issue:   { label: "Issue",   color: "#f97316", bg: "rgba(249,115,22,0.1)" },
};

function formatRelative(iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  const diff = Date.now() - d.getTime();
  const m = Math.floor(diff / 60000);
  if (m < 1) return "just now";
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const days = Math.floor(h / 24);
  if (days < 7) return `${days}d ago`;
  if (days < 30) return `${Math.floor(days / 7)}w ago`;
  return d.toLocaleDateString();
}

export function AutomationCard({ automation, onClick }: Props) {
  const badge = TRIGGER_BADGE[automation.trigger.type] ?? TRIGGER_BADGE.manual;

  return (
    <button
      onClick={onClick}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 14,
        width: "100%",
        padding: "14px 18px",
        borderRadius: 10,
        background: C.surface,
        border: `1px solid ${C.border}`,
        cursor: "pointer",
        fontFamily: "inherit",
        textAlign: "left",
        transition: "background .12s, border-color .12s",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.background = C.surfaceHover;
        e.currentTarget.style.borderColor = C.borderHover;
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = C.surface;
        e.currentTarget.style.borderColor = C.border;
      }}
    >
      {/* Enabled dot */}
      <span
        style={{
          width: 8,
          height: 8,
          borderRadius: "50%",
          flexShrink: 0,
          background: automation.enabled ? C.accent : "#52575F",
          boxShadow: automation.enabled ? `0 0 6px ${C.accentDim}` : undefined,
        }}
      />

      {/* Middle: name + description */}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 3 }}>
          <span
            style={{
              fontSize: 13,
              fontWeight: 700,
              color: C.text1,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {automation.name}
          </span>
          <span
            style={{
              fontSize: 10,
              fontWeight: 700,
              color: badge.color,
              background: badge.bg,
              padding: "2px 7px",
              borderRadius: 4,
              letterSpacing: "0.03em",
              flexShrink: 0,
            }}
          >
            {badge.label}
          </span>
        </div>
        {automation.description && (
          <div
            style={{
              fontSize: 12,
              color: "#64748b",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              maxWidth: 480,
            }}
          >
            {automation.description}
          </div>
        )}
      </div>

      {/* Right: last triggered */}
      <div
        style={{
          flexShrink: 0,
          textAlign: "right",
          fontSize: 11,
          color: "#475569",
          whiteSpace: "nowrap",
        }}
      >
        {automation.last_triggered_at
          ? formatRelative(automation.last_triggered_at)
          : "Never run"}
      </div>
    </button>
  );
}
