//! Tauri commands for the timing sources.

use tauri::State;

use super::audio_input::AudioDeviceInfo;
use super::manager::{list_audio_inputs, TimingManager, TimingSettings, TimingStatus};
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

/// Async so the slow first CoreAudio enumeration doesn't block the UI thread.
#[tauri::command]
pub async fn timing_list_audio_inputs() -> Vec<AudioDeviceInfo> {
    list_audio_inputs()
}

/// Async: creating the first CoreMIDI client is slow, like audio above.
#[tauri::command]
pub async fn timing_list_midi_inputs() -> Vec<MidiPortInfo> {
    midi::list_input_ports()
}
