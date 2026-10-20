//! Ties the engine's pieces together as Tauri-managed state: live merge
//! state, the fixture index layers resolve against, the global clocks, and
//! the 44Hz tick thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use super::audio::BandLevels;
use super::merge::merge_universe;
use super::state::{EngineState, Layer};
use super::tempo::{Tempo, TempoStatus};
use super::timecode::TimecodeClock;
use crate::output::manager::OutputManager;
use crate::show::resolve::ResolvedPatch;
use crate::output::UNIVERSE_SIZE;

const TICK_HZ: f64 = 44.0;
const DISPLAY_HZ: f64 = 20.0;
/// Channel rest values (the fixture library's `defaultValue`s) sit in their
/// own layer below everything else.
const DEFAULTS_LAYER: &str = "defaults";
const DEFAULTS_PRIORITY: i64 = i64::MIN;

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct EngineFrame {
    pub universe_id: u32,
    pub channels: Vec<u8>,
}

/// Everything one engine tick reads and writes — shared (by `Arc`) between
/// `EngineManager` and its tick thread, and steppable directly in tests.
struct TickShared {
    tempo: Arc<Mutex<Tempo>>,
    timecode: Arc<Mutex<TimecodeClock>>,
}

impl TickShared {
    /// Advances the clocks by `dt_ms`. Output is the caller's job: merge
    /// and send.
    fn advance(&self, dt_ms: f64) {
        self.tempo.lock().unwrap().advance(dt_ms);
        self.timecode.lock().unwrap().advance(dt_ms);
    }
}

pub struct EngineManager {
    state: Arc<Mutex<EngineState>>,
    /// The current patch, resolved to DMX addresses.
    patch: Arc<Mutex<Arc<ResolvedPatch>>>,
    tick_started: AtomicBool,
    tempo: Arc<Mutex<Tempo>>,
    /// Timing inputs: the `crate::timing` sources write into these.
    timecode: Arc<Mutex<TimecodeClock>>,
    audio: Arc<Mutex<BandLevels>>,
}

/// What `crate::timing` needs from the engine to feed it.
#[derive(Clone)]
pub struct TimingHandles {
    pub tempo: Arc<Mutex<Tempo>>,
    pub timecode: Arc<Mutex<TimecodeClock>>,
    pub audio: Arc<Mutex<BandLevels>>,
}

impl Default for EngineManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(EngineState::new())),
            patch: Arc::new(Mutex::new(Arc::new(ResolvedPatch::default()))),
            tick_started: AtomicBool::new(false),
            tempo: Arc::new(Mutex::new(Tempo::new())),
            timecode: Arc::new(Mutex::new(TimecodeClock::new())),
            audio: Arc::new(Mutex::new(BandLevels::default())),
        }
    }

    fn tick_shared(&self) -> TickShared {
        TickShared { tempo: self.tempo.clone(), timecode: self.timecode.clone() }
    }

    pub fn timing_handles(&self) -> TimingHandles {
        TimingHandles { tempo: self.tempo.clone(), timecode: self.timecode.clone(), audio: self.audio.clone() }
    }

    /// Starts the 44Hz tick thread (idempotent — safe to call from `setup`
    /// every launch).
    pub fn start_tick(&self, app: AppHandle) {
        if self.tick_started.swap(true, Ordering::SeqCst) {
            return;
        }

        let shared = self.tick_shared();
        let state = self.state.clone();

        thread::spawn(move || {
            let tick_interval = Duration::from_secs_f64(1.0 / TICK_HZ);
            let display_interval = Duration::from_secs_f64(1.0 / DISPLAY_HZ);
            let mut last_tick = Instant::now();
            let mut last_display = Instant::now();

            loop {
                let now = Instant::now();
                let dt_ms = now.duration_since(last_tick).as_secs_f64() * 1000.0;
                last_tick = now;

                shared.advance(dt_ms);

                let output = app.state::<OutputManager>();
                let universe_ids = output.universe_ids();
                let show_display = now.duration_since(last_display) >= display_interval;

                for universe_id in universe_ids {
                    let frame = {
                        let state = state.lock().unwrap();
                        merge_universe(&state, universe_id)
                    };
                    let _ = output.set_universe_data(universe_id, frame);

                    if show_display {
                        let _ = app.emit(
                            "engine://universe-frame",
                            EngineFrame { universe_id, channels: frame.to_vec() },
                        );
                    }
                }
                if show_display {
                    last_display = now;
                }

                let elapsed = now.elapsed();
                if elapsed < tick_interval {
                    thread::sleep(tick_interval - elapsed);
                }
            }
        });
    }

    /// Takes a newly resolved patch: which channels merge HTP, and the
    /// layer holding every channel's rest value.
    pub fn set_patch(&self, patch: Arc<ResolvedPatch>) {
        {
            let mut state = self.state.lock().unwrap();
            state.htp_channels = patch.htp.clone();
            let defaults = Layer { values: patch.defaults.clone(), ..Layer::default() };
            state.layers.insert(DEFAULTS_LAYER.to_string(), (DEFAULTS_PRIORITY, defaults));
        }
        *self.patch.lock().unwrap() = patch;
    }

    pub fn get_universe_data(&self, universe: u32) -> [u8; UNIVERSE_SIZE] {
        let state = self.state.lock().unwrap();
        merge_universe(&state, universe)
    }

    pub fn set_grand_master(&self, percent: f32) {
        self.state.lock().unwrap().grand_master = percent.clamp(0.0, 100.0) / 100.0;
    }

    pub fn grand_master(&self) -> f32 {
        self.state.lock().unwrap().grand_master * 100.0
    }

    // --- Tempo ---

    pub fn tempo_status(&self) -> TempoStatus {
        self.tempo.lock().unwrap().status()
    }

    pub fn bpm(&self) -> f32 {
        self.tempo.lock().unwrap().bpm()
    }

    pub fn set_bpm(&self, bpm: f32) {
        self.tempo.lock().unwrap().set_bpm(bpm);
    }

    /// `now_ms`: the caller's timestamp for the tap (the frontend's
    /// `performance.now()`), so network/IPC jitter doesn't skew the
    /// measured interval.
    pub fn tap_tempo(&self, now_ms: f64) -> f32 {
        self.tempo.lock().unwrap().tap(now_ms)
    }
}
