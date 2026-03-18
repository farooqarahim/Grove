import { C } from "@/lib/theme";

interface EmptyStateProps {
  icon: React.ReactNode;
  title: string;
  description: string;
  action?: React.ReactNode;
}

export function EmptyState({ icon, title, description, action }: EmptyStateProps) {
  return (
    <div style={{
      flex: 1,
      display: "flex",
      flexDirection: "column",
      alignItems: "center",
      justifyContent: "center",
      gap: 12,
      padding: "24px 20px",
      textAlign: "center",
    }}>
      <div style={{
        width: 42,
        height: 42,
        borderRadius: 2,
        background: C.surfaceHover,
        color: C.text3,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}>
        {icon}
      </div>
      <div>
        <div style={{ fontSize: 12, fontWeight: 600, color: C.text2, marginBottom: 4 }}>
          {title}
        </div>
        <div style={{ fontSize: 11, color: C.text4, lineHeight: 1.5 }}>
          {description}
        </div>
      </div>
      {action}
    </div>
  );
}
