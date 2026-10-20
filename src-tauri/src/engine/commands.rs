use tauri::State;

use super::manager::EngineManager;
use super::tempo::TempoStatus;

#[tauri::command]
pub fn engine_get_universe_data(engine: State<EngineManager>, universe: u32) -> Vec<u8> {
    engine.get_universe_data(universe).to_vec()
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
