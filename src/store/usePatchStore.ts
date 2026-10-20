import { create } from "zustand";
import { persist } from "zustand/middleware";
import { useUndoStore } from "./useUndoStore";

/** One physical fixture in the user's rig: which library fixture it is, which of
 * its modes it's running in, and where it's addressed. */
export interface PatchedFixture {
  id: string;
  fixtureId: string;
  modeIndex: number;
  /** DMX start address, 1-512. */
  address: number;
  universe: number;
  /** Stable, user-assignable fixture number. */
  fixtureNumber: number;
  name: string;
  invertPan: boolean;
  invertTilt: boolean;
  swapPanTilt: boolean;
}

interface PatchState {
  fixtures: PatchedFixture[];
  addFixture: (f: Omit<PatchedFixture, "id" | "fixtureNumber"> & { fixtureNumber?: number }) => PatchedFixture;
  removeFixture: (id: string) => void;
  /** Changes an existing fixture's address/universe/orientation without
   * unpatching and repatching it (keeps its id, fixture number and name). */
  repatch: (id: string, changes: Partial<Pick<PatchedFixture, "address" | "universe" | "invertPan" | "invertTilt" | "swapPanTilt">>) => void;
  /** Replaces the whole patch list (e.g. loading a show file), bypassing undo. */
  replaceAll: (fixtures: PatchedFixture[]) => void;
  /** Next unused fixture number (max existing + 1, or 1 if empty). */
  nextFixtureNumber: () => number;
}

export const usePatchStore = create<PatchState>()(
  persist(
    (set, get) => ({
      fixtures: [],
      addFixture: (f) => {
        const fixture: PatchedFixture = {
          ...f,
          id: crypto.randomUUID(),
          fixtureNumber: f.fixtureNumber ?? get().nextFixtureNumber(),
        };
        useUndoStore.getState().push({
          label: `Patch ${fixture.name}`,
          redo: () => set((s) => ({ fixtures: [...s.fixtures, fixture] })),
          undo: () => set((s) => ({ fixtures: s.fixtures.filter((x) => x.id !== fixture.id) })),
        });
        return fixture;
      },
      removeFixture: (id) => {
        const removed = get().fixtures.find((f) => f.id === id);
        if (!removed) return;
        useUndoStore.getState().push({
          label: `Unpatch ${removed.name}`,
          redo: () => set((s) => ({ fixtures: s.fixtures.filter((f) => f.id !== id) })),
          undo: () => set((s) => ({ fixtures: [...s.fixtures, removed] })),
        });
      },
      repatch: (id, changes) => {
        const before = get().fixtures.find((f) => f.id === id);
        if (!before) return;
        const after = { ...before, ...changes };
        useUndoStore.getState().push({
          label: `Re-patch ${before.name}`,
          redo: () => set((s) => ({ fixtures: s.fixtures.map((f) => (f.id === id ? after : f)) })),
          undo: () => set((s) => ({ fixtures: s.fixtures.map((f) => (f.id === id ? before : f)) })),
        });
      },
      replaceAll: (fixtures) => set({ fixtures }),
      nextFixtureNumber: () => get().fixtures.reduce((max, f) => Math.max(max, f.fixtureNumber), 0) + 1,
    }),
    { name: "lumendmx-patch" },
  ),
);
