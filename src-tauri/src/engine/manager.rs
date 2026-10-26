//! Ties the engine's pieces together as Tauri-managed state: live merge
//! state, the compiled node graph and what it reads (clocks, audio, MIDI,
//! playback-bar controls), and the 44Hz tick thread.

use std::collections::HashMap;
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
use crate::graph::compile::{GraphPlan, Val};
use crate::engine::midi::{MidiEvent, MidiState};
use crate::graph::eval::{GraphRuntime, Inputs};
use crate::output::manager::OutputManager;
use crate::output::UNIVERSE_SIZE;
use crate::show::resolve::ResolvedPatch;

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

/// Live values the editor shows on each node's outputs, by node id.
pub type GraphValues = HashMap<String, Vec<Val>>;

/// Everything one engine tick reads and writes — shared (by `Arc`) between
/// `EngineManager` and its tick thread, and steppable directly in tests.
#[derive(Clone)]
struct TickShared {
    state: Arc<Mutex<EngineState>>,
    tempo: Arc<Mutex<Tempo>>,
    timecode: Arc<Mutex<TimecodeClock>>,
    audio: Arc<Mutex<BandLevels>>,
    midi: Arc<Mutex<MidiState>>,
    controls: Arc<Mutex<HashMap<String, f32>>>,
    plan: Arc<Mutex<Arc<GraphPlan>>>,
    runtime: Arc<Mutex<GraphRuntime>>,
    /// The show's beat offset: the graph sees the beat this far ahead.
    beat_offset_ms: Arc<Mutex<f32>>,
}

impl TickShared {
    /// Advances the clocks by `dt_ms` and re-evaluates the graph into its
    /// output layers. Output is the caller's job: merge and send.
    fn advance(&self, dt_ms: f64) {
        let (prev_beats, beats, seconds, bpm) = {
            let mut tempo = self.tempo.lock().unwrap();
            let prev = tempo.beat();
            tempo.advance(dt_ms);
            // Only the graph's view of the beat moves; tap and sync keep the real one.
            let shift = *self.beat_offset_ms.lock().unwrap() as f64 / 60_000.0 * tempo.bpm() as f64;
            (prev + shift, tempo.beat() + shift, tempo.seconds(), tempo.bpm())
        };
        let (timecode_seconds, timecode_running) = {
            let mut tc = self.timecode.lock().unwrap();
            tc.advance(dt_ms);
            (tc.position_ms() / 1000.0, tc.active())
        };
        let audio = *self.audio.lock().unwrap();
        let plan = self.plan.lock().unwrap().clone();
        // MIDI-pressed Flash buttons count as on for this tick even if a
        // quick pad hit was released already. Lock order: midi, controls.
        let pressed = self.midi.lock().unwrap().take_pressed();
        let live = self.controls.lock().unwrap();
        let latched;
        let controls = if pressed.is_empty() {
            &*live
        } else {
            let mut c = live.clone();
            for id in &pressed {
                c.insert(id.to_string(), 1.0);
            }
            latched = c;
            &latched
        };
        let inputs = Inputs {
            beats,
            prev_beats,
            seconds,
            bpm,
            dt_ms,
            audio,
            controls,
            timecode_seconds,
            timecode_running,
        };
        let mut runtime = self.runtime.lock().unwrap();
        let mut state = self.state.lock().unwrap();
        runtime.run(&plan, &inputs, &mut state.layers);
    }

    fn graph_values(&self) -> GraphValues {
        self.runtime.lock().unwrap().preview().iter().map(|(id, v)| (id.to_string(), v.clone())).collect()
    }
}

pub struct EngineManager {
    shared: TickShared,
    tick_started: AtomicBool,
    /// Shared with `OutputManager`, which zeroes the wire; here it zeroes
    /// what the views and MCP read, so they show what's actually sent.
    blackout: Arc<AtomicBool>,
}

/// What `crate::timing` needs from the engine to feed it.
#[derive(Clone)]
pub struct TimingHandles {
    pub tempo: Arc<Mutex<Tempo>>,
    pub timecode: Arc<Mutex<TimecodeClock>>,
    pub audio: Arc<Mutex<BandLevels>>,
    pub midi: Arc<Mutex<MidiState>>,
    /// Playback-bar values, which bound MIDI moves.
    pub controls: Arc<Mutex<HashMap<String, f32>>>,
}

