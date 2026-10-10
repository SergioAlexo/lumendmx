import { create } from "zustand";
import * as dmx from "../lib/dmx";
import * as engineApi from "../lib/engine";
import { PRIMARY_UNIVERSE_ID } from "../lib/constants";
import {
  showfileAutosave,
  showfileNew,
  showfileOpen,
  showfileRecent,
  showfileRestoreSession,
  showfileSave,
} from "../lib/showfile/api";
import type {
  DriverConfig,
  PatchIndexEntry,
  RecentShowEntry,
  ShowFile,
  ShowSettings,
  UniverseConfig,
  Workspace,
} from "../lib/showfile/generated";
import { assetsToLegacy, legacyToAssets } from "../lib/showfile/legacyAssets";
import { getAllFixtures, useFixtureLibraryStore } from "./useFixtureLibraryStore";
import { useLibraryStore } from "./useLibraryStore";
import { usePatchStore } from "./usePatchStore";

/** Stable reference for "no show open yet" selectors (`s.meta?.universes ??
 * EMPTY_UNIVERSES`). A fresh `[]` literal in the selector itself would give
 * useSyncExternalStore a new snapshot identity every render and spin into an
 * infinite re-render loop. */
export const EMPTY_UNIVERSES: UniverseConfig[] = [];

const AUTOSAVE_INTERVAL_MS = 60_000;
/** How often to check the engine for unsaved group/preset/cuelist edits. */
const ENGINE_DIRTY_POLL_MS = 1_000;

/** Everything in a `ShowFile` except `patch` and `legacyAssets`, which live in
 * (and are read live from) `usePatchStore`/`useLibraryStore` instead of being
 * duplicated in this store's state. */
export interface ShowMeta {
  schemaVersion: number;
  id: string;
  name: string;
  createdAt: string;
  modifiedAt: string;
  universes: UniverseConfig[];
  workspaces: Workspace[];
  settings: ShowSettings;
}

function toMeta(show: ShowFile): ShowMeta {
  return {
    schemaVersion: show.schemaVersion,
    id: show.id,
    name: show.name,
    createdAt: show.createdAt,
    modifiedAt: show.modifiedAt,
    universes: show.universes,
    workspaces: show.workspaces,
    settings: show.settings,
  };
}

/** Groups/presets/cuelists/tempo/pixel maps live in the Rust engine, so a complete show file
 * has to fetch them. `engineRevision` is the engine's data revision the
 * fetch reflects (read first, so an edit racing the save still reads as
 * unsaved afterwards). */
async function assembleShowFile(meta: ShowMeta): Promise<{ show: ShowFile; engineRevision: number }> {
  const engineRevision = await engineApi.showDataRevision();
  const engineData = await engineApi.exportShowData();
  return {
    show: {
      ...meta,
      patch: usePatchStore.getState().fixtures,
      legacyAssets: assetsToLegacy(useLibraryStore.getState().assets),
      ...engineData,
    },
    engineRevision,
  };
}

/** The engine's data revision as of the last save/load; any other value
 * means unsaved group/preset/cuelist edits. Null while a show is loading. */
let savedEngineRevision: number | null = null;

/** True while a show is being loaded into `usePatchStore`/`useLibraryStore`,
 * so their change subscriptions below don't mark the freshly-opened show dirty. */
let applyingShow = false;

/** Loads `show` into the patch/library stores and its engine-owned data
 * (groups, presets, cuelists, tempo, pixel maps) into the engine. */
async function applyShow(show: ShowFile): Promise<void> {
  applyingShow = true;
  try {
    usePatchStore.getState().replaceAll(show.patch);
    useLibraryStore.getState().replaceAll(legacyToAssets(show.legacyAssets));
  } finally {
    applyingShow = false;
  }
  savedEngineRevision = null;
  await engineApi.loadShowData({
    groups: show.groups,
    presets: show.presets,
    cuelists: show.cuelists,
    bpm: show.bpm,
    pixelMaps: show.pixelMaps,
    playbacks: show.playbacks,
    releaseOnPageChange: show.releaseOnPageChange,
  });
  savedEngineRevision = await engineApi.showDataRevision();
}

/** Spins up (or re-spins-up) each of the show's universes' output drivers to
 * match what's on disk. Best-effort: a universe whose driver fails to start
 * (e.g. its serial port isn't plugged in) is logged, not fatal to loading the
 * show — the Setup screen will show it as disconnected. */
