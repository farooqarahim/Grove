import { statusColor } from "./icons";

interface TagProps {
  children: React.ReactNode;
  color?: string;
}

export function Tag({ children, color }: TagProps) {
  return (
    <span
      className="inline-flex items-center text-xs font-medium tracking-wide"
      style={{
        padding: "2px 8px", borderRadius: 4,
        lineHeight: "18px",
        background: color ? `${color}10` : "rgba(255,255,255,0.04)",
        color: color || "#A1A6AE",
      }}
    >
      {children}
    </span>
  );
}

export function StatusTag({ status }: { status: string }) {
  const sc = statusColor(status);
  return <Tag color={sc.text}>{status.charAt(0).toUpperCase() + status.slice(1)}</Tag>;
}
