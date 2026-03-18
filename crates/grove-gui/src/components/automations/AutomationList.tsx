import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { listAutomations } from "@/lib/api";
import { qk } from "@/lib/queryKeys";
import { C } from "@/lib/theme";
import { Plus, Search, Zap } from "@/components/ui/icons";
import { AutomationCard } from "@/components/automations/AutomationCard";
import { CreateAutomationModal } from "@/components/automations/CreateAutomationModal";
import type { AutomationDef, ProjectRow } from "@/types";

interface Props {
  projectId: string | null;
  projects: ProjectRow[];
  onSelect: (automationId: string) => void;
}

type TriggerTab = "all" | "cron" | "webhook" | "manual" | "event" | "issue";
type EnabledFilter = "all" | "enabled" | "disabled";

const TRIGGER_TABS: { value: TriggerTab; label: string }[] = [
  { value: "all", label: "All" },
  { value: "cron", label: "Cron" },
  { value: "issue", label: "Issue" },
  { value: "webhook", label: "Webhook" },
  { value: "manual", label: "Manual" },
  { value: "event", label: "Event" },
];

const ENABLED_OPTIONS: { value: EnabledFilter; label: string }[] = [
  { value: "all", label: "All states" },
  { value: "enabled", label: "Enabled" },
  { value: "disabled", label: "Disabled" },
];

