import { create } from "zustand";
import { checkEditAllowed, needsDeleteConfirmation } from "../lib/showGuard";
import * as reliability from "../lib/reliability";
import { usePrefsStore } from "./usePrefsStore";

interface ShowModeStore {
  /** Show mode: editing and destructive commands are disabled. Rust is the
   * source of truth and enforces it in the command handlers; this mirrors it. */
  locked: boolean;
  init: () => Promise<void>;
  setLocked: (locked: boolean) => Promise<void>;
}

export const useShowModeStore = create<ShowModeStore>((set) => ({
  locked: false,

  init: async () => {
    try {
      set({ locked: await reliability.showModeGet() });
    } catch (err) {
      console.warn("Show mode unavailable:", err);
    }
  },

  setLocked: async (locked) => {
    await reliability.showModeSet(locked);
    set({ locked });
  },
}));

export function isShowModeOn(): boolean {
  return useShowModeStore.getState().locked;
}

/** Call before an editing action. Returns false (and tells the operator why,
 * through the Messages log) when show mode forbids it. */
export function guardEdit(action: string): boolean {
  const result = checkEditAllowed(isShowModeOn(), action);
  if (!result.allowed && result.message) {
    void reliability.logWrite("warn", "show mode", result.message).catch(() => undefined);
  }
  return result.allowed;
}

/** Call before deleting recorded data: false if show mode blocks it or the
 * operator declines the confirmation (when "confirm on delete" is on). */
export function confirmDelete(what: string): boolean {
  if (!guardEdit(`delete ${what}`)) return false;
  const { prefs } = usePrefsStore.getState();
  if (!needsDeleteConfirmation(prefs.confirmOnDelete, isShowModeOn())) return true;
  return window.confirm(`Delete ${what}? This can't be undone.`);
}
