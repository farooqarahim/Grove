/** Configuration for a single terminal tab's PTY session. */
export interface TabConfig {
  /** Working directory for the spawned process. */
  cwd: string;
  /** Command binary (for agent tabs). Omit for default shell. */
  command?: string;
  /** Command arguments. */
  args?: string[];
  /** Shell binary override (for shell tabs). */
  shell?: string;
  /** Extra environment variables. */
  env?: Record<string, string>;
}

/** A single terminal tab within a TerminalColumn. */
export interface TerminalTab {
  /** PtyId: "{conversationId}:{tabIndex}" */
  id: string;
  /** 0 = agent tab, 1+ = shell tabs. */
  tabIndex: number;
  /** Display label: "Agent", "Shell 1", etc. */
  label: string;
  /** Tab type. */
  kind: "agent" | "shell";
  /** PTY spawn configuration. */
  config: TabConfig;
  /** Current lifecycle status. */
  status: "starting" | "running" | "exited";
  /** Process exit code (set when status is "exited"). */
  exitCode?: number;
}

/** Payload from the pty:output:{pty_id} Tauri event. */
export interface PtyOutputPayload {
  data: string;
}

/** Payload from the pty:exit:{pty_id} Tauri event. */
export interface PtyExitPayload {
  code: number | null;
}

/** Result from the pty_open Tauri command. */
export interface PtyOpenResult {
  pty_id: string;
  is_new: boolean;
}
