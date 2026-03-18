import type { RunRecord } from "@/types";

export const ACTIVE_STATES = ["executing", "waiting_for_gate", "planning", "verifying", "publishing", "merging"];

export type GitSourceKind =
  | "none"                // no project selected
  | "loading"             // conversation selected, runs query in flight
  | "conversation-empty"  // conversation selected, runs loaded but empty
  | "project"             // project only (no conversation)
  | "run";                // conversation + latestRun exists

export interface RightPanelProps {
  conversationId: string | null;
  projectRoot: string | null;
  conversationKind: "run" | "cli" | "hive_loom" | null;
  onOpenReview?: () => void;
  onOpenCommit?: () => void;
  onLatestRun?: (run: RunRecord | null) => void;
  headerActionsHost?: HTMLElement | null;
}

export type ToolbarTone = "neutral" | "primary" | "success" | "danger" | "info";

export async function openUrl(url: string) {
  try {
    const { open: openExternal } = await import("@tauri-apps/plugin-shell");
    await openExternal(url);
  } catch {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}
