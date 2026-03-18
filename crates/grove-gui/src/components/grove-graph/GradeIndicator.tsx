const MONO = "'JetBrains Mono', 'Fira Code', 'SF Mono', monospace";

const SIZE_MAP = {
  sm: { fontSize: 10, padding: "2px 5px", lineHeight: "16px" },
  md: { fontSize: 11, padding: "2px 7px", lineHeight: "18px" },
  lg: { fontSize: 13, padding: "3px 9px", lineHeight: "20px" },
};

interface GradeIndicatorProps {
  grade: number | null;
  size?: "sm" | "md" | "lg";
}

export function GradeIndicator({ grade, size = "md" }: GradeIndicatorProps) {
  if (grade === null) return null;
  const c = grade >= 9 ? "#3ecf8e" : grade >= 7 ? "#f59e0b" : grade >= 4 ? "#fb923c" : "#f87171";
  const sz = SIZE_MAP[size];

  return (
    <span
      style={{
        fontFamily: MONO,
        fontWeight: 600,
        color: c,
        background: `${c}12`,
        border: `1px solid ${c}30`,
        padding: sz.padding,
        fontSize: sz.fontSize,
        lineHeight: sz.lineHeight,
        borderRadius: 4,
        whiteSpace: "nowrap",
        display: "inline-block",
      }}
    >
      {grade}/10
    </span>
  );
}
