//! Ties the engine's pieces together as Tauri-managed state: live merge
//! state, running legacy-asset playbacks, the fixture-number index the
//! command line resolves against, and the 44Hz tick thread.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use super::command_line::{self, Command};
use super::cues::{resolve_cue_value, Cue, CueStore, CueValue, Cuelist, CuelistKind};
use super::effects::{self, EffectClock, EffectDef, EffectInstance, ResolvedEffect};
use super::groups::{Group, GroupStore};
use super::legacy_playback::{LegacyAsset, RunningPlayback};
use super::merge::{channel_sources, merge_universe, merge_universe_static, TrackedChannels};
use super::pixelmap::{self, PixelMap, PixelMapRuntime, PixelMapStore};
use super::playback::{FaderMode, PlaybackRuntime};
use super::presets::{Preset, PresetFamily, PresetStore};
use super::state::{ChannelKey, EngineState, Layer, LayerEffect};
use super::tempo::{Tempo, TempoStatus};
use crate::output::manager::OutputManager;
use crate::output::UNIVERSE_SIZE;

const TICK_HZ: f64 = 44.0;
const DISPLAY_HZ: f64 = 20.0;
/// A cuelist marked `Override` gets a priority at least this high (below the
/// programmer's `i64::MAX`), so it wins LTP over any standard/chase playback
/// or legacy-asset layer regardless of trigger order.
pub const OVERRIDE_PRIORITY_BASE: i64 = 1_000_000_000;
/// Default fade time for `playback_release` when the caller doesn't specify one.
const DEFAULT_RELEASE_FADE_MS: f64 = 500.0;

/// What the command line's `1 THRU 8` etc. resolve fixture numbers against,
/// and which channel `@ value` should drive for each. Pushed from the
/// frontend whenever the patch changes (see docs/ENGINE.md) — fixture/channel
/// resolution is static config data, not timing-critical, so it stays where
/// the fixture library already lives (TypeScript) rather than being
/// duplicated in Rust.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct PatchIndexEntry {
    pub fixture_number: u32,
    pub universe: u32,
    /// The channel `@ value` should drive, if this fixture has a dimmer
    /// channel at all.
    pub intensity_channel: Option<u16>,
    /// Every other attribute name (e.g. "red", "pan", "gobo") this fixture
    /// has, mapped to its channel — what a preset's `apply` resolves
    /// attribute values against for an arbitrary target fixture (Phase 5).
    #[serde(default)]
    pub attribute_channels: HashMap<String, u16>,
    /// For a multi-cell fixture (an LED bar, a pixel batten): each cell's
    /// attribute → channel map, in cell order (cell 1 first). Empty for a
    /// single-cell fixture. What a pixel map's per-cell mapping resolves
    /// against (Phase 7).
    #[serde(default)]
    pub cells: Vec<HashMap<String, u16>>,
}

