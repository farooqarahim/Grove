interface IcoProps {
  size?: number;
  stroke?: string;
  fill?: string;
  sw?: number;
  children: React.ReactNode;
}

function Ico({ size = 14, stroke = "currentColor", fill = "none", sw = 1.5, children }: IcoProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill={fill} stroke={stroke} strokeWidth={sw} strokeLinecap="round" strokeLinejoin="round">
      {children}
    </svg>
  );
}

export function ChevronR({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M9 18l6-6-6-6" /></Ico>;
}

export function Check({ size = 10 }: { size?: number }) {
  return <Ico size={size} sw={2}><path d="M5 12l5 5L20 7" /></Ico>;
}

export function XIcon({ size = 10 }: { size?: number }) {
  return <Ico size={size} sw={2}><path d="M18 6L6 18" /><path d="M6 6l12 12" /></Ico>;
}

export function Play({ size = 10 }: { size?: number }) {
  return <Ico size={size} fill="currentColor" stroke="none"><path d="M6 4l14 8-14 8V4z" /></Ico>;
}

export function Clock({ size = 10 }: { size?: number }) {
  return <Ico size={size}><circle cx="12" cy="12" r="9" /><path d="M12 6v6l4 2" /></Ico>;
}

export function Plus({ size = 12 }: { size?: number }) {
  return <Ico size={size} sw={2}><path d="M12 5v14m-7-7h14" /></Ico>;
}

export function Minus({ size = 12 }: { size?: number }) {
  return <Ico size={size} sw={2}><path d="M5 12h14" /></Ico>;
}

export function Terminal({ size = 13 }: { size?: number }) {
  return <Ico size={size}><rect x="2" y="3" width="20" height="18" rx="3" /><path d="M7 9l3 3-3 3" /><path d="M13 15h4" /></Ico>;
}

export function GitBranch({ size = 12 }: { size?: number }) {
  return <Ico size={size}><circle cx="12" cy="5" r="2.5" /><circle cx="12" cy="19" r="2.5" /><path d="M12 7.5v9" /></Ico>;
}

export function Eye({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" /><circle cx="12" cy="12" r="3" /></Ico>;
}

export function Merge({ size = 12 }: { size?: number }) {
  return <Ico size={size}><circle cx="18" cy="18" r="3" /><circle cx="6" cy="6" r="3" /><path d="M6 9v2a4 4 0 004 4h4" /></Ico>;
}

export function Undo({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M9 14L4 9m0 0l5-5M4 9h12a5 5 0 010 10h-2" /></Ico>;
}

export function Bolt({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" /></Ico>;
}

export function Arrow({ size = 10 }: { size?: number }) {
  return <Ico size={size}><path d="M14 5l7 7m0 0l-7 7m7-7H3" /></Ico>;
}

export function Search({ size = 12 }: { size?: number }) {
  return <Ico size={size}><circle cx="11" cy="11" r="7" /><path d="M21 21l-4.35-4.35" /></Ico>;
}

export function Home({ size = 16 }: { size?: number }) {
  return <Ico size={size}><path d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-4 0a1 1 0 01-1-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 01-1 1h-2" /></Ico>;
}

export function Gear({ size = 16 }: { size?: number }) {
  return <Ico size={size}><path d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.573-1.066z" /><circle cx="12" cy="12" r="3" /></Ico>;
}

export function Dollar({ size = 16 }: { size?: number }) {
  return <Ico size={size}><path d="M12 1v22m5-18H9.5a3.5 3.5 0 000 7h5a3.5 3.5 0 010 7H7" /></Ico>;
}

export function Key({ size = 16 }: { size?: number }) {
  return <Ico size={size}><path d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" /></Ico>;
}

export function Trash({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M3 6h18M8 6V4a2 2 0 012-2h4a2 2 0 012 2v2m3 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6h14" /><path d="M10 11v6m4-6v6" /></Ico>;
}

export function Folder({ size = 13 }: { size?: number }) {
  return <Ico size={size}><path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2v11z" /></Ico>;
}

export function FileMinus({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z" /><path d="M14 2v6h6" /><path d="M8 14h8" /></Ico>;
}

export function ChevronDown({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M6 9l6 6 6-6" /></Ico>;
}

export function Layers({ size = 16 }: { size?: number }) {
  return <Ico size={size}><path d="M12 2L2 7l10 5 10-5-10-5z" /><path d="M2 17l10 5 10-5" /><path d="M2 12l10 5 10-5" /></Ico>;
}

export function StatusIcon({ status, size = 10 }: { status: string; size?: number }) {
  switch (status) {
    case "completed": return <Check size={size} />;
    case "running":
    case "executing":
    case "waiting_for_gate":
    case "planning":
    case "verifying":
    case "publishing":
    case "merging":
      return <Play size={size} />;
    case "failed":
    case "cancelled":
      return <XIcon size={size} />;
    default:
      return <Clock size={size} />;
  }
}

export function Pulse({ color = "#3B82F6", size = 7 }: { color?: string; size?: number }) {
  return (
    <span style={{ position: "relative", display: "inline-flex", width: size, height: size }}>
      <span style={{
        position: "absolute", inset: 0, borderRadius: "50%",
        background: color, opacity: 0.4,
        animation: "ping 1.5s cubic-bezier(0,0,0.2,1) infinite"
      }} />
      <span style={{
        position: "relative", width: size, height: size,
        borderRadius: "50%", background: color
      }} />
    </span>
  );
}

export function Dot({ status, size = 7 }: { status: string; size?: number }) {
  const color = statusColor(status).dot;
  if (
    status === "running"
    || status === "executing"
    || status === "waiting_for_gate"
    || status === "planning"
    || status === "verifying"
    || status === "publishing"
    || status === "merging"
  ) {
    return <Pulse color={color} size={size} />;
  }
  return (
    <span style={{
      display: "inline-block", width: size, height: size,
      borderRadius: "50%", background: color
    }} />
  );
}

export interface StatusColors {
  dot: string;
  bg: string;
  border: string;
  text: string;
}

// ─── New icons from grove_gui_final ──────────────────────────────────

export function Commit({ size = 13 }: { size?: number }) {
  return <Ico size={size}><circle cx="12" cy="12" r="3" /><path d="M3 12h6m6 0h6" /></Ico>;
}

export function PullRequest({ size = 13 }: { size?: number }) {
  return <Ico size={size}><circle cx="18" cy="18" r="3" /><circle cx="6" cy="6" r="3" /><path d="M13 6h3a2 2 0 012 2v7" /><path d="M6 9v12" /></Ico>;
}

export function Upload({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" /><polyline points="17 8 12 3 7 8" /><line x1="12" y1="3" x2="12" y2="15" /></Ico>;
}

export function Refresh({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M1 4v6h6" /><path d="M3.51 15a9 9 0 100-6.68L1 10" /></Ico>;
}

export function SplitView({ size = 12 }: { size?: number }) {
  return <Ico size={size}><rect x="3" y="3" width="18" height="18" rx="2" /><path d="M12 3v18" /></Ico>;
}

export function WrapText({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M3 6h18" /><path d="M3 12h15a3 3 0 110 6h-4" /><polyline points="16 16 14 18 16 20" /><path d="M3 18h7" /></Ico>;
}

export function FoldAll({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M3 6h18" /><path d="M3 18h18" /><path d="M12 9v6" /><path d="M9 12h6" /></Ico>;
}

export function Copy({ size = 12 }: { size?: number }) {
  return <Ico size={size}><rect x="9" y="9" width="13" height="13" rx="2" /><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1" /></Ico>;
}

export function Pin({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M12 17v5" /><path d="M9 2h6l-1 7h4l-8 8 1-5H7z" /></Ico>;
}

export function Archive({ size = 12 }: { size?: number }) {
  return <Ico size={size}><polyline points="21 8 21 21 3 21 3 8" /><rect x="1" y="3" width="22" height="5" /><line x1="10" y1="12" x2="14" y2="12" /></Ico>;
}

export function LinkIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71" /><path d="M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71" /></Ico>;
}

export function ForkIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size}><circle cx="12" cy="18" r="3" /><circle cx="6" cy="6" r="3" /><circle cx="18" cy="6" r="3" /><path d="M18 9v1a2 2 0 01-2 2H8a2 2 0 01-2-2V9" /><path d="M12 12v3" /></Ico>;
}

export function Worktree({ size = 12 }: { size?: number }) {
  return <Ico size={size}><rect x="3" y="3" width="7" height="7" rx="1" /><rect x="14" y="3" width="7" height="7" rx="1" /><rect x="3" y="14" width="7" height="7" rx="1" /><path d="M14 14h7v7h-7z" strokeDasharray="3 2" /></Ico>;
}

export function HandIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M18 11V6a2 2 0 00-4 0v1M14 10V4a2 2 0 00-4 0v6M10 10V6a2 2 0 00-4 0v8l-1.46-2.44a2 2 0 00-3.24 2.34L6.3 21h11.18A2.5 2.5 0 0020 18.5V13a2 2 0 00-4 0" /></Ico>;
}

export function HDotsIcon({ size = 14 }: { size?: number }) {
  return <Ico size={size} sw={3}><circle cx="5" cy="12" r="0.5" /><circle cx="12" cy="12" r="0.5" /><circle cx="19" cy="12" r="0.5" /></Ico>;
}

export function VDotsIcon({ size = 14 }: { size?: number }) {
  return <Ico size={size} sw={3}><circle cx="12" cy="5" r="0.5" /><circle cx="12" cy="12" r="0.5" /><circle cx="12" cy="19" r="0.5" /></Ico>;
}

export function Maximize({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M8 3H5a2 2 0 00-2 2v3m18 0V5a2 2 0 00-2-2h-3m0 18h3a2 2 0 002-2v-3M3 16v3a2 2 0 002 2h3" /></Ico>;
}

export function Pencil({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7" /><path d="M18.5 2.5a2.12 2.12 0 013 3L12 15l-4 1 1-4z" /></Ico>;
}

export function InfoCircle({ size = 12 }: { size?: number }) {
  return <Ico size={size}><circle cx="12" cy="12" r="10" /><path d="M12 8v4" /><path d="M12 16h.01" /></Ico>;
}

export function Download({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" /><polyline points="7 10 12 15 17 10" /><line x1="12" y1="15" x2="12" y2="3" /></Ico>;
}

export function Zap({ size = 16 }: { size?: number }) {
  return (
    <Ico size={size} fill="none">
      <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
    </Ico>
  );
}

export function Shield({ size = 14 }: { size?: number }) {
  return <Ico size={size}><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" /></Ico>;
}

export function FileText({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" /><path d="M14 2v6h6" /><path d="M16 13H8" /><path d="M16 17H8" /><path d="M10 9H8" /></Ico>;
}

export function BarChart({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M12 20V10" /><path d="M18 20V4" /><path d="M6 20v-4" /></Ico>;
}

/** Kanban board icon — three vertical columns with cards. */
export function KanbanIcon({ size = 16 }: { size?: number }) {
  return <Ico size={size}><rect x="3" y="3" width="5" height="11" rx="1" /><rect x="9.5" y="3" width="5" height="7" rx="1" /><rect x="16" y="3" width="5" height="14" rx="1" /></Ico>;
}

export function ArrowUp({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M12 19V5m-7 7l7-7 7 7" /></Ico>;
}

export function ArrowDown({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M12 5v14m7-7l-7 7-7-7" /></Ico>;
}

export function Sparkles({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M12 2l2.09 6.26L20 10l-5.91 1.74L12 18l-2.09-6.26L4 10l5.91-1.74z" /><path d="M18 14l1 3 3 1-3 1-1 3-1-3-3-1 3-1z" /></Ico>;
}

export function Loader({ size = 12 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" style={{ animation: "spin 1s linear infinite" }}>
      <path d="M21 12a9 9 0 11-6.219-8.56" />
    </svg>
  );
}

export function ChevronUp({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M18 15l-6-6-6 6" /></Ico>;
}

export function Kbd({ children }: { children: React.ReactNode }) {
  return (
    <span style={{
      fontSize: 9, background: "rgba(255,255,255,0.04)",
      padding: "1px 5px", borderRadius: 3,
      fontFamily: "'SF Mono','Menlo','Monaco',monospace",
      color: "#52575F",
    }}>
      {children}
    </span>
  );
}

export function StreamIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M22 12h-4l-3 9L9 3l-3 9H2" /></Ico>;
}

// ─── Graph / Pipeline icons ─────────────────────────────────────────

export function HexagonIcon({ size = 14 }: { size?: number }) {
  return <Ico size={size}><path d="M12 2l8.66 5v10L12 22l-8.66-5V7z" /></Ico>;
}

export function PauseIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size} sw={2}><rect x="6" y="4" width="4" height="16" rx="1" /><rect x="14" y="4" width="4" height="16" rx="1" /></Ico>;
}

export function PlayIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size} fill="currentColor" stroke="none"><path d="M6 4l14 8-14 8V4z" /></Ico>;
}

export function StopIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size} sw={2}><rect x="4" y="4" width="16" height="16" rx="2" /></Ico>;
}

export function RestartIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M1 4v6h6" /><path d="M23 20v-6h-6" /><path d="M20.49 9A9 9 0 005.64 5.64L1 10m22 4l-4.64 4.36A9 9 0 013.51 15" /></Ico>;
}

export function BugIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M8 2l1.88 1.88M16 2l-1.88 1.88" /><path d="M9 7.13v-1a3.003 3.003 0 116 0v1" /><path d="M12 20c-3.3 0-6-2.7-6-6v-3a4 4 0 014-4h4a4 4 0 014 4v3c0 3.3-2.7 6-6 6z" /><path d="M12 20v-9" /><path d="M6.53 9C4.6 8.8 3 7.1 3 5" /><path d="M6 13H2" /><path d="M3 21c0-2.1 1.7-3.9 3.8-4" /><path d="M17.47 9c1.93-.2 3.53-1.9 3.53-4" /><path d="M18 13h4" /><path d="M21 21c0-2.1-1.7-3.9-3.8-4" /></Ico>;
}

export function BuildIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z" /></Ico>;
}

export function VerdictIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2" /><rect x="9" y="3" width="6" height="4" rx="1" /><path d="M9 14l2 2 4-4" /></Ico>;
}

export function JudgeIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M12 3v2m0 14v2M5.636 5.636l1.414 1.414m9.9 9.9l1.414 1.414M3 12h2m14 0h2M5.636 18.364l1.414-1.414m9.9-9.9l1.414-1.414" /><circle cx="12" cy="12" r="4" /></Ico>;
}

export function GradeIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01z" /></Ico>;
}

export function LockIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size}><rect x="3" y="11" width="18" height="11" rx="2" /><path d="M7 11V7a5 5 0 0110 0v4" /></Ico>;
}

export function GitBranchIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size}><path d="M6 3v12" /><circle cx="18" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><path d="M18 9a9 9 0 01-9 9" /></Ico>;
}

