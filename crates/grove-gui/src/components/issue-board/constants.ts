import type { CanonicalStatus } from "@/types";

// ── Column config ─────────────────────────────────────────────────────────────

export const COLUMN_CONFIGS: Record<string, { label: string; dot: string }> = {
  open: { label: "Open", dot: "#3b82f6" },
  in_progress: { label: "In Progress", dot: "#31B97B" },
  in_review: { label: "In Review", dot: "#8b5cf6" },
  blocked: { label: "Blocked", dot: "#ef4444" },
  done: { label: "Done", dot: "#6b7280" },
  cancelled: { label: "Cancelled", dot: "#475569" },
};

// ── Priority config ───────────────────────────────────────────────────────────

export const PRIORITY_CONFIG: Record<string, { color: string; bg: string; border: string; icon: string }> = {
  Critical: { color: "#ef4444", bg: "rgba(239,68,68,0.1)", border: "rgba(239,68,68,0.2)", icon: "!!!" },
  High: { color: "#f97316", bg: "rgba(249,115,22,0.1)", border: "rgba(249,115,22,0.2)", icon: "!!" },
  Medium: { color: "#eab308", bg: "rgba(234,179,8,0.08)", border: "rgba(234,179,8,0.15)", icon: "!" },
  Low: { color: "#6b7280", bg: "rgba(107,114,128,0.08)", border: "rgba(107,114,128,0.15)", icon: "—" },
  None: { color: "#475569", bg: "rgba(71,85,105,0.08)", border: "rgba(71,85,105,0.15)", icon: "·" },
};

// ── Label colors ──────────────────────────────────────────────────────────────

export const LABEL_COLORS: Record<string, { color: string; bg: string }> = {
  bug: { color: "#fca5a5", bg: "rgba(252,165,165,0.1)" },
  auth: { color: "#f59e0b", bg: "rgba(245,158,11,0.08)" },
  feature: { color: "#31b97b", bg: "rgba(49,185,123,0.1)" },
  migration: { color: "#a78bfa", bg: "rgba(167,139,250,0.1)" },
  infra: { color: "#3b82f6", bg: "rgba(59,130,246,0.1)" },
  ux: { color: "#f472b6", bg: "rgba(244,114,182,0.1)" },
  lint: { color: "#94a3b8", bg: "rgba(148,163,184,0.08)" },
  perf: { color: "#fb923c", bg: "rgba(251,146,60,0.1)" },
  enterprise: { color: "#c084fc", bg: "rgba(192,132,252,0.1)" },
  cleanup: { color: "#6b7280", bg: "rgba(107,114,128,0.08)" },
};

// ── Canonical status sequence ─────────────────────────────────────────────────

export const CANONICAL_SEQUENCE: CanonicalStatus[] = [
  "open",
  "in_progress",
  "in_review",
  "blocked",
  "done",
  "cancelled",
];

// ── Provider status ordering ──────────────────────────────────────────────────

export const PROVIDER_STATUS_ORDER: Record<string, string[]> = {
  github: ["open", "closed"],
  jira: [
    "backlog", "selected for development", "to do", "open",
    "in progress", "in development", "in review", "code review",
    "qa", "in testing", "blocked", "done", "resolved", "closed",
    "cancelled", "won't do",
  ],
  linear: [
    "backlog", "todo", "triage", "in progress", "in review",
    "blocked", "done", "cancelled",
  ],
  grove: ["open", "in_progress", "in_review", "blocked", "done", "cancelled"],
  linter: ["open", "in_review", "done"],
};

// ── Filter constants ──────────────────────────────────────────────────────────

export const SOURCES = ["All", "GitHub", "Jira", "Linear", "Grove", "Linter"];
export const FILTER_PRIORITIES = ["Any priority", "Critical", "High", "Medium", "Low"];
export const BOARD_EDIT_PROVIDERS = ["github", "jira", "linear", "grove"];

// ── Types ─────────────────────────────────────────────────────────────────────

export type LayoutMode = "project" | "provider";

export type DisplayColumn = {
  id: string;
  title: string;
  subtitle?: string;
  accent: string;
  issues: import("@/types").Issue[];
  count: number;
  canonicalStatus: CanonicalStatus;
  provider?: string;
};

// ── CSS ───────────────────────────────────────────────────────────────────────

export const BOARD_CSS = `
  @keyframes fadeIn { from { opacity: 0 } to { opacity: 1 } }
  @keyframes slideUp { from { opacity: 0; transform: translateY(16px) scale(0.97) } to { opacity: 1; transform: translateY(0) scale(1) } }
  @keyframes slideIn { from { transform: translateX(100%) } to { transform: translateX(0) } }
  .ib-card:hover { background: rgba(15,23,42,0.9) !important; border-color: rgba(71,85,105,0.4) !important; transform: translateY(-1px) !important; }
  .ib-add-btn:hover { color: #94a3b8 !important; }
  .ib-close-btn:hover { background: rgba(239,68,68,0.1) !important; color: #ef4444 !important; }
  .ib-source-tab-active { background: rgba(49,185,123,0.12) !important; color: #4ade80 !important; }
  .ib-source-tab-inactive { background: transparent !important; color: #64748b !important; }
  .ib-sync-btn:hover { background: rgba(51,65,85,0.35) !important; }
  .ib-layout-active { background: rgba(59,130,246,0.12) !important; color: #93c5fd !important; border-color: rgba(59,130,246,0.2) !important; }
  input::placeholder, textarea::placeholder { color: #334155; }
`;