#[derive(Default)]
struct ProgrammerContext {
    selection: HashSet<u32>,
    /// CLEAR's first press clears the selection; a second press (selection
    /// already empty) clears the programmer's values instead.
    pending_time_s: f32,
    /// The cuelist `RECORD CUE`/`UPDATE`/`DELETE CUE`/`NEXT`/`PREV` operate
    /// against — set by `set_current_cuelist` when the Cuelists tab selects
    /// one (Phase 6). `None` until then.
    current_cuelist: Option<u32>,
    /// The last cue number `RECORD CUE` touched, so a bare `UPDATE` (which
    /// takes no number, per the command line's grammar) knows which cue to
    /// re-capture.
    last_cue_number: Option<f64>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct EngineFrame {
    pub universe_id: u32,
    pub channels: Vec<u8>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct CommandResult {
    pub ok: bool,
    pub message: String,
    pub selection: Vec<u32>,
}

/// `Cue.values` is keyed by `(u32, u16)`, which can't cross the Tauri
/// boundary as-is (JSON object keys must be strings) — these DTOs flatten it
/// into a plain list. See `cues.rs`'s module docs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct CueValueEntry {
    pub universe: u32,
    pub channel: u16,
    pub value: CueValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct CueDto {
    pub number: f64,
    pub name: String,
    pub values: Vec<CueValueEntry>,
    pub fade_in_ms: f64,
    pub fade_out_ms: f64,
    pub delay_in_ms: f64,
    pub mark: bool,
    /// Phase 7; absent in files saved before effects existed.
    #[serde(default)]
    pub effects: Vec<EffectInstance>,
}

impl From<&Cue> for CueDto {
    fn from(cue: &Cue) -> Self {
        let mut values: Vec<CueValueEntry> = cue
            .values
            .iter()
            .map(|(&(universe, channel), value)| CueValueEntry { universe, channel, value: value.clone() })
            .collect();
        values.sort_by_key(|e| (e.universe, e.channel));
        Self {
            number: cue.number,
            name: cue.name.clone(),
            values,
            fade_in_ms: cue.fade_in_ms,
            fade_out_ms: cue.fade_out_ms,
            delay_in_ms: cue.delay_in_ms,
            mark: cue.mark,
            effects: cue.effects.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct CuelistDto {
    pub id: u32,
    pub name: String,
    pub kind: CuelistKind,
    pub tracking: bool,
    pub cues: Vec<CueDto>,
}

impl From<CuelistDto> for Cuelist {
    fn from(dto: CuelistDto) -> Self {
        let mut list = Cuelist::new(dto.id, dto.name, dto.kind, dto.tracking);
        for cue_dto in dto.cues {
            let mut cue = Cue::new(cue_dto.number, cue_dto.name);
            cue.values = cue_dto.values.into_iter().map(|e| ((e.universe, e.channel), e.value)).collect();
            cue.fade_in_ms = cue_dto.fade_in_ms;
            cue.fade_out_ms = cue_dto.fade_out_ms;
            cue.delay_in_ms = cue_dto.delay_in_ms;
            cue.mark = cue_dto.mark;
            cue.effects = cue_dto.effects;
            list.upsert_cue(cue); // keeps cues sorted even if the file isn't
        }
        list
    }
}

impl From<&Cuelist> for CuelistDto {
    fn from(list: &Cuelist) -> Self {
        Self {
            id: list.id,
            name: list.name.clone(),
            kind: list.kind,
            tracking: list.tracking,
            cues: list.cues.iter().map(CueDto::from).collect(),
        }
    }
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct PlaybackStatus {
    pub id: u32,
    pub cuelist_id: u32,
    pub current_cue_number: Option<f64>,
    pub fader: f32,
    pub fader_mode: FaderMode,
    pub paused: bool,
}

/// The engine-owned part of a show: what `.lumen` files store in their
/// `groups`/`presets`/`cuelists` fields (schema v4+).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct ShowEngineData {
    pub groups: Vec<Group>,
    pub presets: Vec<Preset>,
    pub cuelists: Vec<CuelistDto>,
    /// The global tempo (Phase 7).
    pub bpm: f32,
    pub pixel_maps: Vec<PixelMap>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct PixelMapStatus {
    pub map_id: u32,
    /// 0.0-1.0.
    pub intensity: f32,
}

/// Everything one engine tick reads and writes — shared (by `Arc`) between
/// `EngineManager` and its tick thread, and steppable directly in tests.
struct TickShared {
    state: Arc<Mutex<EngineState>>,
    playbacks: Arc<Mutex<Vec<RunningPlayback>>>,
    cues: Arc<Mutex<CueStore>>,
    cue_playbacks: Arc<Mutex<Vec<PlaybackRuntime>>>,
    presets: Arc<Mutex<PresetStore>>,
    patch_index: Arc<Mutex<Vec<PatchIndexEntry>>>,
    tempo: Arc<Mutex<Tempo>>,
    pixel_maps: Arc<Mutex<PixelMapStore>>,
    map_runtimes: Arc<Mutex<Vec<PixelMapRuntime>>>,
}

impl TickShared {
    /// Advances time by `dt_ms` and re-renders every layer (playbacks, pixel
    /// maps, effects). Output is the caller's job: merge and send.
    fn advance(&self, dt_ms: f64) {
        let clock = {
            let mut tempo = self.tempo.lock().unwrap();
            tempo.advance(dt_ms);
            EffectClock { seconds: tempo.seconds(), beats: tempo.beat() }
        };

        {
            // Lock order: patch index before state, as everywhere else.
            let patch = self.patch_index.lock().unwrap();
            let mut state = self.state.lock().unwrap();
            let mut playbacks = self.playbacks.lock().unwrap();
            playbacks.retain_mut(|playback| {
                let entry = state
                    .playbacks
                    .entry(playback.id.clone())
                    .or_insert_with(|| (playback.priority, Default::default()));
                // `advance` returning false only means "stop ticking
                // this playback" — a finished non-looping asset's
                // last values stay live in `state.playbacks` (a Scene
                // applies once and persists; see LightAsset's
                // docstring) until `stop_asset`/`trigger_asset`
                // explicitly removes that layer.
                playback.advance(dt_ms, &mut entry.1)
            });

            // Cue playbacks (Phase 6) render into the same
            // `state.playbacks` map, keyed distinctly, so they merge
            // through exactly the same HTP/LTP path as legacy assets.
            let cues = self.cues.lock().unwrap();
            let presets = self.presets.lock().unwrap();
            for pb in self.cue_playbacks.lock().unwrap().iter_mut() {
                let key = format!("cue-playback-{}", pb.id);
                let Some(cuelist) = cues.get(pb.cuelist_id) else {
                    state.playbacks.remove(&key);
                    continue;
                };
                pb.tick(dt_ms, cuelist, &presets);
                let entry = state.playbacks.entry(key).or_insert_with(|| (pb.priority, Default::default()));
                entry.0 = pb.priority;
                entry.1.values = pb.render_values();
                entry.1.effects = pb.effects();
                entry.1.intensity_scale = pb.intensity_scale();
            }

            // Pixel maps (Phase 7) are playback layers too.
            let maps = self.pixel_maps.lock().unwrap();
            let index: HashMap<u32, &PatchIndexEntry> = patch.iter().map(|e| (e.fixture_number, e)).collect();
            for runtime in self.map_runtimes.lock().unwrap().iter_mut() {
                let key = pixel_map_layer_key(runtime.map_id);
                let Some(map) = maps.get(runtime.map_id) else {
                    state.playbacks.remove(&key);
                    continue;
                };
                let frame = runtime.render_frame(map, clock);
                let entry = state.playbacks.entry(key).or_insert_with(|| (runtime.priority, Default::default()));
                entry.1.values = pixelmap::frame_to_channels(map, &frame, &index, runtime.intensity);
                entry.1.intensity_scale = runtime.intensity;
            }
            drop(maps);

            render_layer_effects(&mut state, &patch, &presets, clock);
        }
    }
}

pub struct EngineManager {
    state: Arc<Mutex<EngineState>>,
    playbacks: Arc<Mutex<Vec<RunningPlayback>>>,
    patch_index: Arc<Mutex<Vec<PatchIndexEntry>>>,
    programmer_ctx: Mutex<ProgrammerContext>,
    groups: Mutex<GroupStore>,
    presets: Arc<Mutex<PresetStore>>,
    cues: Arc<Mutex<CueStore>>,
    cue_playbacks: Arc<Mutex<Vec<PlaybackRuntime>>>,
    next_playback_id: AtomicU32,
    tick_started: AtomicBool,
    tempo: Arc<Mutex<Tempo>>,
    pixel_maps: Arc<Mutex<PixelMapStore>>,
    /// Running pixel maps, each a playback layer.
    map_runtimes: Arc<Mutex<Vec<PixelMapRuntime>>>,
    /// Ids for effect instances, unique across the programmer and every cue.
    next_effect_id: AtomicU32,
    /// Bumped by every change to groups/presets/cuelists, so the frontend
    /// can tell the show has unsaved engine-side edits (see `ShowEngineData`).
    data_revision: AtomicU64,
}

impl EngineManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(EngineState::new())),
            playbacks: Arc::new(Mutex::new(Vec::new())),
            patch_index: Arc::new(Mutex::new(Vec::new())),
            programmer_ctx: Mutex::new(ProgrammerContext::default()),
            groups: Mutex::new(GroupStore::new()),
            presets: Arc::new(Mutex::new(PresetStore::new())),
            cues: Arc::new(Mutex::new(CueStore::new())),
            cue_playbacks: Arc::new(Mutex::new(Vec::new())),
            next_playback_id: AtomicU32::new(0),
            tick_started: AtomicBool::new(false),
            tempo: Arc::new(Mutex::new(Tempo::new())),
            pixel_maps: Arc::new(Mutex::new(PixelMapStore::new())),
            map_runtimes: Arc::new(Mutex::new(Vec::new())),
            next_effect_id: AtomicU32::new(1),
            data_revision: AtomicU64::new(0),
        }
    }

    fn tick_shared(&self) -> TickShared {
        TickShared {
            state: self.state.clone(),
            playbacks: self.playbacks.clone(),
            cues: self.cues.clone(),
            cue_playbacks: self.cue_playbacks.clone(),
            presets: self.presets.clone(),
            patch_index: self.patch_index.clone(),
            tempo: self.tempo.clone(),
            pixel_maps: self.pixel_maps.clone(),
            map_runtimes: self.map_runtimes.clone(),
        }
    }

    /// One tick without the output half — for tests.
    #[cfg(test)]
    fn step(&self, dt_ms: f64) {
        self.tick_shared().advance(dt_ms);
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

    /// Immediate (unfaded) set into the programmer layer — what slider drags,
    /// color-preset buttons, and the Live AI console use.
    pub fn set_channels(&self, universe: u32, values: &HashMap<u16, u8>) {
        let mut state = self.state.lock().unwrap();
        for (&channel, &value) in values {
            state.programmer.values.insert((universe, channel), value as f32);
        }
    }

    pub fn get_universe_data(&self, universe: u32) -> [u8; UNIVERSE_SIZE] {
        let state = self.state.lock().unwrap();
        merge_universe(&state, universe)
    }

    /// Per-channel winning source for one universe (see `merge::ChannelSource`
    /// codes). A polled, read-only query for the Output/Live sheet; takes the
    /// locks briefly like any command and is never called from the tick.
    pub fn get_universe_sources(&self, universe: u32) -> Vec<u8> {
        let tracked = self.tracked_channels();
        let state = self.state.lock().unwrap();
        channel_sources(&state, universe, &tracked).iter().map(|s| *s as u8).collect()
    }

    /// Channels each cue playback holds from an earlier cue (tracking) rather
    /// than its current cue. Lock order matches the tick (cues, then
    /// cue_playbacks); both are released before `state` is taken.
    fn tracked_channels(&self) -> TrackedChannels {
        let cues = self.cues.lock().unwrap();
        let playbacks = self.cue_playbacks.lock().unwrap();
        let mut out = TrackedChannels::new();
        for pb in playbacks.iter() {
            let (Some(index), Some(cuelist)) = (pb.current_index, cues.get(pb.cuelist_id)) else { continue };
            if !cuelist.tracking {
                continue;
            }
            let Some(current) = cuelist.cues.get(index) else { continue };
            let tracked: HashSet<ChannelKey> = cuelist.cues[..index]
                .iter()
                .flat_map(|c| c.values.keys().copied())
                .filter(|key| !current.values.contains_key(key))
                .collect();
            out.insert(format!("cue-playback-{}", pb.id), tracked);
        }
        out
    }

    pub fn trigger_asset(&self, id: String, universe: u32, asset: LegacyAsset) {
        self.stop_asset(&id);
        let priority = {
            let mut state = self.state.lock().unwrap();
            let priority = state.next_priority();
            state.playbacks.insert(id.clone(), (priority, Default::default()));
            priority
        };
        self.playbacks.lock().unwrap().push(RunningPlayback::new(id, universe, priority, asset));
    }

    pub fn stop_asset(&self, id: &str) {
        self.playbacks.lock().unwrap().retain(|p| p.id != id);
        self.state.lock().unwrap().playbacks.remove(id);
    }

    /// Parses and executes one command-line entry against the current
    /// selection/patch index. See `command_line`'s module docs for the
    /// supported grammar and what's a parse-only stub (groups, cues).
    pub fn execute_command(&self, text: &str) -> CommandResult {
        let selection_vec = |ctx: &ProgrammerContext| {
            let mut v: Vec<u32> = ctx.selection.iter().copied().collect();
            v.sort_unstable();
            v
        };

        let command = match command_line::parse(text) {
            Ok(c) => c,
            Err(e) => {
                let ctx = self.programmer_ctx.lock().unwrap();
                return CommandResult { ok: false, message: e, selection: selection_vec(&ctx) };
            }
        };

        let known: HashSet<u32> =
            self.patch_index.lock().unwrap().iter().map(|e| e.fixture_number).collect();
        let group_members = self.groups.lock().unwrap().fixture_numbers_by_id();
        let mut ctx = self.programmer_ctx.lock().unwrap();

        let (ok, message) = match command {
            Command::Select(expr) => match expr.resolve(&known, &group_members) {
                Ok(set) => {
                    ctx.selection = set;
                    (true, format!("Selected {} fixture(s)", ctx.selection.len()))
                }
                Err(e) => (false, e),
            },
            Command::SetIntensity(expr, value) => {
                let target = match expr {
                    // "1 THRU 8 @ 50" both selects and sets intensity, like a
                    // real console's command line.
                    Some(expr) => match expr.resolve(&known, &group_members) {
                        Ok(set) => {
                            ctx.selection = set.clone();
                            set
                        }
                        Err(e) => {
                            return CommandResult { ok: false, message: e, selection: selection_vec(&ctx) }
                        }
                    },
                    None => ctx.selection.clone(),
                };
                self.apply_intensity(&target, value.as_u8())
            }
            Command::Clear => {
                if ctx.selection.is_empty() {
                    self.state.lock().unwrap().programmer.clear();
                    (true, "Cleared programmer values".to_string())
                } else {
                    ctx.selection.clear();
                    (true, "Cleared selection".to_string())
                }
            }
            Command::Release => {
                ctx.selection.clear();
                self.state.lock().unwrap().programmer.clear();
                (true, "Released".to_string())
            }
            Command::SelectAll => {
                ctx.selection = known.clone();
                (true, format!("Selected all {} fixture(s)", ctx.selection.len()))
            }
            Command::Next | Command::Prev => match ctx.current_cuelist {
                None => (false, "No current cuelist — select one in the Cuelists tab first".to_string()),
                Some(cuelist_id) => match self.find_playback_for_cuelist(cuelist_id) {
                    None => (false, "No active playback for this cuelist — add one from the Cuelists tab".to_string()),
                    Some(playback_id) => {
                        let result = if command == Command::Next {
                            self.playback_go(playback_id)
                        } else {
                            self.playback_go_back(playback_id)
                        };
                        match result {
                            Ok(()) => (true, if command == Command::Next { "GO".to_string() } else { "Back".to_string() }),
                            Err(e) => (false, e),
                        }
                    }
                },
            },
            Command::SetTime(seconds) => {
                ctx.pending_time_s = seconds;
                (true, format!("Fade time set to {seconds}s (applies to the next RECORD CUE/UPDATE)"))
            }
            Command::RecordCue(number) => match ctx.current_cuelist {
                None => (false, "No current cuelist — select one in the Cuelists tab first".to_string()),
                Some(cuelist_id) => {
                    let number: f64 = number.into();
                    let fade_ms = (ctx.pending_time_s as f64) * 1000.0;
                    let selection = ctx.selection.clone();
                    match self.record_cue(cuelist_id, number, format!("Cue {number}"), &selection, fade_ms, fade_ms) {
                        Ok(()) => {
                            ctx.last_cue_number = Some(number);
                            (true, format!("Recorded cue {number}"))
                        }
                        Err(e) => (false, e),
                    }
                }
            },
            Command::UpdateCue => match (ctx.current_cuelist, ctx.last_cue_number) {
                (Some(cuelist_id), Some(number)) => {
                    let name = self
                        .cues
                        .lock()
                        .unwrap()
                        .get(cuelist_id)
                        .and_then(|list| list.cues.iter().find(|c| (c.number - number).abs() < f64::EPSILON))
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| format!("Cue {number}"));
                    let fade_ms = (ctx.pending_time_s as f64) * 1000.0;
                    let selection = ctx.selection.clone();
                    match self.record_cue(cuelist_id, number, name, &selection, fade_ms, fade_ms) {
                        Ok(()) => (true, format!("Updated cue {number}")),
                        Err(e) => (false, e),
                    }
                }
                _ => (false, "No cue to update yet — RECORD CUE first".to_string()),
            },
            Command::DeleteCue(number) => match ctx.current_cuelist {
                None => (false, "No current cuelist — select one in the Cuelists tab first".to_string()),
                Some(cuelist_id) => match self.delete_cue(cuelist_id, number.into()) {
                    Ok(()) => (true, format!("Deleted cue {number}")),
                    Err(e) => (false, e),
                },
            },
            // COPY/MOVE parse (and are unit-tested) but take no arguments in
            // this project's grammar (see command_line.rs's module docs) --
            // there's no target number to copy/move to. Rather than guess at
            // syntax BUILD_PLAN doesn't specify, they stay unimplemented; see
            // DECISIONS.md.
            Command::Copy | Command::Move => {
                (false, "COPY/MOVE need a target cue number the command line doesn't have syntax for yet".to_string())
            }
        };

        CommandResult { ok, message, selection: selection_vec(&ctx) }
    }

    fn apply_intensity(&self, fixture_numbers: &HashSet<u32>, value: u8) -> (bool, String) {
        let index = self.patch_index.lock().unwrap();
        let mut state = self.state.lock().unwrap();
        let mut set_count = 0;
        let mut skipped = 0;
        for &number in fixture_numbers {
            match index.iter().find(|e| e.fixture_number == number) {
                Some(entry) => match entry.intensity_channel {
                    Some(channel) => {
                        state.programmer.values.insert((entry.universe, channel), value as f32);
                        set_count += 1;
                    }
                    None => skipped += 1,
                },
                None => skipped += 1,
            }
        }
        let message = if skipped == 0 {
            format!("Set intensity on {set_count} fixture(s)")
        } else {
            format!("Set intensity on {set_count} fixture(s), skipped {skipped} (no dimmer channel or unpatched)")
        };
        (set_count > 0 || fixture_numbers.is_empty(), message)
    }

    // --- Groups (Phase 5) ---

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

    /// A "group master": sets every member's intensity to `percent` (0-100),
    /// the same operation as the command line's `GROUP N @ value`. Not a
    /// continuously-multiplying scaling layer over whatever else is live —
    /// see DECISIONS.md for why that's out of scope here.
    pub fn apply_group_master(&self, id: u32, percent: f32) -> Result<(bool, String), String> {
        let members: HashSet<u32> = self
            .groups
            .lock()
            .unwrap()
            .get(id)
            .ok_or_else(|| format!("No group {id}"))?
            .fixture_numbers
            .iter()
            .copied()
            .collect();
        let value = (percent.clamp(0.0, 100.0) / 100.0 * 255.0).round() as u8;
        Ok(self.apply_intensity(&members, value))
    }

    // --- Presets (Phase 5) ---

    /// Reads each target fixture's *currently rendered* (post-merge) value
    /// for every attribute it has, so recording a preset captures what you
    /// actually see, not just whatever the programmer layer happens to hold.
    fn capture_attribute_values(&self, fixture_numbers: &HashSet<u32>) -> HashMap<String, f32> {
        let index = self.patch_index.lock().unwrap();
        let state = self.state.lock().unwrap();
        let mut frames: HashMap<u32, [u8; UNIVERSE_SIZE]> = HashMap::new();
        let mut values: HashMap<String, f32> = HashMap::new();

        let mut entries: Vec<&PatchIndexEntry> =
            index.iter().filter(|e| fixture_numbers.contains(&e.fixture_number)).collect();
        entries.sort_by_key(|e| e.fixture_number);

        for entry in entries {
            let frame = frames.entry(entry.universe).or_insert_with(|| merge_universe(&state, entry.universe));
            for (attr, &channel) in &entry.attribute_channels {
                values.entry(attr.clone()).or_insert(frame[(channel - 1) as usize] as f32);
            }
        }
        values
    }

    pub fn record_preset(
        &self,
        family: PresetFamily,
        name: String,
        fixture_numbers: &HashSet<u32>,
        color: Option<String>,
    ) -> u32 {
        let values = self.capture_attribute_values(fixture_numbers);
        self.mark_data_changed();
        self.presets.lock().unwrap().record(family, name, values, color)
    }

    pub fn update_preset(&self, id: u32, fixture_numbers: &HashSet<u32>, color: Option<String>) -> Result<(), String> {
        let values = self.capture_attribute_values(fixture_numbers);
        self.mark_data_changed();
        self.presets.lock().unwrap().update(id, values, color)
    }

    pub fn rename_preset(&self, id: u32, name: String) -> Result<(), String> {
        self.mark_data_changed();
        self.presets.lock().unwrap().rename(id, name)
    }

    pub fn delete_preset(&self, id: u32) {
        self.mark_data_changed();
        self.presets.lock().unwrap().delete(id);
    }

    pub fn list_presets(&self, family: Option<PresetFamily>) -> Vec<Preset> {
        self.presets.lock().unwrap().list(family)
    }

    /// Applies preset `id` to every fixture in `fixture_numbers` that has a
    /// channel for one of its attributes. Always looks the preset up fresh by
    /// id — never caches/copies its values — so updating a preset changes
    /// what every future apply (including, once Phase 6 exists, a recorded
    /// cue that references it) actually does.
    pub fn apply_preset(&self, id: u32, fixture_numbers: &HashSet<u32>) -> Result<usize, String> {
        let (values, effect) = {
            let presets = self.presets.lock().unwrap();
            let preset = presets.get(id).ok_or_else(|| format!("No preset {id}"))?;
            (preset.values.clone(), preset.effect.clone())
        };
        if let Some(def) = effect {
            // An Effect preset runs its effect on the selection, linked to
            // the preset so a recorded cue keeps following it.
            let mut ordered: Vec<u32> = fixture_numbers.iter().copied().collect();
            ordered.sort_unstable();
            self.start_effect(def, ordered, Some(id));
            return Ok(fixture_numbers.len());
        }
        let index = self.patch_index.lock().unwrap();
        let mut state = self.state.lock().unwrap();
        let mut applied = 0;
        for entry in index.iter().filter(|e| fixture_numbers.contains(&e.fixture_number)) {
            for (attr, &value) in &values {
                if let Some(&channel) = entry.attribute_channels.get(attr) {
                    state.programmer.values.insert((entry.universe, channel), value);
                    applied += 1;
                }
            }
        }
        Ok(applied)
    }

    // --- Cuelists and cues (Phase 6) ---

    pub fn create_cuelist(&self, name: String, kind: CuelistKind, tracking: bool) -> u32 {
        self.mark_data_changed();
        self.cues.lock().unwrap().create(name, kind, tracking)
    }

    pub fn list_cuelists(&self) -> Vec<CuelistDto> {
        self.cues.lock().unwrap().list().iter().map(CuelistDto::from).collect()
    }

    pub fn delete_cuelist(&self, id: u32) {
        self.mark_data_changed();
        self.cues.lock().unwrap().delete(id);
        // Any playback still pointing at it stops contributing next tick
        // (its `cues.get(cuelist_id)` lookup will miss and its layer is
        // dropped), but explicitly drop the runtimes too so `list_playbacks`
        // doesn't keep showing them.
        self.cue_playbacks.lock().unwrap().retain(|p| p.cuelist_id != id);
    }

    pub fn rename_cuelist(&self, id: u32, name: String) -> Result<(), String> {
        self.mark_data_changed();
        let mut cues = self.cues.lock().unwrap();
        let list = cues.get_mut(id).ok_or_else(|| format!("No cuelist {id}"))?;
        list.name = name;
        Ok(())
    }

    pub fn set_cuelist_tracking(&self, id: u32, tracking: bool) -> Result<(), String> {
        self.mark_data_changed();
        let mut cues = self.cues.lock().unwrap();
        let list = cues.get_mut(id).ok_or_else(|| format!("No cuelist {id}"))?;
        list.tracking = tracking;
        Ok(())
    }

    /// Sets which cuelist the command line's `RECORD CUE`/`UPDATE`/`DELETE
    /// CUE`/`NEXT`/`PREV` operate against — called when the Cuelists tab
    /// selects one. `None` clears it (e.g. the cuelist was deleted).
    pub fn set_current_cuelist(&self, id: Option<u32>) {
        self.programmer_ctx.lock().unwrap().current_cuelist = id;
    }

    /// Records a cue from `fixture_numbers`' currently-rendered (post-merge)
    /// values — the same "record what you see" approach as
    /// `record_preset`/`update_preset`. Every attribute channel those
    /// fixtures have becomes a `CueValue::Literal`; use
    /// `set_cue_preset_reference` afterwards to turn specific channels into
    /// live preset references instead.
    pub fn record_cue(
        &self,
        cuelist_id: u32,
        number: f64,
        name: String,
        fixture_numbers: &HashSet<u32>,
        fade_in_ms: f64,
        fade_out_ms: f64,
    ) -> Result<(), String> {
        let index = self.patch_index.lock().unwrap();
        let state = self.state.lock().unwrap();
        let mut frames: HashMap<u32, [u8; UNIVERSE_SIZE]> = HashMap::new();
        let mut cue = Cue::new(number, name);
        cue.fade_in_ms = fade_in_ms;
        cue.fade_out_ms = fade_out_ms;

        let mut entries: Vec<&PatchIndexEntry> =
            index.iter().filter(|e| fixture_numbers.contains(&e.fixture_number)).collect();
        entries.sort_by_key(|e| e.fixture_number);
        for entry in entries {
            // The static (effect-free) merge: a running effect is recorded
            // as an effect below, not as whatever value it happened to be
            // at this instant.
            let frame = frames.entry(entry.universe).or_insert_with(|| merge_universe_static(&state, entry.universe));
            for &channel in entry.attribute_channels.values() {
                cue.values.insert((entry.universe, channel), CueValue::Literal { value: frame[(channel - 1) as usize] as f32 });
            }
        }
        cue.effects = programmer_effects_for(&state.programmer, fixture_numbers);
        drop(state);
        drop(index);

        self.mark_data_changed();
        let mut cues = self.cues.lock().unwrap();
        let list = cues.get_mut(cuelist_id).ok_or_else(|| format!("No cuelist {cuelist_id}"))?;
        list.upsert_cue(cue);
        Ok(())
    }

    pub fn delete_cue(&self, cuelist_id: u32, number: f64) -> Result<(), String> {
        self.mark_data_changed();
        let mut cues = self.cues.lock().unwrap();
        let list = cues.get_mut(cuelist_id).ok_or_else(|| format!("No cuelist {cuelist_id}"))?;
        list.remove_cue(number);
        Ok(())
    }

    /// Turns one channel of an existing cue into a live reference to preset
    /// `preset_id`'s `attribute` value, instead of a literal — this is what
    /// makes "updating a preset changes every cue that references it" real
    /// for an actual recorded cue, not just the unit-tested mechanism.
    pub fn set_cue_preset_reference(
        &self,
        cuelist_id: u32,
        cue_number: f64,
        fixture_number: u32,
        attribute: String,
        preset_id: u32,
    ) -> Result<(), String> {
        let (universe, channel) = {
            let index = self.patch_index.lock().unwrap();
            let entry = index
                .iter()
                .find(|e| e.fixture_number == fixture_number)
                .ok_or_else(|| format!("No fixture {fixture_number}"))?;
            let channel = *entry
                .attribute_channels
                .get(&attribute)
                .ok_or_else(|| format!("Fixture {fixture_number} has no '{attribute}' channel"))?;
            (entry.universe, channel)
        };

        self.mark_data_changed();
        let mut cues = self.cues.lock().unwrap();
        let list = cues.get_mut(cuelist_id).ok_or_else(|| format!("No cuelist {cuelist_id}"))?;
        let index = list.index_of(cue_number).ok_or_else(|| format!("No cue {cue_number}"))?;
        list.cues[index].values.insert((universe, channel), CueValue::Preset { preset_id, attribute });
        Ok(())
    }

    /// Undoes `set_cue_preset_reference` for one channel: the value becomes a
    /// literal equal to what the preset currently gives it, so the cue's look
    /// is unchanged but no longer follows later edits to that preset.
    pub fn unlink_cue_preset(&self, cuelist_id: u32, cue_number: f64, universe: u32, channel: u16) -> Result<(), String> {
        self.mark_data_changed();
        let mut cues = self.cues.lock().unwrap();
        let presets = self.presets.lock().unwrap();
        let list = cues.get_mut(cuelist_id).ok_or_else(|| format!("No cuelist {cuelist_id}"))?;
        let index = list.index_of(cue_number).ok_or_else(|| format!("No cue {cue_number}"))?;
        let value = list.cues[index]
            .values
            .get_mut(&(universe, channel))
            .ok_or_else(|| format!("Cue {cue_number} has no value for {universe}.{channel}"))?;
        if !matches!(value, CueValue::Preset { .. }) {
            return Err(format!("Cue {cue_number}'s {universe}.{channel} isn't linked to a preset"));
        }
        *value = CueValue::Literal { value: resolve_cue_value(value, &presets) };
        Ok(())
    }

    pub fn set_chase_tempo(&self, cuelist_id: u32, bpm: f32) -> Result<(), String> {
        self.mark_data_changed();
        let mut cues = self.cues.lock().unwrap();
        let list = cues.get_mut(cuelist_id).ok_or_else(|| format!("No cuelist {cuelist_id}"))?;
        match &mut list.kind {
            CuelistKind::Chase { bpm: current, .. } => {
                *current = bpm;
                Ok(())
            }
            _ => Err("Not a chase cuelist".to_string()),
        }
    }

    // --- Show data (saved in .lumen files) ---

    fn mark_data_changed(&self) {
        self.data_revision.fetch_add(1, Ordering::SeqCst);
    }

    /// Changes whenever groups/presets/cuelists change; compare against the
    /// value at the last save/load to know whether there's unsaved work.
    pub fn show_data_revision(&self) -> u64 {
        self.data_revision.load(Ordering::SeqCst)
    }

    pub fn export_show_data(&self) -> ShowEngineData {
        ShowEngineData {
            groups: self.list_groups(),
            presets: self.list_presets(None),
            cuelists: self.list_cuelists(),
            bpm: self.tempo.lock().unwrap().bpm(),
            pixel_maps: self.list_pixel_maps(),
        }
    }

    /// Replaces all groups/presets/cuelists with a show's (open/new/restore).
    /// Running cue playbacks and the command line's current cuelist point at
    /// the outgoing cuelists' ids, so they're dropped first.
    pub fn load_show_data(&self, data: ShowEngineData) {
        let playback_ids: Vec<u32> = self.cue_playbacks.lock().unwrap().drain(..).map(|p| p.id).collect();
        {
            let mut state = self.state.lock().unwrap();
            for id in playback_ids {
                state.playbacks.remove(&format!("cue-playback-{id}"));
            }
        }
        {
            let mut ctx = self.programmer_ctx.lock().unwrap();
            ctx.current_cuelist = None;
            ctx.last_cue_number = None;
        }
        let max_effect_id = data.cuelists.iter().flat_map(|l| &l.cues).flat_map(|c| &c.effects).map(|e| e.id).max();
        let max_effect_id = max_effect_id.into_iter().chain(self.list_effects().iter().map(|e| e.id)).max().unwrap_or(0);
        self.next_effect_id.fetch_max(max_effect_id + 1, Ordering::SeqCst);
        self.tempo.lock().unwrap().set_bpm(data.bpm);
        let map_ids: Vec<u32> = self.map_runtimes.lock().unwrap().drain(..).map(|r| r.map_id).collect();
        {
            let mut state = self.state.lock().unwrap();
            for id in map_ids {
                state.playbacks.remove(&pixel_map_layer_key(id));
            }
        }
        self.pixel_maps.lock().unwrap().replace_all(data.pixel_maps);
        self.groups.lock().unwrap().replace_all(data.groups);
        self.presets.lock().unwrap().replace_all(data.presets);
        self.cues.lock().unwrap().replace_all(data.cuelists.into_iter().map(Cuelist::from).collect());
        self.mark_data_changed();
    }

    // --- Tempo (Phase 7) ---

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

    // --- Programmer effects (Phase 7) ---

    /// Starts `def` on `fixtures` (in that order) in the programmer. Any
    /// programmer effect already driving one of the same attributes on one
    /// of these fixtures gives those fixtures up — applying a new effect to
    /// an attribute replaces the old one, it doesn't stack.
    pub fn start_effect(&self, def: EffectDef, fixtures: Vec<u32>, preset_id: Option<u32>) -> u32 {
        let id = self.next_effect_id.fetch_add(1, Ordering::SeqCst);
        let mut state = self.state.lock().unwrap();
        let attributes: HashSet<String> = def.attributes().into_iter().map(str::to_string).collect();
        let taken: HashSet<u32> = fixtures.iter().copied().collect();
        let programmer = &mut state.programmer;
        for existing in programmer.effects.iter_mut() {
            if existing.instance.def.attributes().iter().any(|a| attributes.contains(*a)) {
                existing.instance.fixtures.retain(|f| !taken.contains(f));
            }
        }
        programmer.effects.retain(|e| !e.instance.fixtures.is_empty());
        programmer.effects.push(LayerEffect { instance: EffectInstance { id, def, preset_id, fixtures }, scale: 1.0 });
        id
    }

    /// Replaces a running programmer effect's definition. Unlinks it from
    /// its preset, since it no longer plays the preset's definition.
    pub fn update_effect(&self, id: u32, def: EffectDef) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        let effect = state
            .programmer
            .effects
            .iter_mut()
            .find(|e| e.instance.id == id)
            .ok_or_else(|| format!("No running effect {id}"))?;
        effect.instance.def = def;
        effect.instance.preset_id = None;
        Ok(())
    }

    pub fn stop_effect(&self, id: u32) {
        self.state.lock().unwrap().programmer.effects.retain(|e| e.instance.id != id);
    }

    /// The programmer's running effects, with preset-linked ones showing
    /// the preset's current definition.
    pub fn list_effects(&self) -> Vec<EffectInstance> {
        let effects: Vec<EffectInstance> =
            self.state.lock().unwrap().programmer.effects.iter().map(|e| e.instance.clone()).collect();
        let presets = self.presets.lock().unwrap();
        effects
            .into_iter()
            .map(|mut e| {
                if let Some(def) = e.preset_id.and_then(|id| presets.get(id)).and_then(|p| p.effect.clone()) {
                    e.def = def;
                }
                e
            })
            .collect()
    }

    pub fn record_effect_preset(&self, name: String, def: EffectDef) -> u32 {
        self.mark_data_changed();
        self.presets.lock().unwrap().record_effect(name, def)
    }

    pub fn update_effect_preset(&self, id: u32, def: EffectDef) -> Result<(), String> {
        self.mark_data_changed();
        self.presets.lock().unwrap().update_effect(id, def)
    }

    // --- Pixel maps (Phase 7) ---

    pub fn create_pixel_map(&self, name: String, width: u32, height: u32) -> u32 {
        self.mark_data_changed();
        self.pixel_maps.lock().unwrap().create(name, width, height)
    }

    pub fn update_pixel_map(&self, map: PixelMap) -> Result<(), String> {
        self.mark_data_changed();
        self.pixel_maps.lock().unwrap().update(map)
    }

    pub fn delete_pixel_map(&self, id: u32) {
        self.mark_data_changed();
        self.stop_pixel_map(id);
        self.pixel_maps.lock().unwrap().delete(id);
    }

    pub fn list_pixel_maps(&self) -> Vec<PixelMap> {
        self.pixel_maps.lock().unwrap().list()
    }

    /// Decodes an image/GIF, scales it to the map and switches the map to
    /// play it. A GIF's own frame timing becomes one cycle of the map's
    /// rate, so it plays at its natural speed until the rate is changed.
    pub fn import_pixel_map_media(&self, id: u32, bytes: &[u8]) -> Result<(), String> {
        let mut maps = self.pixel_maps.lock().unwrap();
        let mut map = maps.get(id).ok_or_else(|| format!("No pixel map {id}"))?.clone();
        let media = pixelmap::import_media(bytes, map.width, map.height)?;
        let total_ms: f64 = media.frames.iter().map(|f| f.delay_ms.max(1.0)).sum();
        map.rate = effects::EffectRate::Hz { hz: (1000.0 / total_ms) as f32 };
        map.generator = pixelmap::Generator::Media;
        map.media = Some(media);
        maps.update(map)?;
        drop(maps);
        self.mark_data_changed();
        Ok(())
    }

    pub fn start_pixel_map(&self, id: u32) -> Result<(), String> {
        if self.pixel_maps.lock().unwrap().get(id).is_none() {
            return Err(format!("No pixel map {id}"));
        }
        if self.map_runtimes.lock().unwrap().iter().any(|r| r.map_id == id) {
            return Ok(());
        }
        let priority = self.state.lock().unwrap().next_priority();
        self.map_runtimes.lock().unwrap().push(PixelMapRuntime::new(id, priority));
        Ok(())
    }

    pub fn stop_pixel_map(&self, id: u32) {
        self.map_runtimes.lock().unwrap().retain(|r| r.map_id != id);
        self.state.lock().unwrap().playbacks.remove(&pixel_map_layer_key(id));
    }

    pub fn set_pixel_map_intensity(&self, id: u32, percent: f32) -> Result<(), String> {
        let mut runtimes = self.map_runtimes.lock().unwrap();
        let runtime = runtimes.iter_mut().find(|r| r.map_id == id).ok_or_else(|| format!("Pixel map {id} isn't running"))?;
        runtime.intensity = percent.clamp(0.0, 100.0) / 100.0;
        Ok(())
    }

    pub fn running_pixel_maps(&self) -> Vec<PixelMapStatus> {
        self.map_runtimes
            .lock()
            .unwrap()
            .iter()
            .map(|r| PixelMapStatus { map_id: r.map_id, intensity: r.intensity })
            .collect()
    }

    /// The map's current frame as `width × height × 3` RGB bytes: the
    /// running map's last output, or (for a stopped map, e.g. a virtual
    /// matrix in the editor) a frame rendered now at the current clock.
    pub fn pixel_map_preview(&self, id: u32) -> Result<Vec<u8>, String> {
        let frame = {
            let runtimes = self.map_runtimes.lock().unwrap();
            runtimes.iter().find(|r| r.map_id == id).map(|r| r.last_frame.clone())
        };
        let frame = match frame {
            Some(frame) if !frame.is_empty() => frame,
            _ => {
                let map = self.pixel_maps.lock().unwrap().get(id).cloned().ok_or_else(|| format!("No pixel map {id}"))?;
                let clock = {
                    let tempo = self.tempo.lock().unwrap();
                    EffectClock { seconds: tempo.seconds(), beats: tempo.beat() }
                };
                PixelMapRuntime::new(id, 0).render_frame(&map, clock)
            }
        };
        Ok(frame.iter().flat_map(|c| c.map(|v| v.round().clamp(0.0, 255.0) as u8)).collect())
    }

    // --- Playbacks (Phase 6) ---

    pub fn create_playback(&self, cuelist_id: u32) -> Result<u32, String> {
        let kind = self.cues.lock().unwrap().get(cuelist_id).ok_or_else(|| format!("No cuelist {cuelist_id}"))?.kind;
        let base_priority = self.state.lock().unwrap().next_priority();
        let priority = if kind == CuelistKind::Override { OVERRIDE_PRIORITY_BASE + base_priority } else { base_priority };

        let id = self.next_playback_id.fetch_add(1, Ordering::SeqCst);
        let mut runtime = PlaybackRuntime::new(id, cuelist_id, priority);
        if kind == CuelistKind::Submaster {
            // A submaster has no GO stepping -- it always renders cue 0
            // (tracked), scaled by its fader.
            let cues = self.cues.lock().unwrap();
            let presets = self.presets.lock().unwrap();
            if let Some(list) = cues.get(cuelist_id) {
                if !list.cues.is_empty() {
                    runtime.go(list, &presets, 0);
                }
            }
        }
        self.cue_playbacks.lock().unwrap().push(runtime);
        Ok(id)
    }

    pub fn delete_playback(&self, id: u32) {
        self.cue_playbacks.lock().unwrap().retain(|p| p.id != id);
        self.state.lock().unwrap().playbacks.remove(&format!("cue-playback-{id}"));
    }

    pub fn list_playbacks(&self) -> Vec<PlaybackStatus> {
        let cues = self.cues.lock().unwrap();
        self.cue_playbacks
            .lock()
            .unwrap()
            .iter()
            .map(|pb| PlaybackStatus {
                id: pb.id,
                cuelist_id: pb.cuelist_id,
                current_cue_number: pb
                    .current_index
                    .and_then(|i| cues.get(pb.cuelist_id).and_then(|list| list.cues.get(i)))
                    .map(|c| c.number),
                fader: pb.fader,
                fader_mode: pb.fader_mode,
                paused: pb.paused,
            })
            .collect()
    }

    /// The first playback slot attached to `cuelist_id`, if any — what the
    /// command line's `NEXT`/`PREV` step, since the command line has no
    /// concept of playback ids, only of "the current cuelist".
    fn find_playback_for_cuelist(&self, cuelist_id: u32) -> Option<u32> {
        self.cue_playbacks.lock().unwrap().iter().find(|p| p.cuelist_id == cuelist_id).map(|p| p.id)
    }

    fn with_playback<R>(&self, id: u32, f: impl FnOnce(&mut PlaybackRuntime, &Cuelist, &PresetStore) -> R) -> Result<R, String> {
        let mut playbacks = self.cue_playbacks.lock().unwrap();
        let pb = playbacks.iter_mut().find(|p| p.id == id).ok_or_else(|| format!("No playback {id}"))?;
        let cues = self.cues.lock().unwrap();
        let list = cues.get(pb.cuelist_id).ok_or_else(|| format!("Cuelist {} is missing", pb.cuelist_id))?;
        let presets = self.presets.lock().unwrap();
        Ok(f(pb, list, &presets))
    }

    pub fn playback_go(&self, id: u32) -> Result<(), String> {
        self.with_playback(id, |pb, list, presets| pb.go_next(list, presets))
    }

    pub fn playback_go_back(&self, id: u32) -> Result<(), String> {
        self.with_playback(id, |pb, list, presets| pb.go_back(list, presets))
    }

    pub fn playback_release(&self, id: u32) -> Result<(), String> {
        self.with_playback(id, |pb, _list, _presets| pb.release(DEFAULT_RELEASE_FADE_MS))
    }

    pub fn playback_set_paused(&self, id: u32, paused: bool) -> Result<(), String> {
        let mut playbacks = self.cue_playbacks.lock().unwrap();
        let pb = playbacks.iter_mut().find(|p| p.id == id).ok_or_else(|| format!("No playback {id}"))?;
        pb.paused = paused;
        Ok(())
    }

    pub fn playback_set_fader(&self, id: u32, percent: f32) -> Result<(), String> {
        let mut playbacks = self.cue_playbacks.lock().unwrap();
        let pb = playbacks.iter_mut().find(|p| p.id == id).ok_or_else(|| format!("No playback {id}"))?;
        pb.fader = (percent.clamp(0.0, 100.0)) / 100.0;
        Ok(())
    }

    pub fn playback_set_fader_mode(&self, id: u32, mode: FaderMode) -> Result<(), String> {
        let mut playbacks = self.cue_playbacks.lock().unwrap();
        let pb = playbacks.iter_mut().find(|p| p.id == id).ok_or_else(|| format!("No playback {id}"))?;
        pb.fader_mode = mode;
        Ok(())
    }
}

fn pixel_map_layer_key(id: u32) -> String {
    format!("pixelmap-{id}")
}

/// The programmer's effects restricted to `fixture_numbers` (in each
/// effect's own fixture order) — what `record_cue` captures.
fn programmer_effects_for(programmer: &Layer, fixture_numbers: &HashSet<u32>) -> Vec<EffectInstance> {
    programmer
        .effects
        .iter()
        .filter_map(|e| {
            let fixtures: Vec<u32> = e.instance.fixtures.iter().copied().filter(|f| fixture_numbers.contains(f)).collect();
            (!fixtures.is_empty()).then(|| EffectInstance { fixtures, ..e.instance.clone() })
        })
        .collect()
}

/// Renders every layer's effects into its `fx_values` for this tick.
fn render_layer_effects(state: &mut EngineState, patch: &[PatchIndexEntry], presets: &PresetStore, clock: EffectClock) {
    let any_effects = state.programmer.has_effects() || state.playbacks.values().any(|(_, l)| l.has_effects());
    if !any_effects {
        state.programmer.fx_values.clear();
        for (_, layer) in state.playbacks.values_mut() {
            layer.fx_values.clear();
        }
        return;
    }

    // A relative effect swings around the underlying value. For a channel
    // its own layer sets, that's the layer's value; otherwise it's what the
    // rest of the show (without any effects) is outputting there.
    let universes: HashSet<u32> = patch.iter().map(|e| e.universe).collect();
    let static_frames: HashMap<u32, [u8; UNIVERSE_SIZE]> =
        universes.into_iter().map(|u| (u, merge_universe_static(state, u))).collect();
    let index: HashMap<u32, &PatchIndexEntry> = patch.iter().map(|e| (e.fixture_number, e)).collect();

    let layers = std::iter::once(&mut state.programmer).chain(state.playbacks.values_mut().map(|(_, l)| l));
    for layer in layers {
        let Layer { values, effects, fx_values, .. } = layer;
        fx_values.clear();
        if effects.is_empty() {
            continue;
        }
        // Preset-linked effects play the preset's current definition.
        let defs: Vec<EffectDef> = effects
            .iter()
            .map(|e| {
                e.instance
                    .preset_id
                    .and_then(|id| presets.get(id))
                    .and_then(|p| p.effect.clone())
                    .unwrap_or_else(|| e.instance.def.clone())
            })
            .collect();
        let resolved: Vec<ResolvedEffect> = effects
            .iter()
            .zip(&defs)
            .map(|(e, def)| ResolvedEffect { def, fixtures: &e.instance.fixtures, scale: e.scale })
            .collect();
        let base = |key: ChannelKey| {
            values
                .get(&key)
                .copied()
                .unwrap_or_else(|| static_frames.get(&key.0).map_or(0.0, |frame| frame[(key.1 - 1) as usize] as f32))
        };
        effects::render(&resolved, &index, clock, &base, fx_values);
    }
}

impl Default for EngineManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-dimmer-channel fixture. Mirrors how `useShowStore.ts`'s
    /// `syncEnginePatch` actually populates `PatchIndexEntry` in production:
    /// a dimmer channel sets both `intensity_channel` *and* an
    /// `attribute_channels["dimmer"]` entry (it falls through the "first
    /// occurrence of each attribute wins" logic same as every other
    /// channel type) — a cue/preset capture reads only `attribute_channels`,
    /// so a fixture with no non-dimmer attributes still needs this to have
    /// anything to record.
    fn patched_fixture(number: u32, universe: u32, intensity_channel: u16) -> PatchIndexEntry {
        PatchIndexEntry {
            fixture_number: number,
            universe,
            intensity_channel: Some(intensity_channel),
            attribute_channels: [("dimmer".to_string(), intensity_channel)].into_iter().collect(),
            cells: Vec::new(),
        }
    }

