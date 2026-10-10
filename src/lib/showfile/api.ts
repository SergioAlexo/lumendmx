import { invoke } from "@tauri-apps/api/core";
import type { RecentShowEntry, RestoredSession, ShowFile } from "./generated";

export function showfileNew(): Promise<ShowFile> {
  return invoke("showfile_new");
}

/** Opens `path`, or prompts the user with a native file dialog when omitted. */
export function showfileOpen(path?: string): Promise<[ShowFile, string]> {
  return invoke("showfile_open", { path: path ?? null });
}

/** Saves to `path`, or prompts with a native Save dialog when omitted (Save As
 * / first save). Returns the path used and the show with `schemaVersion`/
 * `modifiedAt` refreshed. */
export function showfileSave(show: ShowFile, path?: string): Promise<[string, ShowFile]> {
  return invoke("showfile_save", { show, path: path ?? null });
}

export function showfileAutosave(show: ShowFile): Promise<void> {
  return invoke("showfile_autosave", { show });
}

export function showfileRecent(): Promise<RecentShowEntry[]> {
  return invoke("showfile_recent");
}

/** The last session to reopen on launch (newest autosave or last show file),
 * or null when there's nothing to restore. */
export function showfileRestoreSession(): Promise<RestoredSession | null> {
  return invoke("showfile_restore_session");
}

export type { RecentShowEntry, RestoredSession, ShowFile } from "./generated";
