import { create } from "zustand";
import * as showApi from "../lib/show";
import type {
  FixtureTypeSummary,
  Group,
  PatchedFixture,
  RecentShowEntry,
  ShowSnapshot,
  UniverseConfig,
} from "../lib/showfile/generated";
import { getAllFixtures, useFixtureLibraryStore } from "./useFixtureLibraryStore";

/** Stable reference for "no show loaded yet" selectors (`s.show?.universes ??
 * EMPTY_UNIVERSES`). A fresh `[]` literal in the selector itself would give
 * useSyncExternalStore a new snapshot identity every render and spin into an
 * infinite re-render loop. */
export const EMPTY_UNIVERSES: UniverseConfig[] = [];
export const EMPTY_PATCH: PatchedFixture[] = [];
export const EMPTY_GROUPS: Group[] = [];

interface ShowState {
  /** The show as Rust last reported it; null until the first fetch. */
  show: ShowSnapshot | null;
  fixtureTypes: FixtureTypeSummary[];
  recent: RecentShowEntry[];
  /** The last failed action's message, shown until the next action. */
  error: string | null;
  /** Bumped when a different show is opened, so views can drop local state. */
  loadCount: number;

  init: () => Promise<void>;
  refresh: () => Promise<void>;
  /** Runs a show command, recording a failure in `error` instead of throwing. */
  run: <T>(action: () => Promise<T>) => Promise<T | undefined>;
}

/** Pushes the frontend's fixture library (bundled + imported) to Rust. */
async function pushLibrary(): Promise<void> {
  const rejected = await showApi.setLibrary(getAllFixtures());
  if (rejected.length > 0) console.warn("Fixture definitions Rust couldn't read:", rejected);
}

export const useShowStore = create<ShowState>((set, get) => ({
  show: null,
  fixtureTypes: [],
  recent: [],
  error: null,
  loadCount: 0,

  init: async () => {
    try {
      await showApi.onShowChanged(() => void get().refresh());
      await pushLibrary();
      useFixtureLibraryStore.subscribe(() => void pushLibrary());
      await get().refresh();
    } catch (err) {
      // Expected outside the Tauri runtime (plain browser preview).
      console.warn("Show backend unavailable:", err);
    }
  },

  refresh: async () => {
    const [show, fixtureTypes, recent] = await Promise.all([
      showApi.getShow(),
      showApi.fixtureTypes(),
      showApi.recentShows(),
    ]);
    const previous = get().show;
    // Refreshes can race; never step back to an older revision.
    if (previous && previous.id === show.id && show.revision < previous.revision) return;
    const reloaded = previous !== null && previous.id !== show.id;
    set((s) => ({ show, fixtureTypes, recent, loadCount: reloaded ? s.loadCount + 1 : s.loadCount }));
  },

  run: async (action) => {
    set({ error: null });
    try {
      return await action();
    } catch (err) {
      set({ error: err instanceof Error ? err.message : String(err) });
      return undefined;
    }
  },
}));