    #[test]
    fn record_cue_via_command_line_captures_the_current_selection() {
        let engine = EngineManager::new();
        engine.set_patch_index(vec![patched_fixture(1, 1, 1)]);
        let cuelist_id = engine.create_cuelist("List".to_string(), CuelistKind::Standard, true);
        engine.set_current_cuelist(Some(cuelist_id));

        assert!(engine.execute_command("1 @ 100").ok);
        let result = engine.execute_command("RECORD CUE 1");
        assert!(result.ok, "{}", result.message);

        let lists = engine.list_cuelists();
        let cue = &lists[0].cues[0];
        assert_eq!(cue.number, 1.0);
        assert_eq!(cue.values[0].channel, 1);
        assert!(matches!(cue.values[0].value, CueValue::Literal { value } if value == 255.0));
    }

    #[test]
    fn unlink_cue_preset_freezes_the_presets_current_value() {
        let engine = EngineManager::new();
        engine.set_patch_index(vec![patched_fixture(1, 1, 1)]);
        let fixtures: HashSet<u32> = [1].into();
        let cuelist_id = engine.create_cuelist("List".to_string(), CuelistKind::Standard, true);
        engine.set_current_cuelist(Some(cuelist_id));

        engine.execute_command("1 @ 50");
        let preset_id = engine.record_preset(PresetFamily::Intensity, "Half".to_string(), &fixtures, None);
        assert!(engine.execute_command("RECORD CUE 1").ok);
        engine.set_cue_preset_reference(cuelist_id, 1.0, 1, "dimmer".to_string(), preset_id).unwrap();

        engine.unlink_cue_preset(cuelist_id, 1.0, 1, 1).unwrap();
        let half = match engine.list_cuelists()[0].cues[0].values[0].value {
            CueValue::Literal { value } => value,
            ref other => panic!("expected a literal after unlink, got {other:?}"),
        };

        // Changing the preset afterwards must not touch the unlinked cue.
        engine.execute_command("1 @ 100");
        engine.update_preset(preset_id, &fixtures, None).unwrap();
        assert!(matches!(
            engine.list_cuelists()[0].cues[0].values[0].value,
            CueValue::Literal { value } if value == half
        ));

        // A value that isn't linked can't be unlinked.
        assert!(engine.unlink_cue_preset(cuelist_id, 1.0, 1, 1).is_err());
    }

