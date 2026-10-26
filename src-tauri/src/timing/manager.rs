//! Owns the live timing sources: which one drives the tempo and timecode,
//! the audio / MIDI inputs they need, and the status the UI polls. All
//! settings are applied through `configure`, which starts and stops inputs
//! to match, and saved per machine (`timing.json` in the app data folder) so
//! the chosen devices come back on the next launch.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::analysis::Analyzer;
use super::audio_input::{self, AudioCapture};
use super::ltc::LtcDecoder;
use super::mixxx::MixxxDecoder;
use super::mtc::{ClockEvent, MidiClockTracker, MtcDecoder};
use crate::engine::audio::BandLevels;
use crate::engine::manager::TimingHandles;
use crate::engine::timecode::{parts_to_ms, TimecodeSource, TimecodeStatus};
use crate::io::midi::{self, MidiInputHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum TempoSource {
    /// Nothing drives the tempo; type the BPM.
    Manual,
    /// Nothing drives the tempo; tap it in.
    Tap,
    /// Beat detection on the audio input.
    Audio,
    /// MIDI clock from the clock port.
    MidiClock,
    /// BPM and beats from the Mixxx script (see `timing::mixxx`), over the
    /// clock port.
    Mixxx,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct TimingSettings {
    pub tempo_source: TempoSource,
    pub timecode_source: TimecodeSource,
    /// Run audio analysis (band levels for the graph's Audio node) even
    /// when the tempo source isn't Audio.
    pub audio_analysis: bool,
    /// `None`: the system default input.
    pub audio_device: Option<String>,
    pub audio_channel: u32,
    pub ltc_device: Option<String>,
    pub ltc_channel: u32,
    /// The MIDI port MIDI clock, MTC and Mixxx are read from.
    pub clock_port: Option<String>,
    /// The MIDI ports the graph's MIDI CC / Note nodes read, all merged.
    /// May include the clock port.
    pub controller_ports: Vec<String>,
}

impl Default for TimingSettings {
    fn default() -> Self {
        Self {
            tempo_source: TempoSource::Manual,
            timecode_source: TimecodeSource::None,
            audio_analysis: false,
            audio_device: None,
            audio_channel: 0,
            ltc_device: None,
            ltc_channel: 0,
            clock_port: None,
            controller_ports: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct AudioInputStatus {
    pub running: bool,
    /// The detector's current tempo, whether or not it drives the show.
    pub bpm: Option<f32>,
    pub locked: bool,
    pub confidence: f32,
    pub levels: BandLevels,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct MidiClockStatus {
    pub connected: bool,
    pub clock_bpm: Option<f32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct MidiPortStatus {
    pub port: String,
    pub connected: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct TimingStatus {
    pub settings: TimingSettings,
    pub audio: AudioInputStatus,
    pub midi_clock: MidiClockStatus,
    /// One per controller port, in `controller_ports` order.
    pub midi_controllers: Vec<MidiPortStatus>,
    pub ltc_running: bool,
    pub ltc_error: Option<String>,
    pub timecode: TimecodeStatus,
}

/// State the input callbacks and `status` share.
#[derive(Default)]
struct Shared {
    tempo_source: Option<TempoSource>,
    audio: AudioInputStatus,
    midi_clock: MidiClockStatus,
    /// Why each MIDI port that failed to open didn't.
    midi_errors: BTreeMap<String, String>,
    ltc_error: Option<String>,
}

/// What a MIDI port is open for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MidiRoles {
    clock: bool,
    controllers: bool,
}

struct Inputs {
    settings: TimingSettings,
    audio: Option<AudioCapture>,
    ltc: Option<AudioCapture>,
    /// Open MIDI ports by id. Each port is opened once, whatever it's for.
    midi: HashMap<String, (MidiRoles, MidiInputHandle)>,
}

pub struct TimingManager {
    handles: TimingHandles,
    /// Where the settings are saved; `None` when there's no data folder.
    settings_path: Option<PathBuf>,
    shared: Arc<Mutex<Shared>>,
    inputs: Mutex<Inputs>,
}

impl TimingManager {
    pub fn new(handles: TimingHandles, data_dir: Option<PathBuf>) -> Self {
        Self {
            handles,
            settings_path: data_dir.map(|d| d.join("timing.json")),
            shared: Arc::new(Mutex::new(Shared { tempo_source: Some(TempoSource::Manual), ..Default::default() })),
            inputs: Mutex::new(Inputs { settings: TimingSettings::default(), audio: None, ltc: None, midi: HashMap::new() }),
        }
    }

    /// The settings saved by the last launch, if any (and readable).
    pub fn saved_settings(&self) -> Option<TimingSettings> {
        let text = fs::read_to_string(self.settings_path.as_ref()?).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn save(&self, settings: &TimingSettings) {
        let Some(path) = &self.settings_path else { return };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(settings) {
            let _ = fs::write(path, text);
        }
    }

    /// Applies `settings`, starting and stopping inputs as needed, and saves
    /// them. An input that fails to open (a device that's unplugged) is
    /// reported in the status, not as an error here; it's tried again on the
    /// next `configure`.
    pub fn configure(&self, settings: TimingSettings) {
        self.save(&settings);
        let mut inputs = self.inputs.lock().unwrap();
        let old = inputs.settings.clone();
        self.shared.lock().unwrap().tempo_source = Some(settings.tempo_source);
        self.handles.timecode.lock().unwrap().set_source(settings.timecode_source);

        let want_audio = settings.audio_analysis || settings.tempo_source == TempoSource::Audio;
        let audio_changed =
            old.audio_device != settings.audio_device || old.audio_channel != settings.audio_channel;
        if !want_audio || audio_changed {
            inputs.audio = None;
            self.stop_audio_status();
        }
        if want_audio && inputs.audio.is_none() {
            match self.start_audio(&settings) {
                Ok(capture) => {
                    inputs.audio = Some(capture);
                    self.shared.lock().unwrap().audio = AudioInputStatus { running: true, ..Default::default() };
                }
                Err(e) => self.shared.lock().unwrap().audio.error = Some(e),
            }
        }

        let want_ltc = settings.timecode_source == TimecodeSource::Ltc;
        if !want_ltc || old.ltc_device != settings.ltc_device || old.ltc_channel != settings.ltc_channel {
            inputs.ltc = None;
            self.shared.lock().unwrap().ltc_error = None;
        }
        if want_ltc && inputs.ltc.is_none() {
            match self.start_ltc(&settings) {
                Ok(capture) => inputs.ltc = Some(capture),
                Err(e) => self.shared.lock().unwrap().ltc_error = Some(e),
            }
        }

        // Each chosen port is open whenever it's chosen; one that changes
        // role is reopened.
        let mut wanted: BTreeMap<&str, MidiRoles> = BTreeMap::new();
        if let Some(port) = &settings.clock_port {
            wanted.entry(port).or_insert(MidiRoles { clock: false, controllers: false }).clock = true;
        }
        for port in &settings.controller_ports {
            wanted.entry(port).or_insert(MidiRoles { clock: false, controllers: false }).controllers = true;
        }
        inputs.midi.retain(|port, (roles, _)| wanted.get(port.as_str()) == Some(roles));
        if old.clock_port != settings.clock_port {
            self.shared.lock().unwrap().midi_clock = MidiClockStatus::default();
        }
        let mut errors = BTreeMap::new();
        for (port, roles) in wanted {
            if inputs.midi.contains_key(port) {
                continue;
            }
            match self.start_midi(port, roles) {
                Ok(handle) => {
                    inputs.midi.insert(port.to_string(), (roles, handle));
                }
                Err(e) => {
                    errors.insert(port.to_string(), e);
                }
            }
        }
        {
            let mut shared = self.shared.lock().unwrap();
            // Every wanted port that isn't open was just tried again.
            shared.midi_errors = errors;
            let clock_open = settings.clock_port.as_ref().is_some_and(|p| inputs.midi.contains_key(p));
            shared.midi_clock.connected = clock_open;
            shared.midi_clock.error = settings.clock_port.as_ref().and_then(|p| shared.midi_errors.get(p).cloned());
        }

        inputs.settings = settings;
    }

    fn stop_audio_status(&self) {
        self.shared.lock().unwrap().audio = AudioInputStatus::default();
        *self.handles.audio.lock().unwrap() = BandLevels::default();
    }

    fn start_audio(&self, settings: &TimingSettings) -> Result<AudioCapture, String> {
        let shared = self.shared.clone();
        let tempo = self.handles.tempo.clone();
        let reactive = self.handles.audio.clone();
        let mut analyzer: Option<Analyzer> = None;
        audio_input::start_capture(settings.audio_device.clone(), settings.audio_channel as usize, move |block, rate| {
            let analyzer = analyzer.get_or_insert_with(|| Analyzer::new(rate));
            analyzer.push(block);
            let levels = analyzer.levels();
            let estimate = analyzer.tempo();
            *reactive.lock().unwrap() = levels;
            let mut shared = shared.lock().unwrap();
            shared.audio.levels = levels;
            shared.audio.bpm = estimate.bpm;
            shared.audio.locked = estimate.locked;
            shared.audio.confidence = estimate.confidence;
            if shared.tempo_source == Some(TempoSource::Audio) && estimate.locked {
                if let Some(bpm) = estimate.bpm {
                    let mut tempo = tempo.lock().unwrap();
                    // Re-setting an unchanged BPM is harmless, but skip
                    // sub-0.05 jitter so the display doesn't flicker.
                    if (tempo.bpm() - bpm).abs() >= 0.05 {
                        tempo.set_bpm((bpm * 10.0).round() / 10.0);
                    }
                }
            }
        })
    }

    fn start_ltc(&self, settings: &TimingSettings) -> Result<AudioCapture, String> {
        let clock = self.handles.timecode.clone();
        let mut decoder: Option<LtcDecoder> = None;
        audio_input::start_capture(settings.ltc_device.clone(), settings.ltc_channel as usize, move |block, rate| {
            let decoder = decoder.get_or_insert_with(|| LtcDecoder::new(rate));
            decoder.push(block, |f| {
                // The frame has just finished: now is one frame after its label.
                let ms = parts_to_ms(f.hours, f.minutes, f.seconds, f.frames, f.fps) + 1000.0 / f.fps as f64;
                clock.lock().unwrap().set_external(TimecodeSource::Ltc, ms, f.fps);
            });
        })
    }

    fn start_midi(&self, port: &str, roles: MidiRoles) -> Result<MidiInputHandle, String> {
        let shared = self.shared.clone();
        let tempo = self.handles.tempo.clone();
        let clock = self.handles.timecode.clone();
        let control = self.handles.midi.clone();
        let mut mtc = MtcDecoder::new();
        let mut midi_clock = MidiClockTracker::new();
        let mut mixxx = MixxxDecoder::new();
        let port_id = port.to_string();
        midi::open_input(port, move |stamp_us, msg| {
            if roles.controllers {
                control.lock().unwrap().feed(&port_id, msg);
            }
            if !roles.clock {
                return;
            }
            if let Some(pos) = mtc.feed(msg) {
                clock.lock().unwrap().set_external(TimecodeSource::Mtc, pos.ms, pos.fps);
            }
            let source = shared.lock().unwrap().tempo_source;
            let event = match source {
                Some(TempoSource::Mixxx) => mixxx.feed(msg),
                _ => midi_clock.feed(stamp_us, msg),
            };
            let driving = matches!(source, Some(TempoSource::MidiClock | TempoSource::Mixxx));
            match event {
                Some(ClockEvent::Bpm(bpm)) => {
                    shared.lock().unwrap().midi_clock.clock_bpm = Some(bpm);
                    if driving {
                        tempo.lock().unwrap().set_bpm((bpm * 10.0).round() / 10.0);
                    }
                }
                Some(ClockEvent::Start) if driving => tempo.lock().unwrap().align_beat(),
                _ => {}
            }
        })
    }

    pub fn status(&self) -> TimingStatus {
        // Read `inputs` once, up front: a guard taken inside the struct
        // literal lives until the function returns, so locking it twice there
        // deadlocks the calling (main) thread.
        let (settings, ltc_running) = {
            let inputs = self.inputs.lock().unwrap();
            (inputs.settings.clone(), inputs.ltc.is_some())
        };
        let shared = self.shared.lock().unwrap();
        let midi_controllers = settings
            .controller_ports
            .iter()
            .map(|port| {
                let error = shared.midi_errors.get(port).cloned();
                MidiPortStatus { port: port.clone(), connected: error.is_none(), error }
            })
            .collect();
        TimingStatus {
            settings,
            audio: shared.audio.clone(),
            midi_clock: shared.midi_clock.clone(),
            midi_controllers,
            ltc_running,
            ltc_error: shared.ltc_error.clone(),
            timecode: self.handles.timecode.lock().unwrap().status(),
        }
    }
}

pub fn list_audio_inputs() -> Vec<audio_input::AudioDeviceInfo> {
    audio_input::list_input_devices()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::manager::EngineManager;
    use std::sync::mpsc;
    use std::time::Duration;

    /// `status()` once locked `inputs` twice in one expression and hung the
    /// UI thread on the frontend's first `timing_status` poll.
    #[test]
    fn status_does_not_deadlock() {
        let timing = TimingManager::new(EngineManager::new().timing_handles(), None);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            tx.send(timing.status().ltc_running).unwrap();
        });
        let ltc_running = rx.recv_timeout(Duration::from_secs(2)).expect("status() deadlocked");
        assert!(!ltc_running);
    }

    #[test]
    fn settings_are_saved_and_read_back_next_launch() {
        let dir = std::env::temp_dir().join(format!("lumendmx-timing-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let first = TimingManager::new(EngineManager::new().timing_handles(), Some(dir.clone()));
        assert!(first.saved_settings().is_none());
        // Ports that don't exist: they fail to open, but are still saved.
        let settings = TimingSettings {
            tempo_source: TempoSource::Tap,
            controller_ports: vec!["no-such-port-1".into(), "no-such-port-2".into()],
            ..TimingSettings::default()
        };
        first.configure(settings.clone());

        let next = TimingManager::new(EngineManager::new().timing_handles(), Some(dir.clone()));
        assert_eq!(next.saved_settings(), Some(settings));
        let _ = fs::remove_dir_all(&dir);
    }
}
