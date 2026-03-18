import { useRef, useEffect } from "react";
import { Kbd } from "@/components/ui/icons";
import { C } from "@/lib/theme";

export interface MenuItem {
  icon?: React.ReactNode;
  label?: string;
  kbd?: string;
  sep?: boolean;
  action?: () => void;
}

interface MenuProps {
  items: MenuItem[];
  onClose: () => void;
  position: { top: number; left: number };
}

export function Menu({ items, onClose, position }: MenuProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [onClose]);

  return (
    <div
      ref={ref}
      style={{
        position: "fixed",
        top: position.top,
        left: position.left,
        zIndex: 999,
        minWidth: 220,
        background: C.surfaceHover,
        borderRadius: 8,
        padding: "4px 0",
        backdropFilter: "blur(16px)",
      }}
    >
      {items.map((item, i) =>
        item.sep ? (
          <div key={i} style={{ height: 1, background: "rgba(255,255,255,0.04)", margin: "4px 8px" }} />
        ) : (
          <div
            key={i}
            onClick={() => {
              item.action?.();
              onClose();
            }}
            className="hover-row"
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              padding: "6px 12px",
              margin: "0 4px",
              borderRadius: 6,
              cursor: "pointer",
              fontSize: 12,
              color: C.text2,
            }}
          >
            <span style={{ color: C.text3, width: 16, display: "flex", justifyContent: "center" }}>
              {item.icon}
            </span>
            <span style={{ flex: 1 }}>{item.label}</span>
            {item.kbd && <Kbd>{item.kbd}</Kbd>}
          </div>
        )
      )}
    </div>
  );
}