    #[test]
    fn record_cue_without_a_current_cuelist_fails() {
        let engine = EngineManager::new();
        engine.set_patch_index(vec![patched_fixture(1, 1, 1)]);
        let result = engine.execute_command("RECORD CUE 1");
        assert!(!result.ok);
    }

    #[test]
    fn update_re_captures_the_last_recorded_cue_and_delete_removes_it() {
        let engine = EngineManager::new();
        engine.set_patch_index(vec![patched_fixture(1, 1, 1)]);
        let cuelist_id = engine.create_cuelist("List".to_string(), CuelistKind::Standard, true);
        engine.set_current_cuelist(Some(cuelist_id));

        engine.execute_command("1 @ 50");
        assert!(engine.execute_command("RECORD CUE 1").ok);

        engine.execute_command("1 @ 100");
        let update = engine.execute_command("UPDATE");
        assert!(update.ok, "{}", update.message);
        let after_update = engine.list_cuelists();
        assert!(matches!(
            after_update[0].cues[0].values[0].value,
            CueValue::Literal { value } if value == 255.0
        ));

        let delete = engine.execute_command("DELETE CUE 1");
        assert!(delete.ok, "{}", delete.message);
        assert!(engine.list_cuelists()[0].cues.is_empty());
    }

    #[test]
    fn next_and_prev_step_the_playback_attached_to_the_current_cuelist() {
        let engine = EngineManager::new();
        engine.set_patch_index(vec![patched_fixture(1, 1, 1)]);
        let cuelist_id = engine.create_cuelist("List".to_string(), CuelistKind::Standard, true);
        let fixtures: HashSet<u32> = [1].into_iter().collect();
        engine.record_cue(cuelist_id, 1.0, "Cue 1".to_string(), &fixtures, 1.0, 1.0).unwrap();
        engine.record_cue(cuelist_id, 2.0, "Cue 2".to_string(), &fixtures, 1.0, 1.0).unwrap();
        engine.create_playback(cuelist_id).unwrap();
        engine.set_current_cuelist(Some(cuelist_id));

        assert!(engine.execute_command("NEXT").ok);
        assert_eq!(engine.list_playbacks()[0].current_cue_number, Some(1.0));

        assert!(engine.execute_command("NEXT").ok);
        assert_eq!(engine.list_playbacks()[0].current_cue_number, Some(2.0));

        assert!(engine.execute_command("PREV").ok);
        assert_eq!(engine.list_playbacks()[0].current_cue_number, Some(1.0));
    }