impl Default for EngineManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineManager {
    pub fn new() -> Self {
        Self {
            shared: TickShared {
                state: Arc::new(Mutex::new(EngineState::new())),
                tempo: Arc::new(Mutex::new(Tempo::new())),
                timecode: Arc::new(Mutex::new(TimecodeClock::new())),
                audio: Arc::new(Mutex::new(BandLevels::default())),
                midi: Arc::new(Mutex::new(MidiState::default())),
                controls: Arc::new(Mutex::new(HashMap::new())),
                plan: Arc::new(Mutex::new(Arc::new(GraphPlan::default()))),
                runtime: Arc::new(Mutex::new(GraphRuntime::default())),
                beat_offset_ms: Arc::new(Mutex::new(0.0)),
            },
            tick_started: AtomicBool::new(false),
            blackout: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The blackout flag, for `OutputManager` to share.
    pub fn blackout_flag(&self) -> Arc<AtomicBool> {
        self.blackout.clone()
    }

    pub fn timing_handles(&self) -> TimingHandles {
        TimingHandles {
            tempo: self.shared.tempo.clone(),
            timecode: self.shared.timecode.clone(),
            audio: self.shared.audio.clone(),
            midi: self.shared.midi.clone(),
            controls: self.shared.controls.clone(),
        }
    }

    /// One tick without the output half — for tests.
    #[cfg(test)]
    pub fn step(&self, dt_ms: f64) {
        self.shared.advance(dt_ms);
    }

    /// Starts the 44Hz tick thread (idempotent — safe to call from `setup`
    /// every launch).
    pub fn start_tick(&self, app: AppHandle) {
        if self.tick_started.swap(true, Ordering::SeqCst) {
            return;
        }

        let shared = self.shared.clone();
        let state = self.shared.state.clone();
        let blackout = self.blackout.clone();

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
                        let channels = if blackout.load(Ordering::Relaxed) { vec![0; UNIVERSE_SIZE] } else { frame.to_vec() };
                        let _ = app.emit(
                            "engine://universe-frame",
                            EngineFrame { universe_id, channels },
                        );
                    }
                }
                if show_display {
                    let _ = app.emit("graph://values", shared.graph_values());
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
    pub fn set_patch(&self, patch: &ResolvedPatch) {
        let mut state = self.shared.state.lock().unwrap();
        state.htp_channels = patch.htp.clone();
        let defaults = Layer { values: patch.defaults.clone(), ..Layer::default() };
        state.layers.insert(DEFAULTS_LAYER.to_string(), (DEFAULTS_PRIORITY, defaults));
    }

    /// Swaps in a newly compiled graph; it runs from the next tick.
    pub fn set_graph(&self, plan: Arc<GraphPlan>) {
        self.shared.midi.lock().unwrap().bindings = plan.midi_bindings.clone();
        *self.shared.plan.lock().unwrap() = plan;
    }

    pub fn get_universe_data(&self, universe: u32) -> [u8; UNIVERSE_SIZE] {
        if self.blackout.load(Ordering::Relaxed) {
            return [0; UNIVERSE_SIZE];
        }
        let state = self.shared.state.lock().unwrap();
        merge_universe(&state, universe)
    }

    pub fn set_grand_master(&self, percent: f32) {
        self.shared.state.lock().unwrap().grand_master = percent.clamp(0.0, 100.0) / 100.0;
    }

    pub fn grand_master(&self) -> f32 {
        self.shared.state.lock().unwrap().grand_master * 100.0
    }

    // --- Playback bar controls and MIDI ---

    /// A Fader's or Button's live value (0-1), by node id.
    pub fn set_control(&self, node_id: &str, value: f32) {
        self.shared.controls.lock().unwrap().insert(node_id.to_string(), value.clamp(0.0, 1.0));
    }

    pub fn controls(&self) -> HashMap<String, f32> {
        self.shared.controls.lock().unwrap().clone()
    }

    pub fn set_controls(&self, controls: HashMap<String, f32>) {
        *self.shared.controls.lock().unwrap() = controls;
    }

    /// MIDI channel messages received after `after` (a `seq`), for the
    /// Setup MIDI monitor.
    pub fn midi_events(&self, after: u64) -> Vec<MidiEvent> {
        self.shared.midi.lock().unwrap().since(after)
    }

    // --- Tempo ---

    pub fn tempo_status(&self) -> TempoStatus {
        self.shared.tempo.lock().unwrap().status()
    }

    pub fn bpm(&self) -> f32 {
        self.shared.tempo.lock().unwrap().bpm()
    }

    pub fn set_bpm(&self, bpm: f32) {
        self.shared.tempo.lock().unwrap().set_bpm(bpm);
    }

    pub fn set_beat_offset(&self, ms: f32) {
        *self.shared.beat_offset_ms.lock().unwrap() = ms;
    }

    /// `now_ms`: the caller's timestamp for the tap (the frontend's
    /// `performance.now()`), so network/IPC jitter doesn't skew the
    /// measured interval.
    pub fn tap_tempo(&self, now_ms: f64) -> f32 {
        self.shared.tempo.lock().unwrap().tap(now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::compile::compile;
    use crate::graph::model::{Edge, Graph, Node, NodePosition, ParamValue};
    use crate::graph::registry::NodeKind;
    use crate::show::library::test_fixtures::library;
    use crate::show::resolve::resolve;
    use crate::showfile::schema::PatchedFixture;

    fn node(id: &str, kind: NodeKind, params: &[(&str, ParamValue)]) -> Node {
        Node { id: id.into(), kind, position: NodePosition::default(), params: params.iter().map(|(k, v)| (k.to_string(), v.clone())).collect() }
    }

    fn edge(from: &str, fs: &str, to: &str, ts: &str) -> Edge {
        Edge { from_node: from.into(), from_socket: fs.into(), to_node: to.into(), to_socket: ts.into() }
    }

    fn par() -> PatchedFixture {
        PatchedFixture {
            id: "p".into(),
            fixture_id: "par".into(),
            mode_index: 0,
            address: 1,
            name: "Par".into(),
            universe: 1,
            fixture_number: 1,
            invert_pan: false,
            invert_tilt: false,
            swap_pan_tilt: false,
        }
    }

    #[test]
    fn the_beat_offset_shifts_what_the_graph_sees_of_the_beat() {
        let engine = EngineManager::new();
        let patch = resolve(&[par()], &library());
        engine.set_patch(&patch);
        let graph = Graph {
            nodes: vec![node("time", NodeKind::Time, &[]), node("out", NodeKind::Output, &[("fixtures", ParamValue::Text("1".into()))])],
            edges: vec![edge("time", "beat", "out", "intensity")],
        };
        let (plan, _) = compile(&graph, &patch, &[]);
        engine.set_graph(Arc::new(plan));

        // 120 BPM: 250ms is half a beat ahead.
        engine.set_beat_offset(250.0);
        engine.step(0.0);
        assert_eq!(engine.get_universe_data(1)[0], 128);
        engine.set_beat_offset(-250.0);
        engine.step(0.0);
        assert_eq!(engine.get_universe_data(1)[0], 128); // -0.5 wraps to the same half beat
        assert_eq!(engine.tempo_status().beat, 0.0); // the real beat doesn't move
    }

    #[test]
    fn patch_and_graph_reach_the_dmx_frame_with_htp_and_grand_master() {
        let engine = EngineManager::new();
        let patch = resolve(&[par()], &library());
        engine.set_patch(&patch);
        let fixtures = ParamValue::Text("1".into());
        let graph = Graph {
            nodes: vec![
                node("half", NodeKind::Number, &[("value", ParamValue::Number(0.5))]),
                node("fader", NodeKind::Fader, &[]),
                node("red", NodeKind::Color, &[("color", ParamValue::Text("#ff0000".into()))]),
                node("a", NodeKind::Output, &[("fixtures", fixtures.clone())]),
                node("b", NodeKind::Output, &[("fixtures", fixtures), ("priority", ParamValue::Number(5.0))]),
            ],
            edges: vec![
                edge("half", "value", "a", "intensity"),
                edge("fader", "value", "b", "intensity"),
                edge("red", "color", "b", "color"),
            ],
        };
        let (plan, issues) = compile(&graph, &patch, &[]);
        assert!(issues.is_empty(), "{issues:?}");
        engine.set_graph(Arc::new(plan));

        engine.step(10.0);
        let frame = engine.get_universe_data(1);
        assert_eq!(frame[0], 128); // dimmer: HTP of 0.5 and the fader at 0
        assert_eq!(frame[1], 255); // red from the higher-priority output
        assert_eq!(frame[5], 10); // the function channel's rest value

        engine.set_control("fader", 1.0);
        engine.set_grand_master(50.0);
        engine.step(10.0);
        assert_eq!(engine.get_universe_data(1)[0], 128); // fader wins HTP, then halved by the GM

        // Blackout reads as all zeros, and releasing it brings the show back.
        engine.blackout_flag().store(true, Ordering::Relaxed);
        assert!(engine.get_universe_data(1).iter().all(|&v| v == 0));
        engine.blackout_flag().store(false, Ordering::Relaxed);
        assert_eq!(engine.get_universe_data(1)[0], 128);
    }
}
