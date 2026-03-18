const MONO = "'JetBrains Mono', 'Fira Code', 'SF Mono', monospace";

interface GraphProgressBarProps {
  closedSteps: number;
  totalSteps: number;
  /** Optional label override. Defaults to "X/Y" */
  label?: string;
  /** Height of the progress bar track in px */
  height?: number;
}

export function GraphProgressBar({
  closedSteps,
  totalSteps,
  label,
  height = 3,
}: GraphProgressBarProps) {
  const pct = totalSteps > 0 ? (closedSteps / totalSteps) * 100 : 0;
  const barColor =
    closedSteps === totalSteps && totalSteps > 0
      ? "#3ecf8e"
      : closedSteps === 0
        ? "#5c5e6a"
        : "#fb923c";

  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
      <div
        style={{
          flex: 1,
          height,
          borderRadius: height / 2,
          background: "rgba(255,255,255,0.06)",
          overflow: "hidden",
          minWidth: 40,
        }}
      >
        <div
          style={{
            width: `${pct}%`,
            height: "100%",
            borderRadius: height / 2,
            background: barColor,
            transition: "width 0.5s cubic-bezier(0.16,1,0.3,1)",
          }}
        />
      </div>
      <span
        style={{
          fontFamily: MONO,
          fontSize: 10,
          color: "#5c5e6a",
          whiteSpace: "nowrap",
          flexShrink: 0,
          fontVariantNumeric: "tabular-nums",
        }}
      >
        {label ?? `${closedSteps}/${totalSteps}`}
      </span>
    </div>
  );
}