    #[test]
    fn next_without_an_attached_playback_fails_with_a_clear_message() {
        let engine = EngineManager::new();
        let cuelist_id = engine.create_cuelist("List".to_string(), CuelistKind::Standard, true);
        engine.set_current_cuelist(Some(cuelist_id));
        let result = engine.execute_command("NEXT");
        assert!(!result.ok);
    }

    // --- Phase 7: effects, tempo, pixel maps through the real tick ---

    /// The user's rig: 4 six-colour pars (dimmer + red at the start of each
    /// 10-channel block, addresses 1/11/21/31).
    fn four_pars() -> Vec<PatchIndexEntry> {
        (0..4u16)
            .map(|i| {
                let base = 1 + i * 10;
                PatchIndexEntry {
                    fixture_number: i as u32 + 1,
                    universe: 1,
                    intensity_channel: Some(base),
                    attribute_channels: [("dimmer", base), ("red", base + 1), ("green", base + 2), ("blue", base + 3)]
                        .into_iter()
                        .map(|(a, c)| (a.to_string(), c))
                        .collect(),
                    cells: Vec::new(),
                }
            })
            .collect()
    }

    fn reds(engine: &EngineManager) -> Vec<u8> {
        let frame = engine.get_universe_data(1);
        [2usize, 12, 22, 32].iter().map(|&ch| frame[ch - 1]).collect()
    }

