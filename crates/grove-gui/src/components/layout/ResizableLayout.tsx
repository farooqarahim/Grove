import { useState, useEffect, useRef, useCallback } from "react";
import { C } from "@/lib/theme";

interface ResizableLayoutProps {
  sidebar: React.ReactNode;
  main: React.ReactNode;
  right: React.ReactNode;
}

function useResizable(initial: number, min: number, max: number, side: "right" | "left" = "right"): [number, (e: React.MouseEvent) => void] {
  const [width, setWidth] = useState(initial);
  const dragging = useRef(false);
  const startX = useRef(0);
  const startW = useRef(0);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;
    startX.current = e.clientX;
    startW.current = width;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  }, [width]);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!dragging.current) return;
      const dx = side === "right" ? e.clientX - startX.current : startX.current - e.clientX;
      setWidth(Math.min(max, Math.max(min, startW.current + dx)));
    };
    const onUp = () => {
      dragging.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [min, max, side]);

  useEffect(() => {
    setWidth((current) => Math.min(max, Math.max(min, current)));
  }, [min, max]);

  return [width, onMouseDown];
}

function ResizeHandle({ onMouseDown, side }: { onMouseDown: (e: React.MouseEvent) => void; side: "right" | "left" }) {
  return (
    <div
      onMouseDown={onMouseDown}
      style={{
        position: "absolute", top: 0, bottom: 0,
        [side]: -2, width: 5,
        cursor: "col-resize", zIndex: 10,
      }}
    >
      <div
        className="resize-line"
        style={{
          position: "absolute", top: 0, bottom: 0,
          left: 2, width: 1,
          background: "transparent", transition: "background 0.15s",
        }}
      />
    </div>
  );
}

export function ResizableLayout({ sidebar, main, right }: ResizableLayoutProps) {
  const [col1W, col1Drag] = useResizable(248, 200, 340, "right");
  const [viewportWidth, setViewportWidth] = useState(() => window.innerWidth);

  useEffect(() => {
    const onResize = () => setViewportWidth(window.innerWidth);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  const rightMin = Math.min(500, Math.round(viewportWidth * 0.3));
  const rightMax = 500;
  const rightInitial = Math.min(rightMax, Math.max(rightMin, 360));
  const [col3W, col3Drag] = useResizable(rightInitial, rightMin, rightMax, "left");

  return (
    <div style={{ flex: 1, display: "flex", overflow: "hidden" }}>
      {/* Col 1 — Sidebar */}
      <div style={{
        width: col1W, minWidth: 200, maxWidth: 340,
        background: C.surface,
        display: "flex", flexDirection: "column",
        position: "relative", flexShrink: 0,
      }}>
        <ResizeHandle onMouseDown={col1Drag} side="right" />
        {sidebar}
      </div>

      {/* Col 2 — Main */}
      <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0, background: C.base }}>
        {main}
      </div>

      {/* Col 3 — Right */}
      <div style={{
        width: col3W, minWidth: rightMin, maxWidth: rightMax,
        background: C.surface,
        display: "flex", flexDirection: "column",
        position: "relative", flexShrink: 0,
      }}>
        <ResizeHandle onMouseDown={col3Drag} side="left" />
        {right}
      </div>
    </div>
  );
}
