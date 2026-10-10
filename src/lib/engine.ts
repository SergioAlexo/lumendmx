import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  CommandResult,
  CuelistDto,
  CuelistKind,
  EffectDef,
  EffectInstance,
  EngineFrame,
  FaderMode,
  Group,
  PatchIndexEntry,
  PixelMap,
  PixelMapStatus,
  PlaybackStatus,
  Preset,
  PresetFamily,
  ShowEngineData,
  TempoStatus,
} from "./showfile/generated";
import type { LightAsset } from "./types";

export function setHtpChannels(universe: number, channels: number[]): Promise<void> {
  return invoke("engine_set_htp_channels", { universe, channels });
}

export function setPatchIndex(entries: PatchIndexEntry[]): Promise<void> {
  return invoke("engine_set_patch_index", { entries });
}

/** Immediate (unfaded) programmer set: sliders, color-preset buttons, the
 * Live AI console. */
export function setChannels(universe: number, values: Record<number, number>): Promise<void> {
  return invoke("engine_set_channels", { universe, values });
}

export function getUniverseData(universe: number): Promise<number[]> {
  return invoke("engine_get_universe_data", { universe });
}

export interface LegacyStepPayload {
  channels: Record<string, number>;
  fadeMs: number;
  holdMs: number;
}

/** Starts (or restarts) a Scene/Chase/FX asset as a frame-accurate engine
 * playback layer, replacing the old `chaseEngine.ts` setTimeout loop. */
export function triggerAsset(
  id: string,
  universe: number,
  steps: LegacyStepPayload[],
  looped: boolean,
): Promise<void> {
  return invoke("engine_trigger_asset", { id, universe, steps, looped });
}

export function stopAsset(id: string): Promise<void> {
  return invoke("engine_stop_asset", { id });
}

/** Converts a Scene/Chase/FX's steps into the shape `triggerAsset` expects. */
export function legacyAssetToSteps(asset: LightAsset): LegacyStepPayload[] {
  return asset.steps.map((step) => ({
    channels: { ...step.channels } as unknown as Record<string, number>,
    fadeMs: step.fade_time_ms,
    holdMs: step.hold_time_ms,
  }));
}

export function executeCommand(text: string): Promise<CommandResult> {
  return invoke("engine_execute_command", { text });
}

export function onUniverseFrame(cb: (frame: EngineFrame) => void) {
  return listen<EngineFrame>("engine://universe-frame", (event) => cb(event.payload));
}

// --- Groups (Phase 5) ---

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

export function applyGroupMaster(id: number, percent: number): Promise<string> {
  return invoke("engine_apply_group_master", { id, percent });
}

// --- Presets (Phase 5) ---

export function recordPreset(
  family: PresetFamily,
  name: string,
  fixtureNumbers: number[],
  color?: string,
): Promise<number> {
  return invoke("engine_record_preset", { family, name, fixtureNumbers, color: color ?? null });
}

export function updatePreset(id: number, fixtureNumbers: number[], color?: string): Promise<void> {
  return invoke("engine_update_preset", { id, fixtureNumbers, color: color ?? null });
}

export function renamePreset(id: number, name: string): Promise<void> {
  return invoke("engine_rename_preset", { id, name });
}

export function deletePreset(id: number): Promise<void> {
  return invoke("engine_delete_preset", { id });
}

export function listPresets(family?: PresetFamily): Promise<Preset[]> {
  return invoke("engine_list_presets", { family: family ?? null });
}

export function applyPreset(id: number, fixtureNumbers: number[]): Promise<number> {
  return invoke("engine_apply_preset", { id, fixtureNumbers });
}

// --- Cuelists and cues (Phase 6) ---

export function createCuelist(name: string, kind: CuelistKind, tracking: boolean): Promise<number> {
  return invoke("engine_create_cuelist", { name, kind, tracking });
}

export function listCuelists(): Promise<CuelistDto[]> {
  return invoke("engine_list_cuelists");
}

export function deleteCuelist(id: number): Promise<void> {
  return invoke("engine_delete_cuelist", { id });
}

export function renameCuelist(id: number, name: string): Promise<void> {
  return invoke("engine_rename_cuelist", { id, name });
}

export function setCuelistTracking(id: number, tracking: boolean): Promise<void> {
  return invoke("engine_set_cuelist_tracking", { id, tracking });
}

/** Sets which cuelist the command line's RECORD CUE/UPDATE/DELETE CUE/NEXT/
 * PREV operate against — call when a cuelist is selected in the UI. */
export function setCurrentCuelist(id: number | null): Promise<void> {
  return invoke("engine_set_current_cuelist", { id });
}

/** Records a cue from the given fixtures' currently-rendered state — the
 * same "record what you see" approach as `recordPreset`. Use
 * `setCuePresetReference` afterwards to turn a specific channel into a live
 * preset reference instead of a literal. */
export function recordCue(
  cuelistId: number,
  number: number,
  name: string,
  fixtureNumbers: number[],
  fadeInMs: number,
  fadeOutMs: number,
): Promise<void> {
  return invoke("engine_record_cue", { cuelistId, number, name, fixtureNumbers, fadeInMs, fadeOutMs });
}

export function deleteCue(cuelistId: number, number: number): Promise<void> {
  return invoke("engine_delete_cue", { cuelistId, number });
}

export function setCuePresetReference(
  cuelistId: number,
  cueNumber: number,
  fixtureNumber: number,
  attribute: string,
  presetId: number,
): Promise<void> {
  return invoke("engine_set_cue_preset_reference", { cuelistId, cueNumber, fixtureNumber, attribute, presetId });
}

