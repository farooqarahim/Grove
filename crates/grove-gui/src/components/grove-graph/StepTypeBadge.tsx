interface StepTypeBadgeProps {
  stepType: string;
}

interface TypeMeta {
  label: string;
  bg: string;
  text: string;
  border: string;
  iconPath: string;
}

function getTypeMeta(stepType: string): TypeMeta {
  switch (stepType) {
    case "code":
      return {
        label: "Code",
        bg: "rgba(96,165,250,0.08)",
        text: "#60a5fa",
        border: "rgba(96,165,250,0.2)",
        iconPath: "M4 7l3 3-3 3m5 0h4",
      };
    case "config":
      return {
        label: "Config",
        bg: "rgba(167,139,250,0.1)",
        text: "#a78bfa",
        border: "rgba(167,139,250,0.2)",
        iconPath: "M6 3a3 3 0 100 6 3 3 0 000-6zM1.5 6h1.17m6.66 0H10.5M6 9v1.5m0-9V0m-3.18.82l1.06 1.06m6.24 6.24l1.06 1.06M.82 9.18l1.06-1.06m6.24-6.24L9.18.82",
      };
    case "docs":
      return {
        label: "Docs",
        bg: "rgba(62,207,142,0.08)",
        text: "#3ecf8e",
        border: "rgba(62,207,142,0.2)",
        iconPath: "M3 1h6l3 3v8a1 1 0 01-1 1H3a1 1 0 01-1-1V2a1 1 0 011-1zm6 0v3h3M4 7h5M4 9h5",
      };
    case "infra":
      return {
        label: "Infra",
        bg: "rgba(251,146,60,0.08)",
        text: "#fb923c",
        border: "rgba(251,146,60,0.2)",
        iconPath: "M1 2h12v3H1zm0 4h12v3H1zm0 4h12v3H1zM3 3.5h.01M3 7.5h.01M3 11.5h.01",
      };
    case "test":
      return {
        label: "Test",
        bg: "rgba(167,139,250,0.1)",
        text: "#a78bfa",
        border: "rgba(167,139,250,0.2)",
        iconPath: "M5 1v4l-3 7a1 1 0 001 1.37h8A1 1 0 0012 12L9 5V1M4 1h6M3.5 8h7",
      };
    default:
      return {
        label: stepType.charAt(0).toUpperCase() + stepType.slice(1),
        bg: "rgba(113,118,127,0.08)",
        text: "#8b8d98",
        border: "rgba(113,118,127,0.15)",
        iconPath: "M7 1a6 6 0 100 12A6 6 0 007 1zm0 4v4m0 2h.01",
      };
  }
}

export function StepTypeBadge({ stepType }: StepTypeBadgeProps) {
  const meta = getTypeMeta(stepType);

  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        borderRadius: 4,
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: "0.03em",
        padding: "1px 6px 1px 5px",
        lineHeight: "16px",
        whiteSpace: "nowrap",
        background: meta.bg,
        color: meta.text,
        border: `1px solid ${meta.border}`,
      }}
    >
      <svg
        width={10}
        height={10}
        viewBox="0 0 14 14"
        fill="none"
        stroke="currentColor"
        strokeWidth={1.6}
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d={meta.iconPath} />
      </svg>
      {meta.label}
    </span>
  );
}
