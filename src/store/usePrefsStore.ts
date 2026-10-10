import { create } from "zustand";
import { applyPreferencesToRoot } from "../lib/prefsApply";
import * as reliability from "../lib/reliability";
import type { Preferences } from "../lib/showfile/generated";

/** Mirrors `Preferences::default()` in Rust, used until the real values load
 * (and when running outside the Tauri runtime). */
export const DEFAULT_PREFS: Preferences = {
  serialRateHz: 30,
  networkRateHz: 40,
  displayHz: 20,
  defaultFadeInMs: 1000,
  defaultFadeOutMs: 1000,
  goDebounceMs: 0,
  goResumesPaused: false,
  confirmOnDelete: true,
  autosaveIntervalS: 60,
  uiScale: 1,
  touchMode: false,
  window: null,
};

interface PrefsStore {
  prefs: Preferences;
  loaded: boolean;
  error: string | null;
  init: () => Promise<void>;
  /** Saves a change. Rust clamps out-of-range values; the clamped result is
   * what gets applied. */
  update: (patch: Partial<Preferences>) => Promise<void>;
}

function applyToDocument(prefs: Preferences) {
  if (typeof document !== "undefined") applyPreferencesToRoot(document.documentElement, prefs);
}

export const usePrefsStore = create<PrefsStore>((set, get) => ({
  prefs: DEFAULT_PREFS,
  loaded: false,
  error: null,

  init: async () => {
    try {
      const prefs = await reliability.prefsGet();
      set({ prefs, loaded: true });
      applyToDocument(prefs);
    } catch (err) {
      // Expected outside the Tauri runtime (plain browser preview).
      console.warn("Preferences unavailable:", err);
    }
  },

  update: async (patch) => {
    const next = { ...get().prefs, ...patch };
    // Apply optimistically so sliders feel immediate, then settle on what Rust kept.
    set({ prefs: next, error: null });
    applyToDocument(next);
    try {
      const saved = await reliability.prefsSet(next);
      set({ prefs: saved });
      applyToDocument(saved);
    } catch (err) {
      set({ error: err instanceof Error ? err.message : String(err) });
    }
  },
}));
