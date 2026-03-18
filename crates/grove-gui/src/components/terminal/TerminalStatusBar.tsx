import { RotateCw } from "lucide-react";
import { C } from "@/lib/theme";

interface TerminalStatusBarProps {
  status: "starting" | "running" | "exited";
  exitCode?: number;
  onRestart: () => void;
}

export function TerminalStatusBar({ status, exitCode, onRestart }: TerminalStatusBarProps) {
  const dotColor =
    status === "running"
      ? C.accent
      : status === "exited" && (exitCode ?? 0) !== 0
        ? C.danger
        : C.surfaceRaised;

  const label =
    status === "starting"
      ? "Starting..."
      : status === "running"
        ? "Running"
        : exitCode !== undefined
          ? `Exited (${exitCode})`
          : "Exited";

  return (
    <div
      className="flex items-center justify-between shrink-0"
      style={{
        height: 28,
        padding: "0 12px",
        borderTop: `1px solid ${C.border}`,
        background: C.surface,
        fontSize: 11,
        color: C.text4,
        fontFamily: C.mono,
      }}
    >
      <div className="flex items-center gap-2">
        <span
          style={{
            width: 6,
            height: 6,
            borderRadius: "50%",
            background: dotColor,
          }}
        />
        <span>{label}</span>
      </div>

      {status === "exited" && (
        <button
          onClick={onRestart}
          className="flex items-center gap-1 cursor-pointer"
          style={{
            fontSize: 11,
            color: C.accent,
            background: "transparent",
            border: "none",
            fontFamily: C.mono,
            padding: "2px 6px",
            borderRadius: 3,
          }}
        >
          <RotateCw size={10} />
          Restart
        </button>
      )}
    </div>
  );
}
