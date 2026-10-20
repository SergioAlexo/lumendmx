import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AddFixtures,
  DriverConfig,
  FixtureTypeSummary,
  FixtureUpdate,
  Group,
  PatchedFixture,
  RecentShowEntry,
  ShowSnapshot,
  UniverseConfig,
} from "./showfile/generated";

// Wrappers for the show commands (src-tauri/src/show/commands.rs). The show
// lives in Rust; every edit comes back as a `show://changed` event.

export function getShow(): Promise<ShowSnapshot> {
  return invoke("show_get");
}

export function onShowChanged(cb: (revision: number) => void) {
  return listen<number>("show://changed", (event) => cb(event.payload));
}

export function fixtureTypes(): Promise<FixtureTypeSummary[]> {
  return invoke("show_fixture_types");
}

/** Replaces the fixture library Rust resolves the patch against. Returns the
 * definitions it couldn't read. */
export function setLibrary(definitions: unknown[]): Promise<string[]> {
  return invoke("show_set_library", { definitions });
}

export function addFixtures(request: AddFixtures): Promise<PatchedFixture[]> {
  return invoke("show_add_fixtures", { request });
}

export function updateFixture(number: number, update: FixtureUpdate): Promise<PatchedFixture> {
  return invoke("show_update_fixture", { number, update });
}

export function removeFixtures(numbers: number[]): Promise<void> {
  return invoke("show_remove_fixtures", { numbers });
}

export function createGroup(name: string, fixtureNumbers: number[]): Promise<Group> {
  return invoke("show_create_group", { name, fixtureNumbers });
}

export function updateGroup(id: number, changes: { name?: string; fixtureNumbers?: number[] }): Promise<Group> {
  return invoke("show_update_group", { id, name: changes.name ?? null, fixtureNumbers: changes.fixtureNumbers ?? null });
}

export function deleteGroup(id: number): Promise<void> {
  return invoke("show_delete_group", { id });
}

export function undo(): Promise<string> {
  return invoke("show_undo");
}

export function redo(): Promise<string> {
  return invoke("show_redo");
}

export function addUniverse(driver: DriverConfig, name?: string): Promise<UniverseConfig> {
  return invoke("show_add_universe", { name: name ?? null, driver });
}

export function updateUniverse(id: number, changes: { name?: string; driver?: DriverConfig }): Promise<UniverseConfig> {
  return invoke("show_update_universe", { id, name: changes.name ?? null, driver: changes.driver ?? null });
}

export function removeUniverse(id: number): Promise<void> {
  return invoke("show_remove_universe", { id });
}

export function renameShow(name: string): Promise<void> {
  return invoke("show_rename", { name });
}

export function newShow(): Promise<void> {
  return invoke("show_new");
}

/** Opens `path`, or asks with a file dialog when omitted. False: cancelled. */
export function openShow(path?: string): Promise<boolean> {
  return invoke("show_open", { path: path ?? null });
}

/** Saves to the show's file, or asks where (first save / `saveAs`). False: cancelled. */
export function saveShow(saveAs = false): Promise<boolean> {
  return invoke("show_save", { saveAs });
}

export function recentShows(): Promise<RecentShowEntry[]> {
  return invoke("show_recent");
}
