use std::collections::{HashMap, HashSet};

use tauri::State;

use base64::Engine as _;

use super::cues::CuelistKind;
use super::effects::{EffectDef, EffectInstance};
use super::groups::Group;
use super::legacy_playback::{LegacyAsset, LegacyStep};
use super::manager::{
    CommandResult, CuelistDto, EngineManager, PatchIndexEntry, PixelMapStatus, PlaybackStatus, ShowEngineData,
};
use super::pixelmap::PixelMap;
use super::playback::FaderMode;
use super::presets::{Preset, PresetFamily};
use super::tempo::TempoStatus;

#[tauri::command]
pub fn engine_set_htp_channels(engine: State<EngineManager>, universe: u32, channels: Vec<u16>) {
    engine.set_htp_channels(universe, channels);
}

#[tauri::command]
pub fn engine_set_patch_index(engine: State<EngineManager>, entries: Vec<PatchIndexEntry>) {
    engine.set_patch_index(entries);
}

/// Immediate (unfaded) programmer set — sliders, color-preset buttons, the
/// Live AI console.
#[tauri::command]
pub fn engine_set_channels(engine: State<EngineManager>, universe: u32, values: HashMap<u16, u8>) {
    engine.set_channels(universe, &values);
}

#[tauri::command]
pub fn engine_get_universe_data(engine: State<EngineManager>, universe: u32) -> Vec<u8> {
    engine.get_universe_data(universe).to_vec()
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyStepDto {
    /// channel (1-512, as a string key, matching the show file's LegacyStep) -> value.
    pub channels: HashMap<String, u8>,
    pub fade_ms: f64,
    pub hold_ms: f64,
}

/// Starts (or restarts, if already running) a legacy Scene/Chase/FX asset as
/// an engine playback layer — the frame-accurate replacement for the old
/// `chaseEngine.ts` `setTimeout` loop.
#[tauri::command]
pub fn engine_trigger_asset(
    engine: State<EngineManager>,
    id: String,
    universe: u32,
    steps: Vec<LegacyStepDto>,
    looped: bool,
) -> Result<(), String> {
    let steps = steps
        .into_iter()
        .map(|s| {
            let channels = s
                .channels
                .into_iter()
                .map(|(k, v)| k.parse::<u16>().map(|ch| (ch, v)).map_err(|_| format!("Invalid channel '{k}'")))
                .collect::<Result<HashMap<u16, u8>, String>>()?;
            Ok(LegacyStep { channels, fade_ms: s.fade_ms, hold_ms: s.hold_ms })
        })
        .collect::<Result<Vec<LegacyStep>, String>>()?;

    engine.trigger_asset(id, universe, LegacyAsset { steps, loop_: looped });
    Ok(())
}

#[tauri::command]
pub fn engine_stop_asset(engine: State<EngineManager>, id: String) {
    engine.stop_asset(&id);
}

#[tauri::command]
pub fn engine_execute_command(engine: State<EngineManager>, text: String) -> CommandResult {
    engine.execute_command(&text)
}

// --- Groups (Phase 5) ---

#[tauri::command]
pub fn engine_record_group(engine: State<EngineManager>, name: String, fixture_numbers: Vec<u32>) -> u32 {
    engine.record_group(name, fixture_numbers)
}

#[tauri::command]
pub fn engine_list_groups(engine: State<EngineManager>) -> Vec<Group> {
    engine.list_groups()
}

#[tauri::command]
pub fn engine_rename_group(engine: State<EngineManager>, id: u32, name: String) -> Result<(), String> {
    engine.rename_group(id, name)
}

#[tauri::command]
pub fn engine_delete_group(engine: State<EngineManager>, id: u32) {
    engine.delete_group(id);
}

#[tauri::command]
pub fn engine_apply_group_master(engine: State<EngineManager>, id: u32, percent: f32) -> Result<String, String> {
    let (_ok, message) = engine.apply_group_master(id, percent)?;
    Ok(message)
}

// --- Presets (Phase 5) ---

#[tauri::command]
pub fn engine_record_preset(
    engine: State<EngineManager>,
    family: PresetFamily,
    name: String,
    fixture_numbers: Vec<u32>,
    color: Option<String>,
) -> u32 {
    engine.record_preset(family, name, &fixture_numbers.into_iter().collect::<HashSet<u32>>(), color)
}

#[tauri::command]
pub fn engine_update_preset(
    engine: State<EngineManager>,
    id: u32,
    fixture_numbers: Vec<u32>,
    color: Option<String>,
) -> Result<(), String> {
    engine.update_preset(id, &fixture_numbers.into_iter().collect::<HashSet<u32>>(), color)
}

#[tauri::command]
pub fn engine_rename_preset(engine: State<EngineManager>, id: u32, name: String) -> Result<(), String> {
    engine.rename_preset(id, name)
}

#[tauri::command]
pub fn engine_delete_preset(engine: State<EngineManager>, id: u32) {
    engine.delete_preset(id);
}

#[tauri::command]
pub fn engine_list_presets(engine: State<EngineManager>, family: Option<PresetFamily>) -> Vec<Preset> {
    engine.list_presets(family)
}

#[tauri::command]
pub fn engine_apply_preset(engine: State<EngineManager>, id: u32, fixture_numbers: Vec<u32>) -> Result<usize, String> {
    engine.apply_preset(id, &fixture_numbers.into_iter().collect::<HashSet<u32>>())
}

// --- Cuelists and cues (Phase 6) ---

#[tauri::command]
pub fn engine_create_cuelist(engine: State<EngineManager>, name: String, kind: CuelistKind, tracking: bool) -> u32 {
    engine.create_cuelist(name, kind, tracking)
}

#[tauri::command]
pub fn engine_list_cuelists(engine: State<EngineManager>) -> Vec<CuelistDto> {
    engine.list_cuelists()
}

#[tauri::command]
pub fn engine_delete_cuelist(engine: State<EngineManager>, id: u32) {
    engine.delete_cuelist(id);
}

#[tauri::command]
pub fn engine_rename_cuelist(engine: State<EngineManager>, id: u32, name: String) -> Result<(), String> {
    engine.rename_cuelist(id, name)
}

#[tauri::command]
pub fn engine_set_cuelist_tracking(engine: State<EngineManager>, id: u32, tracking: bool) -> Result<(), String> {
    engine.set_cuelist_tracking(id, tracking)
}

#[tauri::command]
pub fn engine_set_current_cuelist(engine: State<EngineManager>, id: Option<u32>) {
    engine.set_current_cuelist(id);
}

#[tauri::command]
pub fn engine_record_cue(
    engine: State<EngineManager>,
    cuelist_id: u32,
    number: f64,
    name: String,
    fixture_numbers: Vec<u32>,
    fade_in_ms: f64,
    fade_out_ms: f64,
) -> Result<(), String> {
    engine.record_cue(
        cuelist_id,
        number,
        name,
        &fixture_numbers.into_iter().collect::<HashSet<u32>>(),
        fade_in_ms,
        fade_out_ms,
    )
}

#[tauri::command]
pub fn engine_delete_cue(engine: State<EngineManager>, cuelist_id: u32, number: f64) -> Result<(), String> {
    engine.delete_cue(cuelist_id, number)
}

#[tauri::command]
pub fn engine_set_cue_preset_reference(
    engine: State<EngineManager>,
    cuelist_id: u32,
    cue_number: f64,
    fixture_number: u32,
    attribute: String,
    preset_id: u32,
) -> Result<(), String> {
    engine.set_cue_preset_reference(cuelist_id, cue_number, fixture_number, attribute, preset_id)
}

#[tauri::command]
pub fn engine_unlink_cue_preset(
    engine: State<EngineManager>,
    cuelist_id: u32,
    cue_number: f64,
    universe: u32,
    channel: u16,
) -> Result<(), String> {
    engine.unlink_cue_preset(cuelist_id, cue_number, universe, channel)
}

#[tauri::command]
pub fn engine_set_chase_tempo(engine: State<EngineManager>, cuelist_id: u32, bpm: f32) -> Result<(), String> {
    engine.set_chase_tempo(cuelist_id, bpm)
}

// --- Show data (groups/presets/cuelists saved in .lumen files) ---

#[tauri::command]
pub fn engine_export_show_data(engine: State<EngineManager>) -> ShowEngineData {
    engine.export_show_data()
}

#[tauri::command]
pub fn engine_load_show_data(engine: State<EngineManager>, data: ShowEngineData) {
    engine.load_show_data(data);
}

#[tauri::command]
pub fn engine_show_data_revision(engine: State<EngineManager>) -> u64 {
    engine.show_data_revision()
}

// --- Playbacks (Phase 6) ---

#[tauri::command]
pub fn engine_create_playback(engine: State<EngineManager>, cuelist_id: u32) -> Result<u32, String> {
    engine.create_playback(cuelist_id)
}

#[tauri::command]
pub fn engine_delete_playback(engine: State<EngineManager>, id: u32) {
    engine.delete_playback(id);
}

#[tauri::command]
pub fn engine_list_playbacks(engine: State<EngineManager>) -> Vec<PlaybackStatus> {
    engine.list_playbacks()
}

#[tauri::command]
pub fn engine_playback_go(engine: State<EngineManager>, id: u32) -> Result<(), String> {
    engine.playback_go(id)
}

#[tauri::command]
pub fn engine_playback_go_back(engine: State<EngineManager>, id: u32) -> Result<(), String> {
    engine.playback_go_back(id)
}

#[tauri::command]
pub fn engine_playback_release(engine: State<EngineManager>, id: u32) -> Result<(), String> {
    engine.playback_release(id)
}

#[tauri::command]
pub fn engine_playback_set_paused(engine: State<EngineManager>, id: u32, paused: bool) -> Result<(), String> {
    engine.playback_set_paused(id, paused)
}

#[tauri::command]
pub fn engine_playback_set_fader(engine: State<EngineManager>, id: u32, percent: f32) -> Result<(), String> {
    engine.playback_set_fader(id, percent)
}

#[tauri::command]
pub fn engine_playback_set_fader_mode(engine: State<EngineManager>, id: u32, mode: FaderMode) -> Result<(), String> {
    engine.playback_set_fader_mode(id, mode)
}

// --- Timecode cues (Phase 8) ---

#[tauri::command]
pub fn engine_set_cue_timecode(
    engine: State<EngineManager>,
    cuelist_id: u32,
    cue_number: f64,
    ms: Option<f64>,
) -> Result<(), String> {
    engine.set_cue_timecode(cuelist_id, cue_number, ms)
}

// --- Tempo (Phase 7) ---

#[tauri::command]
pub fn engine_tempo_status(engine: State<EngineManager>) -> TempoStatus {
    engine.tempo_status()
}

#[tauri::command]
pub fn engine_set_bpm(engine: State<EngineManager>, bpm: f32) {
    engine.set_bpm(bpm);
}

#[tauri::command]
pub fn engine_tap_tempo(engine: State<EngineManager>, now_ms: f64) -> f32 {
    engine.tap_tempo(now_ms)
}

// --- Effects (Phase 7) ---

#[tauri::command]
pub fn engine_start_effect(
    engine: State<EngineManager>,
    def: EffectDef,
    fixture_numbers: Vec<u32>,
    preset_id: Option<u32>,
) -> u32 {
    engine.start_effect(def, fixture_numbers, preset_id)
}

#[tauri::command]
pub fn engine_update_effect(engine: State<EngineManager>, id: u32, def: EffectDef) -> Result<(), String> {
    engine.update_effect(id, def)
}

#[tauri::command]
pub fn engine_stop_effect(engine: State<EngineManager>, id: u32) {
    engine.stop_effect(id);
}

#[tauri::command]
pub fn engine_list_effects(engine: State<EngineManager>) -> Vec<EffectInstance> {
    engine.list_effects()
}

#[tauri::command]
pub fn engine_record_effect_preset(engine: State<EngineManager>, name: String, def: EffectDef) -> u32 {
    engine.record_effect_preset(name, def)
}

#[tauri::command]
pub fn engine_update_effect_preset(engine: State<EngineManager>, id: u32, def: EffectDef) -> Result<(), String> {
    engine.update_effect_preset(id, def)
}

// --- Pixel maps (Phase 7) ---

#[tauri::command]
pub fn engine_create_pixel_map(engine: State<EngineManager>, name: String, width: u32, height: u32) -> u32 {
    engine.create_pixel_map(name, width, height)
}

#[tauri::command]
pub fn engine_update_pixel_map(engine: State<EngineManager>, map: PixelMap) -> Result<(), String> {
    engine.update_pixel_map(map)
}

#[tauri::command]
pub fn engine_delete_pixel_map(engine: State<EngineManager>, id: u32) {
    engine.delete_pixel_map(id);
}

#[tauri::command]
pub fn engine_list_pixel_maps(engine: State<EngineManager>) -> Vec<PixelMap> {
    engine.list_pixel_maps()
}

/// `data`: the image/GIF file, base64-encoded (far smaller over IPC than a
/// JSON array of bytes).
#[tauri::command]
pub fn engine_import_pixel_map_media(engine: State<EngineManager>, id: u32, data: String) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| format!("Bad file data: {e}"))?;
    engine.import_pixel_map_media(id, &bytes)
}

#[tauri::command]
pub fn engine_start_pixel_map(engine: State<EngineManager>, id: u32) -> Result<(), String> {
    engine.start_pixel_map(id)
}

#[tauri::command]
pub fn engine_stop_pixel_map(engine: State<EngineManager>, id: u32) {
    engine.stop_pixel_map(id);
}

#[tauri::command]
pub fn engine_set_pixel_map_intensity(engine: State<EngineManager>, id: u32, percent: f32) -> Result<(), String> {
    engine.set_pixel_map_intensity(id, percent)
}

#[tauri::command]
pub fn engine_running_pixel_maps(engine: State<EngineManager>) -> Vec<PixelMapStatus> {
    engine.running_pixel_maps()
}

#[tauri::command]
pub fn engine_pixel_map_preview(engine: State<EngineManager>, id: u32) -> Result<Vec<u8>, String> {
    engine.pixel_map_preview(id)
}