    #[test]
    fn a_bpm_synced_colour_fan_wave_runs_across_the_four_pars() {
        use crate::engine::effects::{EffectDef, Waveform};
        let engine = EngineManager::new();
        engine.set_patch_index(four_pars());
        engine.set_bpm(120.0); // 500ms per beat
        let mut wave = EffectDef::new_attribute("red", Waveform::Sine);
        wave.spread = 360.0;
        engine.start_effect(wave, vec![1, 2, 3, 4], None);

        engine.step(125.0); // a quarter beat in
        let first = reds(&engine);
        // Fanned: no two pars are at the same point in the wave.
        let mut distinct = first.clone();
        distinct.dedup();
        assert_eq!(distinct.len(), 4, "{first:?}");

        // A quarter cycle later (one cycle per beat), each par has taken the
        // previous par's value: the wave travels along the selection.
        engine.step(125.0);
        let second = reds(&engine);
        assert_eq!(second[1], first[0], "{first:?} -> {second:?}");
        assert_eq!(second[2], first[1], "{first:?} -> {second:?}");

        // Doubling the tempo doubles the speed: a quarter cycle now takes
        // half the time.
        engine.set_bpm(240.0);
        engine.step(62.5);
        assert_eq!(reds(&engine)[1], second[0]);
    }

