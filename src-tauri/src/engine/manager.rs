//! Ties the engine's pieces together as Tauri-managed state: live merge
//! state, the fixture index layers resolve against, the global clocks, and
//! the 44Hz tick thread.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use super::audio::BandLevels;
use super::groups::{Group, GroupStore};
use super::merge::merge_universe;
use super::state::{EngineState, Layer};
use super::tempo::{Tempo, TempoStatus};
use super::timecode::TimecodeClock;
use crate::output::manager::OutputManager;
use crate::output::UNIVERSE_SIZE;

const TICK_HZ: f64 = 44.0;
const DISPLAY_HZ: f64 = 20.0;
/// The layer direct channel sets (the Fixtures panel's sliders) land in.
const MANUAL_LAYER: &str = "manual";
const MANUAL_PRIORITY: i64 = i64::MAX;

/// A patched fixture resolved to DMX addresses: which channel each of its
/// attributes lives on. Pushed from the frontend whenever the patch changes
/// (see docs/ENGINE.md).
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct PatchIndexEntry {
    pub fixture_number: u32,
    pub universe: u32,
    /// The fixture's (first) dimmer channel, if it has one.
    pub intensity_channel: Option<u16>,
    /// Every attribute name (e.g. "red", "pan", "gobo") this fixture has,
    /// mapped to its channel.
    #[serde(default)]
    pub attribute_channels: HashMap<String, u16>,
    /// For a multi-cell fixture (an LED bar, a pixel batten): each cell's
    /// attribute → channel map, in cell order (cell 1 first). Empty for a
    /// single-cell fixture.
    #[serde(default)]
    pub cells: Vec<HashMap<String, u16>>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct EngineFrame {
    pub universe_id: u32,
    pub channels: Vec<u8>,
}

/// The engine-owned part of a show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct ShowEngineData {
    pub groups: Vec<Group>,
    /// The global tempo.
    pub bpm: f32,
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
    patch_index: Arc<Mutex<Vec<PatchIndexEntry>>>,
    groups: Mutex<GroupStore>,
    tick_started: AtomicBool,
    tempo: Arc<Mutex<Tempo>>,
    /// Bumped by every change to engine-owned show data, so the frontend
    /// can tell the show has unsaved engine-side edits.
    data_revision: AtomicU64,
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
            patch_index: Arc::new(Mutex::new(Vec::new())),
            groups: Mutex::new(GroupStore::new()),
            tick_started: AtomicBool::new(false),
            tempo: Arc::new(Mutex::new(Tempo::new())),
            data_revision: AtomicU64::new(0),
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

    pub fn set_htp_channels(&self, universe: u32, channels: Vec<u16>) {
        let mut state = self.state.lock().unwrap();
        state.htp_channels.insert(universe, channels.into_iter().collect());
    }

    pub fn set_patch_index(&self, entries: Vec<PatchIndexEntry>) {
        *self.patch_index.lock().unwrap() = entries;
    }

    /// Immediate set into the manual layer (the Fixtures panel's sliders).
    pub fn set_channels(&self, universe: u32, values: &HashMap<u16, u8>) {
        let mut state = self.state.lock().unwrap();
        let (_, layer) = state
            .layers
            .entry(MANUAL_LAYER.to_string())
            .or_insert_with(|| (MANUAL_PRIORITY, Layer::default()));
        for (&channel, &value) in values {
            layer.values.insert((universe, channel), value as f32);
        }
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

    // --- Groups ---

    pub fn record_group(&self, name: String, fixture_numbers: Vec<u32>) -> u32 {
        self.mark_data_changed();
        self.groups.lock().unwrap().record(name, fixture_numbers)
    }

    pub fn list_groups(&self) -> Vec<Group> {
        self.groups.lock().unwrap().list()
    }

    pub fn rename_group(&self, id: u32, name: String) -> Result<(), String> {
        self.mark_data_changed();
        self.groups.lock().unwrap().rename(id, name)
    }

    pub fn delete_group(&self, id: u32) {
        self.mark_data_changed();
        self.groups.lock().unwrap().delete(id);
    }

    // --- Show data ---

    fn mark_data_changed(&self) {
        self.data_revision.fetch_add(1, Ordering::SeqCst);
    }

    /// Changes whenever engine-owned show data changes; compare against the
    /// value at the last save/load to know whether there's unsaved work.
    pub fn show_data_revision(&self) -> u64 {
        self.data_revision.load(Ordering::SeqCst)
    }

    pub fn export_show_data(&self) -> ShowEngineData {
        ShowEngineData { groups: self.list_groups(), bpm: self.tempo.lock().unwrap().bpm() }
    }

    /// Replaces the engine-owned data with a show's (open/new/restore).
    pub fn load_show_data(&self, data: ShowEngineData) {
        self.tempo.lock().unwrap().set_bpm(data.bpm);
        self.groups.lock().unwrap().replace_all(data.groups);
        self.state.lock().unwrap().layers.clear();
        self.mark_data_changed();
    }

    // --- Tempo ---

    pub fn tempo_status(&self) -> TempoStatus {
        self.tempo.lock().unwrap().status()
    }

    pub fn set_bpm(&self, bpm: f32) {
        self.mark_data_changed();
        self.tempo.lock().unwrap().set_bpm(bpm);
    }

    /// `now_ms`: the caller's timestamp for the tap (the frontend's
    /// `performance.now()`), so network/IPC jitter doesn't skew the
    /// measured interval.
    pub fn tap_tempo(&self, now_ms: f64) -> f32 {
        self.mark_data_changed();
        self.tempo.lock().unwrap().tap(now_ms)
    }
}
