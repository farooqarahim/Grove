import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { qk } from "@/lib/queryKeys";
import { getAutomation, toggleAutomation, triggerAutomationManually } from "@/lib/api";
import { C } from "@/lib/theme";
import { ChevronR, Play, VDotsIcon } from "@/components/ui/icons";
import { StepsTab } from "./StepsTab";
import { RunsTab } from "./RunsTab";
import { ConfigTab } from "./ConfigTab";

// ── Types ────────────────────────────────────────────────────────────

interface Props {
  automationId: string;
  projectId: string | null;
  onBack: () => void;
}

type DetailTab = "steps" | "runs" | "config";

const TABS: { id: DetailTab; label: string }[] = [
  { id: "steps", label: "Steps" },
  { id: "runs",  label: "Runs" },
  { id: "config", label: "Config" },
];

// ── Trigger button state ─────────────────────────────────────────────

type TriggerState = "idle" | "triggering" | "success" | "error";

// ── Main component ───────────────────────────────────────────────────

export function AutomationDetail({ automationId, projectId: _projectId, onBack }: Props) {
  const queryClient = useQueryClient();
  const [tab, setTab] = useState<DetailTab>("steps");
  const [triggerState, setTriggerState] = useState<TriggerState>("idle");
  const [menuOpen, setMenuOpen] = useState(false);

  const { data: automation, refetch } = useQuery({
    queryKey: ["automation", automationId],
    queryFn: () => getAutomation(automationId),
    refetchInterval: 30000,
  });

  async function handleToggle() {
    if (!automation) return;
    await toggleAutomation(automationId, !automation.enabled);
    await refetch();
    queryClient.invalidateQueries({ queryKey: qk.automations(automation.project_id) });
  }

  async function handleRunNow() {
    setTriggerState("triggering");
    try {
      await triggerAutomationManually(automationId);
      setTriggerState("success");
      queryClient.invalidateQueries({ queryKey: qk.automationRuns(automationId) });
      setTimeout(() => setTriggerState("idle"), 2000);
    } catch {
      setTriggerState("error");
      setTimeout(() => setTriggerState("idle"), 2500);
    }
  }

  const runNowLabel =
    triggerState === "triggering" ? "Triggering..."
    : triggerState === "success" ? "Triggered!"
    : triggerState === "error" ? "Failed"
    : "Run Now";

  const runNowColor =
    triggerState === "success" ? C.accent
    : triggerState === "error" ? C.danger
    : C.blue;

  return (
    <div style={{ flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
      {/* ── Header ──────────────────────────────────────────────── */}
      <div style={{ padding: "20px 28px 0", flexShrink: 0 }}>
        {/* Back button */}
        <button
          onClick={onBack}
          style={{
            background: "none",
            border: "none",
            color: "#64748b",
            cursor: "pointer",
            display: "flex",
            alignItems: "center",
            gap: 4,
            marginBottom: 16,
            fontFamily: "inherit",
            fontSize: 13,
            fontWeight: 500,
            padding: 0,
            transition: "color .12s",
          }}
          onMouseEnter={(e) => { e.currentTarget.style.color = C.text1; }}
          onMouseLeave={(e) => { e.currentTarget.style.color = "#64748b"; }}
        >
          <span style={{ display: "inline-flex", transform: "rotate(180deg)" }}>
            <ChevronR size={12} />
          </span>
          Back to automations
        </button>

        {/* Title bar */}
        {automation ? (
          <div style={{ display: "flex", alignItems: "flex-start", gap: 16, marginBottom: 20 }}>
            {/* Name + description */}
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 18, fontWeight: 700, color: C.text1, marginBottom: 4 }}>
                {automation.name}
              </div>
              {automation.description && (
                <div style={{ fontSize: 13, color: "#64748b", lineHeight: 1.5 }}>
                  {automation.description}
                </div>
              )}
            </div>

            {/* Controls */}
            <div style={{ display: "flex", alignItems: "center", gap: 10, flexShrink: 0 }}>
              {/* Enable/disable toggle */}
              <button
                onClick={handleToggle}
                title={automation.enabled ? "Disable automation" : "Enable automation"}
                style={{
                  position: "relative",
                  width: 38,
                  height: 20,
                  borderRadius: 10,
                  border: "none",
                  cursor: "pointer",
                  background: automation.enabled ? C.accent : "#3F434B",
                  transition: "background .2s",
                  padding: 0,
                  flexShrink: 0,
                }}
              >
                <span
                  style={{
                    position: "absolute",
                    top: 2,
                    left: automation.enabled ? 20 : 2,
                    width: 16,
                    height: 16,
                    borderRadius: "50%",
                    background: "#fff",
                    transition: "left .2s",
                    boxShadow: "0 1px 3px rgba(0,0,0,0.3)",
                  }}
                />
              </button>

              {/* Run Now button */}
              <button
                onClick={handleRunNow}
                disabled={triggerState === "triggering"}
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 6,
                  padding: "6px 14px",
                  borderRadius: 6,
                  border: "none",
                  background: runNowColor,
                  color: "#fff",
                  fontSize: 12,
                  fontWeight: 700,
                  cursor: triggerState === "triggering" ? "default" : "pointer",
                  opacity: triggerState === "triggering" ? 0.7 : 1,
                  fontFamily: "inherit",
                  transition: "background .15s, opacity .15s",
                  whiteSpace: "nowrap",
                }}
                onMouseEnter={(e) => {
                  if (triggerState === "idle") e.currentTarget.style.opacity = "0.85";
                }}
                onMouseLeave={(e) => {
                  if (triggerState === "idle") e.currentTarget.style.opacity = "1";
                }}
              >
                <Play size={10} />
                {runNowLabel}
              </button>

              {/* Menu button */}
              <div style={{ position: "relative" }}>
                <button
                  onClick={() => setMenuOpen(!menuOpen)}
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    justifyContent: "center",
                    width: 30,
                    height: 30,
                    borderRadius: 6,
                    border: `1px solid ${C.border}`,
                    background: menuOpen ? C.surfaceHover : "transparent",
                    color: "#64748b",
                    cursor: "pointer",
                    transition: "background .12s",
                  }}
                  onMouseEnter={(e) => { e.currentTarget.style.background = C.surfaceHover; }}
                  onMouseLeave={(e) => {
                    if (!menuOpen) e.currentTarget.style.background = "transparent";
                  }}
                >
                  <VDotsIcon size={14} />
                </button>

                {menuOpen && (
                  <>
                    {/* Click-away overlay */}
                    <div
                      style={{ position: "fixed", inset: 0, zIndex: 99 }}
                      onClick={() => setMenuOpen(false)}
                    />
                    <div
                      style={{
                        position: "absolute",
                        top: "100%",
                        right: 0,
                        marginTop: 4,
                        width: 160,
                        background: C.surfaceRaised,
                        border: `1px solid ${C.border}`,
                        borderRadius: 8,
                        padding: 4,
                        zIndex: 100,
                        boxShadow: "0 8px 24px rgba(0,0,0,0.35)",
                      }}
                    >
                      <button
                        onClick={() => {
                          setMenuOpen(false);
                          setTab("config");
                        }}
                        style={{
                          width: "100%",
                          padding: "8px 12px",
                          borderRadius: 5,
                          border: "none",
                          background: "transparent",
                          color: C.danger,
                          fontSize: 12,
                          cursor: "pointer",
                          textAlign: "left",
                          fontFamily: "inherit",
                          transition: "background .1s",
                        }}
                        onMouseEnter={(e) => { e.currentTarget.style.background = C.surfaceHover; }}
                        onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; }}
                      >
                        Delete
                      </button>
                    </div>
                  </>
                )}
              </div>
            </div>
          </div>
        ) : (
          /* Loading skeleton */
          <div style={{ marginBottom: 20 }}>
            <div style={{
              width: 200, height: 20, borderRadius: 4,
              background: C.surfaceHover, marginBottom: 8,
            }} />
            <div style={{
              width: 320, height: 14, borderRadius: 4,
              background: C.surfaceHover,
            }} />
          </div>
        )}

        {/* ── Tab bar ────────────────────────────────────────────── */}
        <div
          style={{
            display: "flex",
            gap: 0,
            borderBottom: `1px solid ${C.border}`,
          }}
        >
          {TABS.map((t) => {
            const active = tab === t.id;
            return (
              <button
                key={t.id}
                onClick={() => setTab(t.id)}
                style={{
                  padding: "10px 20px",
                  background: "none",
                  border: "none",
                  borderBottom: `2px solid ${active ? C.accent : "transparent"}`,
                  color: active ? C.accent : "#64748b",
                  fontSize: 13,
                  fontWeight: active ? 700 : 500,
                  cursor: "pointer",
                  fontFamily: "inherit",
                  transition: "color .12s, border-color .12s",
                  marginBottom: -1,
                }}
                onMouseEnter={(e) => {
                  if (!active) e.currentTarget.style.color = C.text2;
                }}
                onMouseLeave={(e) => {
                  if (!active) e.currentTarget.style.color = "#64748b";
                }}
              >
                {t.label}
              </button>
            );
          })}
        </div>
      </div>

      {/* ── Tab content ─────────────────────────────────────────── */}
      <div style={{ flex: 1, overflowY: "auto", padding: "20px 28px 32px" }}>
        {tab === "steps" && (
          <StepsTab automationId={automationId} />
        )}
        {tab === "runs" && (
          <RunsTab automationId={automationId} />
        )}
        {tab === "config" && automation && (
          <ConfigTab
            automation={automation}
            onUpdate={() => refetch()}
            onBack={onBack}
          />
        )}
        {tab === "config" && !automation && (
          <div style={{ padding: "48px 0", textAlign: "center", fontSize: 13, color: "#64748b" }}>
            Loading configuration...
          </div>
        )}
      </div>
    </div>
  );
}