/** Turns a linked cue value back into a literal (the preset's current value). */
export function unlinkCuePreset(cuelistId: number, cueNumber: number, universe: number, channel: number): Promise<void> {
  return invoke("engine_unlink_cue_preset", { cuelistId, cueNumber, universe, channel });
}

/** The engine's groups/presets/cuelists, for writing into a show file. */
export function exportShowData(): Promise<ShowEngineData> {
  return invoke("engine_export_show_data");
}

/** Replaces the engine's groups/presets/cuelists with a show's (and drops
 * running cue playbacks, which point at the outgoing cuelists). */
export function loadShowData(data: ShowEngineData): Promise<void> {
  return invoke("engine_load_show_data", { data });
}

/** Changes on every group/preset/cuelist edit; compare with the value at the
 * last save/load to detect unsaved engine-side changes. */
export function showDataRevision(): Promise<number> {
  return invoke("engine_show_data_revision");
}

export function setChaseTempo(cuelistId: number, bpm: number): Promise<void> {
  return invoke("engine_set_chase_tempo", { cuelistId, bpm });
}

// --- Playbacks (Phase 6) ---

export function createPlayback(cuelistId: number): Promise<number> {
  return invoke("engine_create_playback", { cuelistId });
}

export function deletePlayback(id: number): Promise<void> {
  return invoke("engine_delete_playback", { id });
}

export function listPlaybacks(): Promise<PlaybackStatus[]> {
  return invoke("engine_list_playbacks");
}

export function playbackGo(id: number): Promise<void> {
  return invoke("engine_playback_go", { id });
}

export function playbackGoBack(id: number): Promise<void> {
  return invoke("engine_playback_go_back", { id });
}

export function playbackRelease(id: number): Promise<void> {
  return invoke("engine_playback_release", { id });
}

export function playbackSetPaused(id: number, paused: boolean): Promise<void> {
  return invoke("engine_playback_set_paused", { id, paused });
}

export function playbackSetFader(id: number, percent: number): Promise<void> {
  return invoke("engine_playback_set_fader", { id, percent });
}

export function playbackSetFaderMode(id: number, mode: FaderMode): Promise<void> {
  return invoke("engine_playback_set_fader_mode", { id, mode });
}

// --- Tempo (Phase 7) ---

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

// --- Effects (Phase 7) ---

/** Runs `def` on `fixtureNumbers` (order = fan order) in the programmer. */
export function startEffect(def: EffectDef, fixtureNumbers: number[], presetId: number | null = null): Promise<number> {
  return invoke("engine_start_effect", { def, fixtureNumbers, presetId });
}

export function updateEffect(id: number, def: EffectDef): Promise<void> {
  return invoke("engine_update_effect", { id, def });
}

export function stopEffect(id: number): Promise<void> {
  return invoke("engine_stop_effect", { id });
}

export function listEffects(): Promise<EffectInstance[]> {
  return invoke("engine_list_effects");
}

export function recordEffectPreset(name: string, def: EffectDef): Promise<number> {
  return invoke("engine_record_effect_preset", { name, def });
}

export function updateEffectPreset(id: number, def: EffectDef): Promise<void> {
  return invoke("engine_update_effect_preset", { id, def });
}

// --- Pixel maps (Phase 7) ---

export function createPixelMap(name: string, width: number, height: number): Promise<number> {
  return invoke("engine_create_pixel_map", { name, width, height });
}

export function updatePixelMap(map: PixelMap): Promise<void> {
  return invoke("engine_update_pixel_map", { map });
}

export function deletePixelMap(id: number): Promise<void> {
  return invoke("engine_delete_pixel_map", { id });
}

export function listPixelMaps(): Promise<PixelMap[]> {
  return invoke("engine_list_pixel_maps");
}

/** Imports an image/GIF file into the map (scaled to its size). */
export async function importPixelMapMedia(id: number, file: File): Promise<void> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return invoke("engine_import_pixel_map_media", { id, data: btoa(binary) });
}

export function startPixelMap(id: number): Promise<void> {
  return invoke("engine_start_pixel_map", { id });
}

export function stopPixelMap(id: number): Promise<void> {
  return invoke("engine_stop_pixel_map", { id });
}

export function setPixelMapIntensity(id: number, percent: number): Promise<void> {
  return invoke("engine_set_pixel_map_intensity", { id, percent });
}

export function runningPixelMaps(): Promise<PixelMapStatus[]> {
  return invoke("engine_running_pixel_maps");
}

/** The map's current frame: width × height × 3 RGB bytes, row-major. */
export function pixelMapPreview(id: number): Promise<number[]> {
  return invoke("engine_pixel_map_preview", { id });
}

export type {
  CommandResult,
  CuelistDto,
  CuelistKind,
  CueDto,
  CueValue,
  CueValueEntry,
  ChaseDirection,
  EffectDef,
  EffectDirection,
  EffectInstance,
  EffectMode,
  EffectRate,
  EffectTarget,
  EngineFrame,
  Generator,
  MapDirection,
  PixelCell,
  PixelMap,
  PixelMapStatus,
  PositionShape,
  TempoStatus,
  Waveform,
  FaderMode,
  Group,
  PatchIndexEntry,
  PlaybackStatus,
  Preset,
  PresetFamily,
  ShowEngineData,
} from "./showfile/generated";