async function activateUniverses(show: ShowFile): Promise<void> {
  await Promise.all(
    show.universes.map(async (universe) => {
      try {
        await dmx.configureUniverse(universe.id, universe.driver);
      } catch (err) {
        console.warn(`Failed to start universe ${universe.id} (${universe.name}):`, err);
      }
    }),
  );
}

interface ShowState {
  meta: ShowMeta | null;
  path: string | null;
  dirty: boolean;
  saving: boolean;
  recent: RecentShowEntry[];
  /** Bumped every time a show is loaded, so views holding engine data they
   * fetched once (cuelists, presets, groups) can remount and re-fetch. */
  loadCount: number;

  init: () => Promise<void>;
  newShow: () => Promise<void>;
  openShow: (path?: string) => Promise<void>;
  save: () => Promise<void>;
  saveAs: () => Promise<void>;
  refreshRecent: () => Promise<void>;

  /** Reconfigures a universe's driver, live and in the show's saved state. */
  setUniverseDriver: (universeId: number, driver: DriverConfig) => Promise<void>;
  /** Adds a new universe (starts with no driver) and returns its id. */
  addUniverse: (name: string) => number;
  /** Stops a universe's driver and drops it from the show entirely. */
  removeUniverseEntirely: (universeId: number) => void;
}

let autosaveTimer: ReturnType<typeof setInterval> | null = null;
let engineDirtyTimer: ReturnType<typeof setInterval> | null = null;

export const useShowStore = create<ShowState>((set, get) => ({
  meta: null,
  path: null,
  dirty: false,
  saving: false,
  recent: [],
  loadCount: 0,

  init: async () => {
    // Reopen the last session (unsaved work from an autosave, else the last
    // show file); only start a blank show when there's nothing to restore.
    let restored = null;
    try {
      restored = await showfileRestoreSession();
    } catch (err) {
      console.warn("Could not restore last session:", err);
    }
    if (restored) {
      await applyShow(restored.show);
      set((s) => ({
        meta: toMeta(restored.show),
        path: restored.path,
        dirty: restored.recovered,
        loadCount: s.loadCount + 1,
      }));
      void activateUniverses(restored.show);
    } else {
      await get().newShow();
    }
    void get().refreshRecent();

    if (!autosaveTimer) {
      autosaveTimer = setInterval(() => {
        const { meta, dirty } = get();
        if (!meta || !dirty) return;
        void assembleShowFile(meta)
          .then(({ show }) => showfileAutosave(show))
          .catch((err) => {
            console.warn("Autosave failed:", err);
          });
      }, AUTOSAVE_INTERVAL_MS);
    }

    if (!engineDirtyTimer) {
      engineDirtyTimer = setInterval(() => {
        const baseline = savedEngineRevision;
        if (baseline === null || get().dirty) return;
        void engineApi
          .showDataRevision()
          .then((revision) => {
            // Ignore a reply that raced a show load (baseline moved on).
            if (savedEngineRevision === baseline && revision !== baseline) set({ dirty: true });
          })
          .catch(() => undefined);
      }, ENGINE_DIRTY_POLL_MS);
    }
  },

  newShow: async () => {
    try {
      const show = await showfileNew();
      await applyShow(show);
      set((s) => ({ meta: toMeta(show), path: null, dirty: false, loadCount: s.loadCount + 1 }));
      void activateUniverses(show);
    } catch (err) {
      // Expected outside the Tauri runtime (plain browser preview).
      console.warn("Show backend unavailable:", err);
    }
  },

  openShow: async (path) => {
    const [show, usedPath] = await showfileOpen(path);
    await applyShow(show);
    set((s) => ({ meta: toMeta(show), path: usedPath, dirty: false, loadCount: s.loadCount + 1 }));
    void get().refreshRecent();
    void activateUniverses(show);
  },

  save: async () => {
    const { meta, path } = get();
    if (!meta) return;
    set({ saving: true });
    try {
      const { show, engineRevision } = await assembleShowFile(meta);
      const [usedPath, saved] = await showfileSave(show, path ?? undefined);
      savedEngineRevision = engineRevision;
      set({ meta: toMeta(saved), path: usedPath, dirty: false });
      void get().refreshRecent();
    } finally {
      set({ saving: false });
    }
  },

  saveAs: async () => {
    const { meta } = get();
    if (!meta) return;
    set({ saving: true });
    try {
      const { show, engineRevision } = await assembleShowFile(meta);
      const [usedPath, saved] = await showfileSave(show, undefined);
      savedEngineRevision = engineRevision;
      set({ meta: toMeta(saved), path: usedPath, dirty: false });
      void get().refreshRecent();
    } finally {
      set({ saving: false });
    }
  },

  refreshRecent: async () => {
    try {
      const recent = await showfileRecent();
      set({ recent });
    } catch (err) {
      console.warn("Could not load recent shows:", err);
    }
  },

  setUniverseDriver: async (universeId, driver) => {
    await dmx.configureUniverse(universeId, driver);
    const { meta } = get();
    if (!meta) return;
    const universes = meta.universes.map((u) => (u.id === universeId ? { ...u, driver } : u));
    set({ meta: { ...meta, universes }, dirty: true });
  },

  addUniverse: (name) => {
    const { meta } = get();
    if (!meta) throw new Error("No show open");
    const nextId = meta.universes.reduce((max, u) => Math.max(max, u.id), 0) + 1;
    const universe: UniverseConfig = { id: nextId, name, driver: { kind: "null" } };
    set({ meta: { ...meta, universes: [...meta.universes, universe] }, dirty: true });
    return nextId;
  },

  removeUniverseEntirely: (universeId) => {
    void dmx.removeUniverse(universeId);
    const { meta } = get();
    if (!meta) return;
    const universes = meta.universes.filter((u) => u.id !== universeId);
    set({ meta: { ...meta, universes }, dirty: true });
  },
}));

