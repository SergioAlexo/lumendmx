import { create } from "zustand";
import type { Update } from "@tauri-apps/plugin-updater";
import * as appUpdate from "../lib/appUpdate";
import type { UpdateProgress } from "../lib/appUpdate";
import { decideInstall, shouldRunDeferred } from "../lib/updatePolicy";
import { useShowModeStore } from "./useShowModeStore";

interface AppUpdateStore {
  currentVersion: string;
  update: Update | null;
  checking: boolean;
  /** Downloading the package (no restart yet). */
  downloading: boolean;
  /** Installing, which ends in a restart. */
  installing: boolean;
  /** The package is on disk and ready to install instantly. */
  downloaded: boolean;
  /** Install requested during show mode: runs when show mode turns off. */
  deferred: boolean;
  progress: UpdateProgress | null;
  error: string | null;
  checked: boolean;
  /** The check accepted a different (older) version: a rollback target. */
  rollbackCandidate: boolean;
  /** Set at startup when the app version changed since the last run. */
  previousVersion: string | null;
  /** Where the pre-update data backup went, once one was made. */
  backupPath: string | null;

  checkNow: () => Promise<void>;
  /** Looks for a manifest version older than this one (see `checkForUpdate`). */
  checkForRollback: () => Promise<void>;
  /** Downloads the update in the background without installing it. */
  download: () => Promise<void>;
  /** Installs now, or defers until show mode turns off. */
  install: () => Promise<void>;
  cancelDeferred: () => void;
  noteVersionChange: () => Promise<void>;
  dismissVersionNotice: () => void;
}

function messageOf(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export const useAppUpdateStore = create<AppUpdateStore>((set, get) => ({
  currentVersion: "",
  update: null,
  checking: false,
  downloading: false,
  installing: false,
  downloaded: false,
  deferred: false,
  progress: null,
  error: null,
  checked: false,
  rollbackCandidate: false,
  previousVersion: null,
  backupPath: null,

  checkNow: async () => {
    set({ checking: true, error: null });
    try {
      const [update, currentVersion] = await Promise.all([
        appUpdate.checkForUpdate(),
        appUpdate.getCurrentVersion(),
      ]);
      set({ update, currentVersion, checked: true, downloaded: false, rollbackCandidate: false });
    } catch (err) {
      // Expected outside the Tauri runtime, or offline -- try again later.
      set({ error: messageOf(err) });
    } finally {
      set({ checking: false });
    }
  },

  checkForRollback: async () => {
    set({ checking: true, error: null });
    try {
      const [update, currentVersion] = await Promise.all([
        appUpdate.checkForUpdate(true),
        appUpdate.getCurrentVersion(),
      ]);
      // Without downgrades allowed this would be a normal upgrade; flag only
      // a version that isn't newer.
      set({ update, currentVersion, checked: true, downloaded: false, rollbackCandidate: update !== null });
    } catch (err) {
      set({ error: messageOf(err) });
    } finally {
      set({ checking: false });
    }
  },

  download: async () => {
    const update = get().update;
    if (!update || get().downloading || get().downloaded) return;
    set({ downloading: true, error: null });
    try {
      await appUpdate.downloadUpdate(update, (progress) => set({ progress }));
      set({ downloaded: true });
    } catch (err) {
      set({ error: messageOf(err) });
    } finally {
      set({ downloading: false, progress: null });
    }
  },

  install: async () => {
    const { update, installing } = get();
    const decision = decideInstall({
      showMode: useShowModeStore.getState().locked,
      hasUpdate: update !== null,
      installing,
    });
    if (decision.action === "none" || !update) return;
    if (decision.action === "defer") {
      // Get the package ready now so the install later is quick.
      set({ deferred: true, error: null });
      void get().download();
      return;
    }

    set({ installing: true, deferred: false, error: null });
    try {
      const version = get().currentVersion || (await appUpdate.getCurrentVersion());
      const backupPath = await appUpdate.prepareBackup(version).catch(() => null);
      set({ backupPath });
      if (get().downloaded) {
        await appUpdate.installDownloaded(update);
      } else {
        await appUpdate.installUpdate(update, (progress) => set({ progress }));
      }
    } catch (err) {
      set({ error: messageOf(err) });
    } finally {
      set({ installing: false, progress: null });
    }
  },

  cancelDeferred: () => set({ deferred: false }),
  dismissVersionNotice: () => set({ previousVersion: null }),

  noteVersionChange: async () => {
    try {
      const current = await appUpdate.getCurrentVersion();
      const previous = await appUpdate.recordVersion(current);
      set({ currentVersion: current, previousVersion: previous });
    } catch {
      // Not in the Tauri runtime.
    }
  },
}));

// A deferred install runs the moment show mode turns off.
useShowModeStore.subscribe((state) => {
  const { update, installing, deferred, install } = useAppUpdateStore.getState();
  if (shouldRunDeferred(deferred, { showMode: state.locked, hasUpdate: update !== null, installing })) {
    void install();
  }
});
