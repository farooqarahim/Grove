import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { useEffect, useState, useCallback } from "react";

export interface UpdateState {
  /** Whether we're currently checking for updates */
  checking: boolean;
  /** Available update, null if none or not checked yet */
  available: { version: string; body: string | null } | null;
  /** Download + install progress (0-100), null when not downloading */
  progress: number | null;
  /** Error message if check or install failed */
  error: string | null;
  /** Whether the update has been downloaded and is ready to install */
  readyToRestart: boolean;
}

export function useAppUpdate() {
  const [state, setState] = useState<UpdateState>({
    checking: false,
    available: null,
    progress: null,
    error: null,
    readyToRestart: false,
  });

  // Internal ref to the Update object (needed for download+install)
  const [updateObj, setUpdateObj] = useState<Update | null>(null);

  const checkForUpdate = useCallback(async () => {
    setState(s => ({ ...s, checking: true, error: null }));
    try {
      const update = await check();
      if (update) {
        setState(s => ({
          ...s,
          checking: false,
          available: { version: update.version, body: update.body ?? null },
        }));
        setUpdateObj(update);
      } else {
        setState(s => ({ ...s, checking: false, available: null }));
      }
    } catch (err) {
      setState(s => ({
        ...s,
        checking: false,
        error: err instanceof Error ? err.message : String(err),
      }));
    }
  }, []);

  const downloadAndInstall = useCallback(async () => {
    if (!updateObj) return;
    setState(s => ({ ...s, progress: 0, error: null }));
    try {
      let totalBytes = 0;
      let downloadedBytes = 0;
      await updateObj.downloadAndInstall((event) => {
        if (event.event === "Started") {
          totalBytes = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloadedBytes += event.data.chunkLength;
          const pct = totalBytes > 0 ? Math.round((downloadedBytes / totalBytes) * 100) : 0;
          setState(s => ({ ...s, progress: pct }));
        } else if (event.event === "Finished") {
          setState(s => ({ ...s, progress: 100, readyToRestart: true }));
        }
      });
    } catch (err) {
      setState(s => ({
        ...s,
        progress: null,
        error: err instanceof Error ? err.message : String(err),
      }));
    }
  }, [updateObj]);

  const restartApp = useCallback(async () => {
    await relaunch();
  }, []);

  // Check on mount (once, with a 5-second delay to avoid blocking startup)
  useEffect(() => {
    const timer = setTimeout(() => {
      checkForUpdate();
    }, 5000);
    return () => clearTimeout(timer);
  }, [checkForUpdate]);

  return { ...state, checkForUpdate, downloadAndInstall, restartApp };
}
