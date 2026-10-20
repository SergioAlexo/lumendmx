use std::collections::HashMap;

use tauri::State;

use super::groups::Group;
use super::manager::{EngineManager, PatchIndexEntry, ShowEngineData};
use super::tempo::TempoStatus;

#[tauri::command]
pub fn engine_set_htp_channels(engine: State<EngineManager>, universe: u32, channels: Vec<u16>) {
    engine.set_htp_channels(universe, channels);
}

#[tauri::command]
pub fn engine_set_patch_index(engine: State<EngineManager>, entries: Vec<PatchIndexEntry>) {
    engine.set_patch_index(entries);
}

/// Immediate set into the manual layer (the Fixtures panel's sliders).
#[tauri::command]
pub fn engine_set_channels(engine: State<EngineManager>, universe: u32, values: HashMap<u16, u8>) {
    engine.set_channels(universe, &values);
}

#[tauri::command]
pub fn engine_get_universe_data(engine: State<EngineManager>, universe: u32) -> Vec<u8> {
    engine.get_universe_data(universe).to_vec()
}

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

#[tauri::command]
pub fn engine_set_grand_master(engine: State<EngineManager>, percent: f32) {
    engine.set_grand_master(percent);
}

#[tauri::command]
pub fn engine_grand_master(engine: State<EngineManager>) -> f32 {
    engine.grand_master()
}

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
