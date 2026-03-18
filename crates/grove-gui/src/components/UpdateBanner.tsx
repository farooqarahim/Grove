import { C } from "@/lib/theme";
import { useAppUpdate } from "@/lib/useAppUpdate";
import type { CSSProperties } from "react";

export function UpdateBanner() {
  const { available, progress, error, readyToRestart, downloadAndInstall, restartApp, checking } =
    useAppUpdate();

  if (checking || (!available && !error)) return null;

  if (error) {
    return null; // Silently ignore update check failures
  }

  if (readyToRestart) {
    return (
      <div style={bannerStyle}>
        <span style={textStyle}>Update installed. Restart to apply.</span>
        <button style={btnStyle} onClick={restartApp}>
          Restart Now
        </button>
      </div>
    );
  }

  if (progress !== null) {
    return (
      <div style={bannerStyle}>
        <span style={textStyle}>Downloading update... {progress}%</span>
        <div style={progressTrack}>
          <div style={{ ...progressBar, width: `${progress}%` }} />
        </div>
      </div>
    );
  }

  if (available) {
    return (
      <div style={bannerStyle}>
        <span style={textStyle}>
          Grove {available.version} is available
        </span>
        <button style={btnStyle} onClick={downloadAndInstall}>
          Update
        </button>
      </div>
    );
  }

  return null;
}

const bannerStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 10,
  padding: "6px 14px",
  background: C.blueDim,
  borderBottom: `1px solid ${C.border}`,
  fontSize: 12,
  flexShrink: 0,
};

const textStyle: CSSProperties = {
  color: C.text1,
  flex: 1,
};

const btnStyle: CSSProperties = {
  background: C.blue,
  color: "#fff",
  border: "none",
  borderRadius: 4,
  padding: "3px 10px",
  fontSize: 11,
  fontWeight: 600,
  cursor: "pointer",
  flexShrink: 0,
};

const progressTrack: CSSProperties = {
  width: 120,
  height: 4,
  borderRadius: 2,
  background: C.border,
  overflow: "hidden",
  flexShrink: 0,
};

const progressBar: CSSProperties = {
  height: "100%",
  background: C.blue,
  borderRadius: 2,
  transition: "width 0.2s ease",
};