    #[test]
    fn clear_and_release_stop_programmer_effects() {
        use crate::engine::effects::{EffectDef, Waveform};
        let engine = EngineManager::new();
        engine.set_patch_index(four_pars());
        engine.start_effect(EffectDef::new_attribute("red", Waveform::Square), vec![1], None);
        engine.step(10.0);
        assert_eq!(reds(&engine)[0], 255);

        engine.execute_command("CLEAR"); // nothing selected: clears values + effects
        engine.step(10.0);
        assert_eq!(reds(&engine)[0], 0);
        assert!(engine.list_effects().is_empty());
    }

    #[test]
    fn a_new_effect_on_the_same_attribute_replaces_the_old_one_for_those_fixtures() {
        use crate::engine::effects::{EffectDef, Waveform};
        let engine = EngineManager::new();
        engine.set_patch_index(four_pars());
        engine.start_effect(EffectDef::new_attribute("red", Waveform::Sine), vec![1, 2], None);
        engine.start_effect(EffectDef::new_attribute("red", Waveform::Square), vec![2, 3], None);
        engine.start_effect(EffectDef::new_attribute("dimmer", Waveform::Sine), vec![1], None);
        let effects = engine.list_effects();
        assert_eq!(effects.len(), 3);
        assert_eq!(effects[0].fixtures, vec![1]); // gave up fixture 2
        assert_eq!(effects[1].fixtures, vec![2, 3]);
    }

