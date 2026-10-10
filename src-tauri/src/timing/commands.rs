//! Tauri commands for the timing sources.

use tauri::State;

use super::audio_input::AudioDeviceInfo;
use super::manager::{list_audio_inputs, TimingManager, TimingSettings, TimingStatus};
use crate::engine::audio_reactive::AudioTrigger;
use crate::io::midi::{self, MidiPortInfo};

#[tauri::command]
pub fn timing_status(timing: State<TimingManager>) -> TimingStatus {
    timing.status()
}

#[tauri::command]
pub fn timing_configure(timing: State<TimingManager>, settings: TimingSettings) -> TimingStatus {
    timing.configure(settings);
    timing.status()
}

#[tauri::command]
pub fn timing_list_audio_inputs() -> Vec<AudioDeviceInfo> {
    list_audio_inputs()
}

#[tauri::command]
pub fn timing_list_midi_inputs() -> Vec<MidiPortInfo> {
    midi::list_input_ports()
}

#[tauri::command]
pub fn timing_list_triggers(timing: State<TimingManager>) -> Vec<AudioTrigger> {
    timing.list_triggers()
}

#[tauri::command]
pub fn timing_upsert_trigger(timing: State<TimingManager>, trigger: AudioTrigger) -> u32 {
    timing.upsert_trigger(trigger)
}

#[tauri::command]
pub fn timing_delete_trigger(timing: State<TimingManager>, id: u32) {
    timing.delete_trigger(id);
}

#[tauri::command]
pub fn timing_timecode_play(timing: State<TimingManager>) {
    timing.timecode_play();
}

#[tauri::command]
pub fn timing_timecode_pause(timing: State<TimingManager>) {
    timing.timecode_pause();
}

#[tauri::command]
pub fn timing_timecode_stop(timing: State<TimingManager>) {
    timing.timecode_stop();
}

#[tauri::command]
pub fn timing_timecode_seek(timing: State<TimingManager>, ms: f64) {
    timing.timecode_seek(ms);
}

#[tauri::command]
pub fn timing_set_timecode_learn(timing: State<TimingManager>, on: bool) {
    timing.set_timecode_learn(on);
}
