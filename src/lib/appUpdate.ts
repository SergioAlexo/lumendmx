import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { sessionCleanShutdown } from "./reliability";

export interface UpdateProgress {
  downloaded: number;
  total: number;
}

/** Repo whose releases host the installers (matches the updater endpoint in
 * tauri.conf.json); used for the manual rollback link. */
export const RELEASES_REPO = "sergioalexo/lumendmx";

/** Single source of truth for update state (unlike MediaFetch, which keeps a
 * separate GitHub-polling command purely for the version badge alongside the
 * plugin's own check() -- here the one `Update` object drives both).
 *
 * `allowDowngrades` accepts any manifest version different from the running
 * one: the way to roll back, once the release owner points the manifest at
 * an older build. */
export async function checkForUpdate(allowDowngrades = false): Promise<Update | null> {
  return check(allowDowngrades ? { allowDowngrades: true } : undefined);
}

export async function getCurrentVersion(): Promise<string> {
  return getVersion();
}

function trackProgress(onProgress: (progress: UpdateProgress) => void) {
  let downloaded = 0;
  let total = 0;
  return (event: { event: string; data?: { contentLength?: number; chunkLength?: number } }) => {
    if (event.event === "Started") {
      total = event.data?.contentLength ?? 0;
      onProgress({ downloaded: 0, total });
    } else if (event.event === "Progress") {
      downloaded += event.data?.chunkLength ?? 0;
      onProgress({ downloaded, total });
    }
  };
}

/** Downloads the package without installing it, so the install itself can
 * wait for a safe moment (not during a show). */
export async function downloadUpdate(update: Update, onProgress: (progress: UpdateProgress) => void): Promise<void> {
  const handler = trackProgress(onProgress);
  await update.download((event) => handler(event));
}

/** Copies preferences, the recent list and autosaves aside before the
 * binary is replaced. */
export function prepareBackup(version: string): Promise<string> {
  return invoke("update_prepare_backup", { version });
}

/** The version that ran before this launch, if it differs from the running one. */
export function recordVersion(current: string): Promise<string | null> {
  return invoke("update_record_version", { current });
}

/** Installs an already-downloaded update and relaunches. */
export async function installDownloaded(update: Update): Promise<void> {
  // The installer can end the process without the normal exit hook.
  await sessionCleanShutdown().catch(() => undefined);
  await update.install();
  await relaunch();
}

export async function installUpdate(
  update: Update,
  onProgress: (progress: UpdateProgress) => void,
): Promise<void> {
  await downloadUpdate(update, onProgress);
  await installDownloaded(update);
}
