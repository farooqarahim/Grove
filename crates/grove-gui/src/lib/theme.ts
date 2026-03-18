/** Shared color constants — single source of truth for the Grove GUI palette.
 *  Blue-gray tinted dark theme inspired by the page-polish-pro reference. */
export const C = {
  /** Level 0 — deepest background */
  base: "#15171E",
  /** Level 1 — panels, cards, sidebars */
  surface: "#1C1F27",
  /** Level 2 — items on surface, hover, nested sections */
  surfaceHover: "#24272F",
  /** Level 2.5 — active/pressed state */
  surfaceActive: "#292D35",
  /** Level 3 — prominently raised elements */
  surfaceRaised: "#2F333B",
  /** Sidebar — darkest panel background */
  sidebar: "#111419",
  text1: "#FFFFFF",
  text2: "#FFFFFF",
  text3: "#FFFFFF",
  text4: "#FFFFFF",
  accent: "#31B97B",
  accentDim: "rgba(49,185,123,0.12)",
  accentMuted: "rgba(49,185,123,0.06)",
  blue: "#3B82F6",
  blueDim: "rgba(59,130,246,0.12)",
  blueMuted: "rgba(59,130,246,0.06)",
  danger: "#EF4444",
  dangerDim: "rgba(239,68,68,0.12)",
  warn: "#F59E0B",
  warnDim: "rgba(245,158,11,0.12)",
  purple: "#818CF8",
  purpleDim: "rgba(99,102,241,0.12)",
  border: "#262A31",
  borderSubtle: "#222329",
  borderHover: "#363A43",
  accentBorder: "rgba(49,185,123,0.2)",
  blueBorder: "rgba(59,130,246,0.2)",
  warnBorder: "rgba(245,158,11,0.25)",
  dangerBorder: "rgba(239,68,68,0.2)",
  purpleBorder: "rgba(129,140,248,0.2)",
  mono: "'JetBrains Mono', 'SF Mono', 'Menlo', 'Monaco', 'Fira Code', monospace",
} as const;

/** Pipeline-stage & graph-status colors used by Grove Graph components. */
export const graphColors = {
  open:       { bg: "rgba(113,118,127,0.10)", text: "#9CA3AF", border: "rgba(113,118,127,0.20)" },
  pending:    { bg: "rgba(113,118,127,0.10)", text: "#9CA3AF", border: "rgba(113,118,127,0.20)" },
  idle:       { bg: "rgba(113,118,127,0.10)", text: "#9CA3AF", border: "rgba(113,118,127,0.20)" },
  inprogress: { bg: "rgba(245,158,11,0.12)", text: "#FBBF24", border: "rgba(245,158,11,0.25)" },
  running:    { bg: "rgba(245,158,11,0.12)", text: "#FBBF24", border: "rgba(245,158,11,0.25)" },
  building:   { bg: "rgba(245,158,11,0.12)", text: "#FBBF24", border: "rgba(245,158,11,0.25)" },
  fixing:     { bg: "rgba(245,158,11,0.12)", text: "#F59E0B", border: "rgba(245,158,11,0.25)" },
  closed:     { bg: "rgba(49,185,123,0.10)", text: "#34D399", border: "rgba(49,185,123,0.20)" },
  passed:     { bg: "rgba(49,185,123,0.10)", text: "#34D399", border: "rgba(49,185,123,0.20)" },
  done:       { bg: "rgba(49,185,123,0.10)", text: "#34D399", border: "rgba(49,185,123,0.20)" },
  failed:     { bg: "rgba(239,68,68,0.10)",  text: "#F87171", border: "rgba(239,68,68,0.20)" },
  aborted:    { bg: "rgba(239,68,68,0.10)",  text: "#F87171", border: "rgba(239,68,68,0.20)" },
  validating: { bg: "rgba(59,130,246,0.12)", text: "#60A5FA", border: "rgba(59,130,246,0.25)" },
  judging:    { bg: "rgba(129,140,248,0.12)", text: "#A5B4FC", border: "rgba(129,140,248,0.25)" },
  verdict:    { bg: "rgba(251,146,60,0.12)", text: "#FB923C", border: "rgba(251,146,60,0.25)" },
  paused:     { bg: "rgba(234,179,8,0.10)",  text: "#FACC15", border: "rgba(234,179,8,0.20)" },
  queued:     { bg: "rgba(147,130,220,0.10)", text: "#A78BFA", border: "rgba(147,130,220,0.20)" },
  generating: { bg: "rgba(129,140,248,0.12)", text: "#A5B4FC", border: "rgba(129,140,248,0.25)" },
  draft_ready: { bg: "rgba(59,130,246,0.12)", text: "#60A5FA", border: "rgba(59,130,246,0.25)" },
} as const;

export type GraphColorKey = keyof typeof graphColors;

export function getGraphColor(status: string): { bg: string; text: string; border: string } {
  return (graphColors as Record<string, { bg: string; text: string; border: string }>)[status]
    ?? graphColors.pending;
}

export const lbl: React.CSSProperties = {
  fontSize: 10, fontWeight: 600, color: C.text4,
  textTransform: "uppercase", letterSpacing: "0.06em", marginBottom: 6,
};