// `usePatchStore` rehydrates its persisted state asynchronously on startup,
// which fires this subscription once before any real edit happens; skip
// marking dirty until that initial hydration has actually finished.
let patchHydrated = usePatchStore.persist.hasHydrated();
usePatchStore.persist.onFinishHydration(() => {
  patchHydrated = true;
});

function markDirtyUnlessApplyingShow() {
  if (!applyingShow) {
    useShowStore.setState({ dirty: true });
  }
}

usePatchStore.subscribe(() => {
  if (patchHydrated) markDirtyUnlessApplyingShow();
  syncEnginePatch();
});
useLibraryStore.subscribe(markDirtyUnlessApplyingShow);

/** Pushes the current patch to the engine as (a) which channels are HTP
 * (a fixture's dimmer channel) for the merge, per universe, and (b) the
 * fixture-number -> attribute-channel index the command line's
 * `1 THRU 8 @ 50` and Phase 5's presets resolve against. */
function syncEnginePatch(): void {
  const fixtures = usePatchStore.getState().fixtures;
  const allFixtures = getAllFixtures();
  const htpChannelsByUniverse = new Map<number, Set<number>>();

  const patchIndex: PatchIndexEntry[] = fixtures.map((instance) => {
    const def = allFixtures.find((f) => f.id === instance.fixtureId);
    const mode = def?.modes[instance.modeIndex];
    let intensityChannel: number | null = null;
    const attributeChannels: Record<string, number> = {};
    // Multi-cell fixtures: cell number -> that cell's attribute channels.
    const cellChannels = new Map<number, Record<string, number>>();
    if (mode) {
      let htpChannels = htpChannelsByUniverse.get(instance.universe);
      if (!htpChannels) {
        htpChannels = new Set<number>();
        htpChannelsByUniverse.set(instance.universe, htpChannels);
      }
      for (const ch of mode.channels) {
        const channel = instance.address + ch.offset - 1;
        if (ch.type === "dimmer") {
          htpChannels.add(channel);
          if (intensityChannel === null) intensityChannel = channel;
        }
        // First occurrence of each attribute wins, matching the patch
        // panel's own unionChannels() convention for multi-channel fixtures.
        if (!(ch.type in attributeChannels)) attributeChannels[ch.type] = channel;
        if (ch.cell) {
          const cell = cellChannels.get(ch.cell) ?? {};
          if (!(ch.type in cell)) cell[ch.type] = channel;
          cellChannels.set(ch.cell, cell);
        }
      }
    }
    return {
      fixtureNumber: instance.fixtureNumber,
      universe: instance.universe,
      intensityChannel,
      attributeChannels,
      cells: [...cellChannels.entries()].sort(([a], [b]) => a - b).map(([, channels]) => channels),
    };
  });

  for (const [universe, htpChannels] of htpChannelsByUniverse) {
    void engineApi.setHtpChannels(universe, [...htpChannels]);
  }
  // A universe with no patched dimmer channels still needs its HTP set
  // cleared (e.g. after unpatching the last fixture there).
  if (!htpChannelsByUniverse.has(PRIMARY_UNIVERSE_ID)) {
    void engineApi.setHtpChannels(PRIMARY_UNIVERSE_ID, []);
  }
  void engineApi.setPatchIndex(patchIndex);
}

useFixtureLibraryStore.subscribe(syncEnginePatch);
