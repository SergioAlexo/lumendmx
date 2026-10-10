/** Pure startup and autosave decisions, separate from the store so they can
 * be tested without Tauri. */

export type StartupPlan = "adopt-live" | "restore-from-disk";

/** A live session snapshot in Rust means this is a webview reload, not a cold
 * start: the engine already holds the running show, and loading a show from
 * disk would overwrite it. */
export function startupPlan(hasLiveSession: boolean): StartupPlan {
  return hasLiveSession ? "adopt-live" : "restore-from-disk";
}

export type RecoveryNotice = "crash" | "unsaved" | null;

/** What to tell the operator after restoring from disk. `recovered` means the
 * show came from an autosave with changes newer than the last save. */
export function recoveryNotice(recovered: boolean, uncleanShutdown: boolean): RecoveryNotice {
  if (uncleanShutdown) return "crash";
  return recovered ? "unsaved" : null;
}

export interface AutosaveClock {
  dirty: boolean;
  nowMs: number;
  lastAutosaveMs: number;
  intervalS: number;
}

export function autosaveDue({ dirty, nowMs, lastAutosaveMs, intervalS }: AutosaveClock): boolean {
  return dirty && nowMs - lastAutosaveMs >= intervalS * 1000;
}
