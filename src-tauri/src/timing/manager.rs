//! Owns the live timing sources: which one drives the tempo and timecode,
//! the audio / MIDI inputs they need, and the status the UI polls. All
//! settings are applied through `configure`, which starts and stops inputs
//! to match.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::analysis::Analyzer;
use super::audio_input::{self, AudioCapture};
use super::ltc::LtcDecoder;
use super::mtc::{ClockEvent, MidiClockTracker, MtcDecoder};
use crate::engine::audio_reactive::{AudioTrigger, BandLevels};
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
    /// MIDI clock from the selected MIDI port.
    MidiClock,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct TimingSettings {
    pub tempo_source: TempoSource,
    pub timecode_source: TimecodeSource,
    /// Run audio analysis (band levels for triggers, beat detection) even
    /// when the tempo source isn't Audio.
    pub audio_analysis: bool,
    /// `None`: the system default input.
    pub audio_device: Option<String>,
    pub audio_channel: u32,
    pub ltc_device: Option<String>,
    pub ltc_channel: u32,
    /// The MIDI port MIDI clock and MTC are read from.
    pub midi_port: Option<String>,
    /// Chases run at the show tempo instead of their own BPM.
    pub chases_follow_tempo: bool,
}

impl Default for TimingSettings {
    fn default() -> Self {
        Self {
            tempo_source: TempoSource::Manual,
            timecode_source: TimecodeSource::Internal,
            audio_analysis: false,
            audio_device: None,
            audio_channel: 0,
            ltc_device: None,
            ltc_channel: 0,
            midi_port: None,
            chases_follow_tempo: false,
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
pub struct MidiInputStatus {
    pub connected: bool,
    pub clock_bpm: Option<f32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct TimingStatus {
    pub settings: TimingSettings,
    pub audio: AudioInputStatus,
    pub midi: MidiInputStatus,
    pub ltc_running: bool,
    pub ltc_error: Option<String>,
    pub timecode: TimecodeStatus,
    pub timecode_learn: bool,
}

/// State the input callbacks and `status` share.
#[derive(Default)]
struct Shared {
    tempo_source: Option<TempoSource>,
    audio: AudioInputStatus,
    midi: MidiInputStatus,
    ltc_error: Option<String>,
}

struct Inputs {
    settings: TimingSettings,
    audio: Option<AudioCapture>,
    ltc: Option<AudioCapture>,
    midi: Option<MidiInputHandle>,
}

pub struct TimingManager {
    handles: TimingHandles,
    shared: Arc<Mutex<Shared>>,
    inputs: Mutex<Inputs>,
}

impl TimingManager {
    pub fn new(handles: TimingHandles) -> Self {
        Self {
            handles,
            shared: Arc::new(Mutex::new(Shared { tempo_source: Some(TempoSource::Manual), ..Default::default() })),
            inputs: Mutex::new(Inputs { settings: TimingSettings::default(), audio: None, ltc: None, midi: None }),
        }
    }

    /// Applies `settings`, starting and stopping inputs as needed. An input
    /// that fails to open is reported in the status, not as an error here.
    pub fn configure(&self, settings: TimingSettings) {
        let mut inputs = self.inputs.lock().unwrap();
        let old = inputs.settings.clone();
        self.handles.chases_follow_tempo.store(settings.chases_follow_tempo, Ordering::Relaxed);
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

        let want_midi = settings.tempo_source == TempoSource::MidiClock || settings.timecode_source == TimecodeSource::Mtc;
        if !want_midi || old.midi_port != settings.midi_port {
            inputs.midi = None;
            self.shared.lock().unwrap().midi = MidiInputStatus::default();
        }
        if want_midi && inputs.midi.is_none() {
            match &settings.midi_port {
                None => self.shared.lock().unwrap().midi.error = Some("Choose a MIDI port".to_string()),
                Some(port) => match self.start_midi(port) {
                    Ok(handle) => {
                        inputs.midi = Some(handle);
                        self.shared.lock().unwrap().midi = MidiInputStatus { connected: true, ..Default::default() };
                    }
                    Err(e) => self.shared.lock().unwrap().midi.error = Some(e),
                },
            }
        }

        inputs.settings = settings;
    }

    fn stop_audio_status(&self) {
        self.shared.lock().unwrap().audio = AudioInputStatus::default();
        self.handles.audio.lock().unwrap().levels = BandLevels::default();
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
            reactive.lock().unwrap().levels = levels;
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

    fn start_midi(&self, port: &str) -> Result<MidiInputHandle, String> {
        let shared = self.shared.clone();
        let tempo = self.handles.tempo.clone();
        let clock = self.handles.timecode.clone();
        let mut mtc = MtcDecoder::new();
        let mut midi_clock = MidiClockTracker::new();
        midi::open_input(port, move |stamp_us, msg| {
            if let Some(pos) = mtc.feed(msg) {
                clock.lock().unwrap().set_external(TimecodeSource::Mtc, pos.ms, pos.fps);
            }
            let driving = shared.lock().unwrap().tempo_source == Some(TempoSource::MidiClock);
            match midi_clock.feed(stamp_us, msg) {
                Some(ClockEvent::Bpm(bpm)) => {
                    shared.lock().unwrap().midi.clock_bpm = Some(bpm);
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
        let shared = self.shared.lock().unwrap();
        TimingStatus {
            settings: self.inputs.lock().unwrap().settings.clone(),
            audio: shared.audio.clone(),
            midi: shared.midi.clone(),
            ltc_running: self.inputs.lock().unwrap().ltc.is_some(),
            ltc_error: shared.ltc_error.clone(),
            timecode: self.handles.timecode.lock().unwrap().status(),
            timecode_learn: self.handles.timecode_learn.load(Ordering::Relaxed),
        }
    }

    // --- Audio-reactive triggers ---

    pub fn list_triggers(&self) -> Vec<AudioTrigger> {
        self.handles.audio.lock().unwrap().triggers().to_vec()
    }

    pub fn upsert_trigger(&self, trigger: AudioTrigger) -> u32 {
        self.handles.audio.lock().unwrap().upsert(trigger)
    }

    pub fn delete_trigger(&self, id: u32) {
        self.handles.audio.lock().unwrap().delete(id);
    }

    // --- Timecode transport (internal clock) ---

    pub fn set_timecode_learn(&self, on: bool) {
        self.handles.timecode_learn.store(on, Ordering::Relaxed);
    }

    pub fn timecode_play(&self) {
        self.handles.timecode.lock().unwrap().play();
    }

    pub fn timecode_pause(&self) {
        self.handles.timecode.lock().unwrap().pause();
    }

    pub fn timecode_stop(&self) {
        self.handles.timecode.lock().unwrap().stop();
    }

    pub fn timecode_seek(&self, ms: f64) {
        self.handles.timecode.lock().unwrap().seek(ms);
    }
}

pub fn list_audio_inputs() -> Vec<audio_input::AudioDeviceInfo> {
    audio_input::list_input_devices()
}
