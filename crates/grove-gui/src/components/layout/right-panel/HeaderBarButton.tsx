import { C } from "@/lib/theme";
import type { ToolbarTone } from "./constants";

interface HeaderBarButtonProps {
  icon: React.ReactNode;
  label?: string;
  title?: string;
  tone?: ToolbarTone;
  compact?: boolean;
  disabled?: boolean;
  onClick?: () => void;
}

export function HeaderBarButton({
  icon,
  label,
  title,
  tone = "neutral",
  compact = false,
  disabled = false,
  onClick,
}: HeaderBarButtonProps) {
  const toneStyle = tone === "primary"
    ? { background: "rgba(221,224,231,0.94)", color: C.base }
    : tone === "success"
      ? { background: C.accentDim, color: "#A7F3D0" }
      : tone === "danger"
        ? { background: C.dangerDim, color: "#FECACA" }
        : tone === "info"
          ? { background: C.blueDim, color: "#DBEAFE" }
          : { background: "rgba(255,255,255,0.05)", color: C.text1 };

  return (
    <button
      onClick={onClick}
      disabled={disabled}
      title={title ?? label}
      style={{
        height: 30,
        minWidth: compact ? 30 : undefined,
        padding: compact ? "0 9px" : "0 10px",
        borderRadius: 2,
        border: "none",
        background: toneStyle.background,
        color: toneStyle.color,
        fontSize: 11,
        fontWeight: 600,
        cursor: disabled ? "not-allowed" : "pointer",
        opacity: disabled ? 0.4 : 1,
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        gap: label ? 6 : 0,
        whiteSpace: "nowrap",
        flexShrink: 0,
      }}
    >
      {icon}
      {label}
    </button>
  );
}
