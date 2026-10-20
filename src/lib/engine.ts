import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { EngineFrame, Group, PatchIndexEntry, ShowEngineData, TempoStatus } from "./showfile/generated";

export function setHtpChannels(universe: number, channels: number[]): Promise<void> {
  return invoke("engine_set_htp_channels", { universe, channels });
}

export function setPatchIndex(entries: PatchIndexEntry[]): Promise<void> {
  return invoke("engine_set_patch_index", { entries });
}

/** Immediate set into the manual layer (the Fixtures panel's sliders). */
export function setChannels(universe: number, values: Record<number, number>): Promise<void> {
  return invoke("engine_set_channels", { universe, values });
}

export function getUniverseData(universe: number): Promise<number[]> {
  return invoke("engine_get_universe_data", { universe });
}

export function onUniverseFrame(cb: (frame: EngineFrame) => void) {
  return listen<EngineFrame>("engine://universe-frame", (event) => cb(event.payload));
}

// --- Groups ---

export function recordGroup(name: string, fixtureNumbers: number[]): Promise<number> {
  return invoke("engine_record_group", { name, fixtureNumbers });
}

export function listGroups(): Promise<Group[]> {
  return invoke("engine_list_groups");
}

export function renameGroup(id: number, name: string): Promise<void> {
  return invoke("engine_rename_group", { id, name });
}

export function deleteGroup(id: number): Promise<void> {
  return invoke("engine_delete_group", { id });
}

// --- Show data ---

export function exportShowData(): Promise<ShowEngineData> {
  return invoke("engine_export_show_data");
}

export function loadShowData(data: ShowEngineData): Promise<void> {
  return invoke("engine_load_show_data", { data });
}

/** Bumped by every engine-side show edit; compare to detect unsaved work. */
export function showDataRevision(): Promise<number> {
  return invoke("engine_show_data_revision");
}

// --- Masters and tempo ---

/** 0-100%. Scales every intensity channel of the output. */
export function setGrandMaster(percent: number): Promise<void> {
  return invoke("engine_set_grand_master", { percent });
}

export function grandMaster(): Promise<number> {
  return invoke("engine_grand_master");
}

export function tempoStatus(): Promise<TempoStatus> {
  return invoke("engine_tempo_status");
}

export function setBpm(bpm: number): Promise<void> {
  return invoke("engine_set_bpm", { bpm });
}

/** Passes the tap's own timestamp so IPC latency doesn't skew the interval. */
export function tapTempo(): Promise<number> {
  return invoke("engine_tap_tempo", { nowMs: performance.now() });
}

export type { EngineFrame, Group, PatchIndexEntry, ShowEngineData, TempoStatus };