export function AutomationList({ projectId, projects: _projects, onSelect }: Props) {
  const queryClient = useQueryClient();
  const [triggerTab, setTriggerTab] = useState<TriggerTab>("all");
  const [enabledFilter, setEnabledFilter] = useState<EnabledFilter>("all");
  const [search, setSearch] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [enabledDropdown, setEnabledDropdown] = useState(false);

  const { data: automations = [] } = useQuery({
    queryKey: qk.automations(projectId),
    queryFn: () => (projectId ? listAutomations(projectId) : Promise.resolve([])),
    refetchInterval: 30000,
    staleTime: 10000,
  });

  // Listen for backend changes
  useEffect(() => {
    const unlisten = listen("grove://automations-changed", () => {
      queryClient.invalidateQueries({ queryKey: qk.automations(projectId) });
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [projectId, queryClient]);

  const filtered = useMemo(() => {
    const q = search.toLowerCase();
    return automations.filter((a: AutomationDef) => {
      if (triggerTab !== "all" && a.trigger.type !== triggerTab) return false;
      if (enabledFilter === "enabled" && !a.enabled) return false;
      if (enabledFilter === "disabled" && a.enabled) return false;
      if (q && !a.name.toLowerCase().includes(q) && !(a.description ?? "").toLowerCase().includes(q)) return false;
      return true;
    });
  }, [automations, triggerTab, enabledFilter, search]);

  if (!projectId) {
    return (
      <div
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          background: C.base,
          padding: 40,
        }}
      >
        <div style={{ textAlign: "center", maxWidth: 420 }}>
          <div
            style={{
              width: 48,
              height: 48,
              borderRadius: 12,
              margin: "0 auto 20px",
              background: C.accentDim,
              border: `1px solid rgba(49,185,123,0.15)`,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              color: C.accent,
            }}
          >
            <Zap size={20} />
          </div>
          <h2
            style={{
              fontSize: 18,
              fontWeight: 700,
              color: C.text1,
              letterSpacing: "-0.02em",
              margin: "0 0 8px",
            }}
          >
            Select a project
          </h2>
          <p style={{ fontSize: 13, color: "#475569", margin: 0, lineHeight: 1.5 }}>
            Choose a project to view and manage its automations.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        background: C.base,
        color: C.text2,
        fontFamily: "'Geist', 'DM Sans', -apple-system, sans-serif",
      }}
    >
      {/* ── Top bar ── */}
      <div style={{ padding: "20px 28px 0", flexShrink: 0 }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            marginBottom: 18,
          }}
        >
          <div style={{ display: "flex", alignItems: "baseline", gap: 10 }}>
            <h1
              style={{
                fontSize: 20,
                fontWeight: 700,
                color: C.text1,
                letterSpacing: "-0.03em",
                margin: 0,
              }}
            >
              Automations
            </h1>
            <span style={{ fontSize: 13, color: "#334155", fontWeight: 500 }}>
              {filtered.length} of {automations.length}
            </span>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            {/* Import button (disabled for now) */}
            <button
              disabled
              title="Coming soon"
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                padding: "9px 14px",
                borderRadius: 10,
                fontSize: 12,
                fontWeight: 600,
                cursor: "not-allowed",
                fontFamily: "inherit",
                background: "rgba(51,65,85,0.12)",
                border: "1px solid rgba(51,65,85,0.2)",
                color: "#475569",
                opacity: 0.6,
              }}
            >
              Import .md
            </button>
            {/* New automation button */}
            <button
              onClick={() => setShowCreate(true)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                padding: "9px 18px",
                borderRadius: 10,
                fontSize: 13,
                fontWeight: 700,
                cursor: "pointer",
                fontFamily: "inherit",
                background: "linear-gradient(135deg, #31B97B, #269962)",
                color: "#fff",
                border: "1px solid rgba(49,185,123,0.3)",
                boxShadow: "0 0 24px rgba(49,185,123,0.12)",
                transition: "all .2s",
              }}
            >
              <Plus size={12} /> New
            </button>
          </div>
        </div>

        {/* ── Filter bar ── */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            flexWrap: "wrap",
            paddingBottom: 16,
            borderBottom: "1px solid rgba(51,65,85,0.15)",
          }}
        >
          {/* Trigger tabs */}
          <div
            style={{
              display: "flex",
              background: "rgba(15,23,42,0.5)",
              borderRadius: 9,
              border: "1px solid rgba(51,65,85,0.2)",
              padding: 3,
            }}
          >
            {TRIGGER_TABS.map((t) => (
              <button
                key={t.value}
                onClick={() => setTriggerTab(t.value)}
                style={{
                  padding: "5px 13px",
                  borderRadius: 7,
                  fontSize: 12,
                  fontWeight: 600,
                  border: "none",
                  cursor: "pointer",
                  fontFamily: "inherit",
                  transition: "all .15s",
                  background: triggerTab === t.value ? C.accentDim : "transparent",
                  color: triggerTab === t.value ? "#4ade80" : "#64748b",
                }}
              >
                {t.label}
              </button>
            ))}
          </div>

          {/* Enabled/disabled dropdown */}
          <div style={{ position: "relative" }}>
            <button
              onClick={() => setEnabledDropdown(!enabledDropdown)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                padding: "6px 12px",
                borderRadius: 8,
                fontSize: 12,
                fontWeight: 600,
                fontFamily: "inherit",
                background: "rgba(15,23,42,0.5)",
                border: "1px solid rgba(51,65,85,0.25)",
                color: "#64748b",
                cursor: "pointer",
              }}
            >
              {ENABLED_OPTIONS.find((o) => o.value === enabledFilter)?.label}
              <svg
                width="10"
                height="10"
                viewBox="0 0 16 16"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <path d="M4 6l4 4 4-4" />
              </svg>
            </button>
            {enabledDropdown && (
              <>
                <div
                  onClick={() => setEnabledDropdown(false)}
                  style={{ position: "fixed", inset: 0, zIndex: 40 }}
                />
                <div
                  style={{
                    position: "absolute",
                    top: "100%",
                    left: 0,
                    marginTop: 4,
                    zIndex: 50,
                    background: "#0f172a",
                    border: "1px solid rgba(51,65,85,0.35)",
                    borderRadius: 10,
                    boxShadow: "0 12px 40px rgba(0,0,0,0.4)",
                    overflow: "hidden",
                    minWidth: 140,
                  }}
                >
                  {ENABLED_OPTIONS.map((opt) => (
                    <button
                      key={opt.value}
                      onClick={() => {
                        setEnabledFilter(opt.value);
                        setEnabledDropdown(false);
                      }}
                      style={{
                        display: "block",
                        width: "100%",
                        padding: "8px 14px",
                        background:
                          enabledFilter === opt.value ? "rgba(51,65,85,0.2)" : "transparent",
                        border: "none",
                        color: "#cbd5e1",
                        fontSize: 12,
                        cursor: "pointer",
                        fontFamily: "inherit",
                        textAlign: "left",
                        transition: "background .1s",
                      }}
                      onMouseEnter={(e) => {
                        e.currentTarget.style.background = "rgba(51,65,85,0.25)";
                      }}
                      onMouseLeave={(e) => {
                        e.currentTarget.style.background =
                          enabledFilter === opt.value ? "rgba(51,65,85,0.2)" : "transparent";
                      }}
                    >
                      {opt.label}
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>

          {/* Search */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              padding: "6px 12px",
              borderRadius: 8,
              background: "rgba(15,23,42,0.5)",
              border: "1px solid rgba(51,65,85,0.25)",
              marginLeft: "auto",
              minWidth: 200,
              maxWidth: 280,
              flex: "0 1 auto",
            }}
          >
            <span style={{ color: "#334155", flexShrink: 0 }}>
              <Search size={12} />
            </span>
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search automations..."
              style={{
                flex: 1,
                background: "none",
                border: "none",
                outline: "none",
                color: C.text2,
                fontSize: 12,
                fontFamily: "inherit",
              }}
            />
          </div>
        </div>
      </div>

      {/* ── Card list ── */}
      <div
        style={{
          flex: 1,
          overflowY: "auto",
          padding: "16px 28px 28px",
          display: "flex",
          flexDirection: "column",
          gap: 8,
        }}
      >
        {filtered.length === 0 ? (
          <div
            style={{
              flex: 1,
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              justifyContent: "center",
              gap: 12,
              padding: 40,
            }}
          >
            <div
              style={{
                width: 44,
                height: 44,
                borderRadius: 12,
                background: "rgba(51,65,85,0.12)",
                border: "1px solid rgba(51,65,85,0.2)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                color: "#475569",
              }}
            >
              <Zap size={18} />
            </div>
            <div style={{ fontSize: 14, fontWeight: 600, color: "#64748b" }}>
              {automations.length === 0 ? "No automations yet" : "No matching automations"}
            </div>
            <div style={{ fontSize: 12, color: "#475569", maxWidth: 300, textAlign: "center", lineHeight: 1.5 }}>
              {automations.length === 0
                ? "Create your first automation to run tasks on a schedule, in response to events, or manually."
                : "Try adjusting your filters or search query."}
            </div>
          </div>
        ) : (
          filtered.map((a) => (
            <AutomationCard key={a.id} automation={a} onClick={() => onSelect(a.id)} />
          ))
        )}
      </div>

      <CreateAutomationModal
        open={showCreate}
        projectId={projectId}
        onClose={() => setShowCreate(false)}
        onCreated={() => {
          queryClient.invalidateQueries({ queryKey: qk.automations(projectId) });
        }}
      />
    </div>
  );
}
