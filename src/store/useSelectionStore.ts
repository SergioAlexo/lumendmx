import { create } from "zustand";
import * as engineApi from "../lib/engine";

/**
 * The shared fixture selection for visual windows (2D stage plot, Output
 * sheet, 3D view). Keyed by fixture number - the same number the command line
 * and the engine's programmer selection use.
 *
 * Selecting here also drives the engine's programmer selection (it runs the
 * equivalent `1 + 3 + 7` command line), so a fixture clicked on the plot is
 * what the next `@ 50` or colour pick affects. The flow is one-way: the
 * engine's own selection is not read back, so other windows that want to join
 * in should call `select`/`toggle` here on their own clicks and read
 * `selected` for highlighting. Adopting it elsewhere is a drop-in change.
 */
interface SelectionState {
  selected: number[];
  /** Replaces the selection. */
  select: (fixtureNumbers: number[]) => void;
  /** Adds the fixture if absent, removes it if present (shift/ctrl click). */
  toggle: (fixtureNumber: number) => void;
  clear: () => void;
}

/** Best-effort: outside the Tauri runtime (browser preview) there's no engine. */
function syncToEngine(fixtureNumbers: number[]): void {
  if (fixtureNumbers.length === 0) return;
  void engineApi.executeCommand(fixtureNumbers.join(" + ")).catch(() => undefined);
}

export const useSelectionStore = create<SelectionState>((set, get) => ({
  selected: [],
  select: (fixtureNumbers) => {
    const next = [...new Set(fixtureNumbers)].sort((a, b) => a - b);
    set({ selected: next });
    syncToEngine(next);
  },
  toggle: (fixtureNumber) => {
    const current = get().selected;
    get().select(current.includes(fixtureNumber) ? current.filter((n) => n !== fixtureNumber) : [...current, fixtureNumber]);
  },
  clear: () => set({ selected: [] }),
}));