export function GitCommitIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size}><circle cx="12" cy="12" r="4" /><path d="M1.05 12H7" /><path d="M17.01 12h5.95" /></Ico>;
}

export function GitMergeIcon({ size = 12 }: { size?: number }) {
  return <Ico size={size}><circle cx="18" cy="18" r="3" /><circle cx="6" cy="6" r="3" /><path d="M6 21V9a9 9 0 009 9" /></Ico>;
}

export function statusColor(status: string): StatusColors {
  switch (status) {
    case "completed":
      return { dot: "#31B97B", bg: "rgba(49,185,123,0.08)", border: "rgba(49,185,123,0.15)", text: "#31B97B" };
    case "running":
    case "executing":
    case "waiting_for_gate":
    case "planning":
    case "verifying":
    case "publishing":
    case "merging":
      return { dot: "#3B82F6", bg: "rgba(59,130,246,0.08)", border: "rgba(59,130,246,0.15)", text: "#3B82F6" };
    case "failed":
    case "cancelled":
      return { dot: "#EF4444", bg: "rgba(239,68,68,0.08)", border: "rgba(239,68,68,0.15)", text: "#EF4444" };
    default:
      return { dot: "#71767F", bg: "rgba(113,118,127,0.06)", border: "rgba(113,118,127,0.12)", text: "#71767F" };
  }
}