    #[test]
    fn a_recorded_effect_plays_back_from_its_cue_and_follows_its_preset() {
        use crate::engine::effects::{EffectDef, EffectMode, Waveform};
        let engine = EngineManager::new();
        engine.set_patch_index(four_pars());
        let mut on = EffectDef::new_attribute("red", Waveform::Square);
        on.mode = EffectMode::Absolute;
        let preset = engine.record_effect_preset("Strobe red".to_string(), on.clone());
        let fixtures: HashSet<u32> = [1].into();
        engine.apply_preset(preset, &fixtures).unwrap();

        let list = engine.create_cuelist("FX".to_string(), CuelistKind::Standard, true);
        engine.record_cue(list, 1.0, "Cue 1".to_string(), &fixtures, 1.0, 1.0).unwrap();
        engine.execute_command("CLEAR");
        let pb = engine.create_playback(list).unwrap();
        engine.playback_go(pb).unwrap();
        engine.step(10.0); // square: high for the first half of the beat
        assert_eq!(reds(&engine)[0], 255);

        // Turn the preset's square into a constant-low one: the cue follows.
        let mut off = on;
        off.size = 0.0;
        off.center = 0.0;
        engine.update_effect_preset(preset, off).unwrap();
        engine.step(10.0);
        assert_eq!(reds(&engine)[0], 0);
    }

    #[test]
    fn sources_report_tracked_values_programmer_overrides_and_defaults() {
        let engine = EngineManager::new();
        engine.set_patch_index(vec![patched_fixture(1, 1, 1), patched_fixture(2, 1, 2), patched_fixture(3, 1, 3)]);
        let list = engine.create_cuelist("List".to_string(), CuelistKind::Standard, true);
        engine.execute_command("1 @ 50");
        engine.record_cue(list, 1.0, "One".to_string(), &[1].into(), 0.0, 0.0).unwrap();
        engine.execute_command("RELEASE");
        engine.execute_command("2 @ 50");
        engine.record_cue(list, 2.0, "Two".to_string(), &[2].into(), 0.0, 0.0).unwrap();
        engine.execute_command("RELEASE");
        let pb = engine.create_playback(list).unwrap();
        engine.playback_go(pb).unwrap();
        engine.playback_go(pb).unwrap();
        engine.step(10.0);

        engine.execute_command("3 @ 100");
        let sources = engine.get_universe_sources(1);
        assert_eq!(sources.len(), UNIVERSE_SIZE);
        assert_eq!(sources[0], 3, "fixture 1 is tracked from cue 1");
        assert_eq!(sources[1], 2, "fixture 2 is recorded in cue 2");
        assert_eq!(sources[2], 1, "fixture 3 is in the programmer");
        assert_eq!(sources[3], 0);
    }

    #[test]
    fn a_running_pixel_map_drives_fixtures_and_its_master_scales_their_dimmers() {
        let engine = EngineManager::new();
        engine.set_patch_index(four_pars());
        engine.set_htp_channels(1, vec![1, 11, 21, 31]);
        let id = engine.create_pixel_map("Row".to_string(), 4, 1);
        let mut map = engine.list_pixel_maps()[0].clone();
        map.cells = (0..4).map(|x| pixelmap::PixelCell { x, y: 0, fixture_number: x + 1, cell: 0 }).collect();
        engine.update_pixel_map(map).unwrap();

        engine.start_pixel_map(id).unwrap();
        engine.step(10.0);
        let frame = engine.get_universe_data(1);
        assert_eq!(frame[0], 255); // dimmer up so the colour shows
        assert!(reds(&engine).iter().any(|&r| r > 0)); // a rainbow has some red

        engine.set_pixel_map_intensity(id, 50.0).unwrap();
        engine.step(10.0);
        assert_eq!(engine.get_universe_data(1)[0], 128);
        assert_eq!(engine.pixel_map_preview(id).unwrap().len(), 4 * 3);

        engine.stop_pixel_map(id);
        engine.step(10.0);
        assert_eq!(engine.get_universe_data(1)[0], 0);
    }
}
