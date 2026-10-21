//! Runs a `GraphPlan` once per engine tick. Nodes that don't depend on the
//! fixture are evaluated once; varying ones once per Output target, with the
//! target's place in the list as context. Each Output node's values go
//! through `write::write_item` into its own engine layer.
//!
//! Stateful nodes (Smooth, Envelope, Toggle, Counter, Random) keep their
//! state by node id and fixture, so it survives recompiles: editing one node
//! doesn't reset a smoothing or a counter elsewhere.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use serde::Serialize;
use ts_rs::TS;

use super::compile::{CompareOp, Config, Conversion, GraphPlan, MathOp, Rate, Source, Val, Write};
use super::registry::{NodeKind, MAX_OUTPUTS};
use super::write::{hsv_to_rgb, write_item, ItemValues};
use crate::engine::audio::BandLevels;
use crate::engine::state::Layer;
use crate::engine::waves::{fan_position, shape_value, waveform_value, Waveform};

const LAYER_PREFIX: &str = "graph-out-";

type Outs = [Val; MAX_OUTPUTS];
const ZERO: Outs = [[0.0; 3]; MAX_OUTPUTS];

/// How many recent messages the MIDI monitor keeps.
const MIDI_LOG_LEN: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum MidiEventKind {
    Cc,
    NoteOn,
    NoteOff,
    PitchBend,
    ProgramChange,
    Aftertouch,
}

/// A channel message, as the Setup MIDI monitor lists it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct MidiEvent {
    /// Counts up from 1; pass the last one seen back to get only newer ones.
    #[ts(type = "number")]
    pub seq: u64,
    /// The id of the port it came in on.
    pub port: String,
    pub kind: MidiEventKind,
    /// 1-16.
    pub channel: u8,
    /// Controller or note number; 0 for pitch bend and channel aftertouch.
    pub number: u8,
    /// 0-127, or 0-16383 for pitch bend.
    pub value: u16,
}

/// Latest value of every MIDI controller and note, per channel (0-15).
#[derive(Debug, Clone)]
pub struct MidiState {
    pub cc: [[f32; 128]; 16],
    /// Velocity of each held note, 0 when released.
    pub notes: [[f32; 128]; 16],
    /// The last controller or note-on received: (is_note, channel 1-16,
    /// number), for MIDI learn.
    pub last: Option<(bool, u8, u8)>,
    /// Recent channel messages, oldest first.
    pub log: VecDeque<MidiEvent>,
    seq: u64,
}

impl Default for MidiState {
    fn default() -> Self {
        Self { cc: [[0.0; 128]; 16], notes: [[0.0; 128]; 16], last: None, log: VecDeque::new(), seq: 0 }
    }
}

impl MidiState {
    /// A message from the MIDI port `port` (its id).
    pub fn feed(&mut self, port: &str, msg: &[u8]) {
        self.record(port, msg);
        let [status, number, value] = match msg {
            [s, n, v, ..] => [*s, *n & 0x7f, *v & 0x7f],
            _ => return,
        };
        let channel = (status & 0x0f) as usize;
        match status & 0xf0 {
            0xb0 => {
                self.cc[channel][number as usize] = value as f32 / 127.0;
                self.last = Some((false, channel as u8 + 1, number));
            }
            0x90 if value > 0 => {
                self.notes[channel][number as usize] = value as f32 / 127.0;
                self.last = Some((true, channel as u8 + 1, number));
            }
            0x80 | 0x90 => self.notes[channel][number as usize] = 0.0,
            _ => {}
        }
    }

    /// Logs channel messages; clock, timecode and other system messages
    /// would drown them out.
    fn record(&mut self, port: &str, msg: &[u8]) {
        let (Some(&status), data) = (msg.first(), &msg[msg.len().min(1)..]) else { return };
        let d = |i: usize| data.get(i).map_or(0, |b| b & 0x7f);
        let (kind, number, value) = match status & 0xf0 {
            0x80 => (MidiEventKind::NoteOff, d(0), d(1) as u16),
            0x90 if d(1) == 0 => (MidiEventKind::NoteOff, d(0), 0),
            0x90 => (MidiEventKind::NoteOn, d(0), d(1) as u16),
            0xa0 => (MidiEventKind::Aftertouch, d(0), d(1) as u16),
            0xb0 => (MidiEventKind::Cc, d(0), d(1) as u16),
            0xc0 => (MidiEventKind::ProgramChange, d(0), 0),
            0xd0 => (MidiEventKind::Aftertouch, 0, d(0) as u16),
            0xe0 => (MidiEventKind::PitchBend, 0, d(0) as u16 | (d(1) as u16) << 7),
            _ => return,
        };
        self.seq += 1;
        if self.log.len() == MIDI_LOG_LEN {
            self.log.pop_front();
        }
        self.log.push_back(MidiEvent { seq: self.seq, port: port.to_string(), kind, channel: (status & 0x0f) + 1, number, value });
    }

    /// Logged messages newer than `after`.
    pub fn since(&self, after: u64) -> Vec<MidiEvent> {
        self.log.iter().filter(|e| e.seq > after).cloned().collect()
    }

    /// `channel` 1-16, or 0 for whichever channel is highest.
    fn read(table: &[[f32; 128]; 16], channel: u8, number: u8) -> f32 {
        let n = number.min(127) as usize;
        match channel {
            1..=16 => table[channel as usize - 1][n],
            _ => table.iter().map(|ch| ch[n]).fold(0.0, f32::max),
        }
    }
}

/// Everything outside the graph that nodes read, for one tick.
pub struct Inputs<'a> {
    pub beats: f64,
    pub prev_beats: f64,
    pub seconds: f64,
    pub bpm: f32,
    pub dt_ms: f64,
    pub audio: BandLevels,
    pub midi: &'a MidiState,
    /// Fader and Button values by node id.
    pub controls: &'a HashMap<String, f32>,
    pub timecode_seconds: f64,
    pub timecode_running: bool,
}

#[derive(Debug, Clone, Copy)]
struct Context {
    index: usize,
    count: usize,
    /// Fixture (and cell) identity, for per-fixture state and randomness.
    key: u64,
}

const UNIFORM: Context = Context { index: 0, count: 1, key: 0 };

#[derive(Debug, Clone, Copy, Default)]
struct NodeState {
    tick: u64,
    out: Outs,
    mem: [f32; 3],
    started: bool,
}

#[derive(Default)]
pub struct GraphRuntime {
    tick: u64,
    uniform: Vec<Outs>,
    scratch: Vec<Outs>,
    state: HashMap<(Arc<str>, u64), NodeState>,
    /// Each node's outputs this tick (for varying nodes, at the first
    /// target that evaluated them), for the editor's live previews.
    preview: HashMap<Arc<str>, Vec<Val>>,
}

fn rate_cycles(rate: Rate, inputs: &Inputs) -> f64 {
    match rate {
        Rate::Beats(r) => inputs.beats * r as f64,
        Rate::Hz(r) => inputs.seconds * r as f64,
    }
}

fn hash_unit(a: u64, b: u64) -> f32 {
    let mut x = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x ^= x >> 33;
    (x as f64 / u64::MAX as f64) as f32
}

fn lerp(a: Val, b: Val, t: f32) -> Val {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn rising(prev: f32, now: f32) -> bool {
    prev <= 0.5 && now > 0.5
}

fn bool_val(b: bool) -> Val {
    [if b { 1.0 } else { 0.0 }, 0.0, 0.0]
}

impl GraphRuntime {
    pub fn preview(&self) -> &HashMap<Arc<str>, Vec<Val>> {
        &self.preview
    }

    /// The node's Enabled input (always its last) is on.
    fn enabled(&self, plan: &GraphPlan, node: usize) -> bool {
        plan.nodes[node].inputs.last().is_none_or(|s| self.read(plan, s)[0] > 0.5)
    }

    fn read(&self, plan: &GraphPlan, source: &Source) -> Val {
        match source {
            Source::Const(v) => *v,
            Source::Link { node, fallback, .. } if !self.enabled(plan, *node) => *fallback,
            Source::Link { node, socket, conversion, .. } => {
                let outs = if plan.nodes[*node].varying { &self.scratch[*node] } else { &self.uniform[*node] };
                let v = outs[*socket];
                match conversion {
                    Conversion::None => v,
                    Conversion::Spread => [v[0]; 3],
                    Conversion::Brightness => [v[0].max(v[1]).max(v[2]), 0.0, 0.0],
                }
            }
        }
    }

    fn compute(&mut self, plan: &GraphPlan, idx: usize, ctx: Context, inputs: &Inputs) -> Outs {
        let node = &plan.nodes[idx];
        let input = |s: usize| self.read(plan, &node.inputs[s]);
        let num = |s: usize| input(s)[0];
        let mut out = ZERO;
        match (&node.kind, &node.config) {
            (NodeKind::Number | NodeKind::Color, Config::Value(v)) => out[0] = *v,
            (NodeKind::Time, _) => {
                // rem_euclid, not fract: a negative beat offset can put the beat below 0.
                out[0][0] = inputs.beats.rem_euclid(1.0) as f32;
                out[1][0] = (inputs.beats / 4.0).rem_euclid(1.0) as f32;
                out[2] = bool_val(inputs.beats.floor() != inputs.prev_beats.floor());
                out[3][0] = inputs.bpm;
                out[4][0] = inputs.seconds as f32;
            }
            (NodeKind::Lfo, Config::Lfo { waveform, rate, width, curve }) => {
                let cycles = rate_cycles(*rate, inputs) + num(0) as f64;
                let cycle = cycles.floor();
                let p = cycles - cycle;
                out[0][0] = match waveform {
                    Waveform::Custom => curve.value(p),
                    _ => (waveform_value(*waveform, p, *width, cycle as i64, ctx.key) + 1.0) / 2.0,
                };
                // Not a socket: where in the cycle it is, for the curve editor's playhead.
                out[1][0] = p as f32;
            }
            (NodeKind::Audio, Config::Gain(gain)) => {
                let a = inputs.audio;
                for (i, level) in [a.low, a.mid, a.high, a.loudness].into_iter().enumerate() {
                    out[i][0] = (level * gain).clamp(0.0, 1.0);
                }
            }
            (NodeKind::MidiCc, Config::Midi { channel, number }) => out[0][0] = MidiState::read(&inputs.midi.cc, *channel, *number),
            (NodeKind::MidiNote, Config::Midi { channel, number }) => {
                let velocity = MidiState::read(&inputs.midi.notes, *channel, *number);
                out[0] = bool_val(velocity > 0.0);
                out[1][0] = velocity;
            }
            (NodeKind::Fader | NodeKind::Button, _) => out[0][0] = inputs.controls.get(&*node.id).copied().unwrap_or(0.0),
            (NodeKind::Timecode, _) => {
                out[0][0] = inputs.timecode_seconds as f32;
                out[1] = bool_val(inputs.timecode_running);
            }
            (NodeKind::FixtureIndex, _) => {
                out[0][0] = ctx.index as f32;
                out[1][0] = if ctx.count > 1 { ctx.index as f32 / (ctx.count - 1) as f32 } else { 0.0 };
                out[2][0] = ctx.count as f32;
            }
            (NodeKind::Fan, Config::Fan(fan)) => {
                let (position, positions) = fan_position(*fan, ctx.index, ctx.count);
                out[0][0] = -num(0) * position as f32 / positions as f32;
            }
            (NodeKind::Math, Config::Math { op, clamp }) => {
                let (a, b) = (num(0), num(1));
                let v = match op {
                    MathOp::Add => a + b,
                    MathOp::Subtract => a - b,
                    MathOp::Multiply => a * b,
                    MathOp::Divide => if b == 0.0 { 0.0 } else { a / b },
                    MathOp::Min => a.min(b),
                    MathOp::Max => a.max(b),
                    MathOp::Power => a.powf(b),
                    MathOp::Modulo => if b == 0.0 { 0.0 } else { a.rem_euclid(b) },
                    MathOp::Invert => 1.0 - a,
                    MathOp::Abs => a.abs(),
                };
                let v = if v.is_finite() { v } else { 0.0 };
                out[0][0] = if *clamp { v.clamp(0.0, 1.0) } else { v };
            }
            (NodeKind::MapRange, Config::MapRange { from, to, clamp }) => {
                let span = from.1 - from.0;
                let t = if span == 0.0 { 0.0 } else { (num(0) - from.0) / span };
                let t = if *clamp { t.clamp(0.0, 1.0) } else { t };
                out[0][0] = to.0 + t * (to.1 - to.0);
            }
            (NodeKind::Compare, Config::Compare(op)) => {
                let (a, b) = (num(0), num(1));
                out[0] = bool_val(match op {
                    CompareOp::Greater => a > b,
                    CompareOp::Less => a < b,
                    CompareOp::Equal => (a - b).abs() < 1e-4,
                });
            }
            (NodeKind::Rgb, _) => out[0] = [num(0), num(1), num(2)],
            (NodeKind::Hsv, _) => out[0] = hsv_to_rgb(num(0), num(1), num(2)),
            (NodeKind::MixColor, _) => out[0] = lerp(input(0), input(1), num(2).clamp(0.0, 1.0)),
            (NodeKind::Gradient, Config::Gradient { stops, wrap }) => {
                out[0] = match stops.len() {
                    0 => [0.0; 3],
                    1 => stops[0],
                    n => {
                        let t = num(0);
                        let (segments, t) = if *wrap { (n, t.rem_euclid(1.0)) } else { (n - 1, t.clamp(0.0, 1.0)) };
                        let x = t * segments as f32;
                        let i = (x.floor() as usize).min(segments - 1);
                        lerp(stops[i], stops[(i + 1) % n], x - i as f32)
                    }
                };
            }
            (NodeKind::Shape, Config::Shape { shape, rate }) => {
                let cycles = rate_cycles(*rate, inputs) + num(0) as f64;
                let (dx, dy) = shape_value(*shape, cycles.rem_euclid(1.0));
                let (size, center) = (num(1), input(2));
                out[0] = [(center[0] + size * dx).clamp(0.0, 1.0), (center[1] + size * dy).clamp(0.0, 1.0), 0.0];
            }
            (NodeKind::PanTilt, _) => out[0] = [num(0), num(1), 0.0],
            (NodeKind::Switch, Config::Count(count)) => {
                let pick = (num(0).round().max(0.0) as usize).min(count - 1);
                out[0] = input(1 + pick);
            }
            (NodeKind::Any, Config::Count(count)) => out[0] = bool_val((0..*count).any(|i| num(i) > 0.5)),
            (NodeKind::Mix, _) => out[0] = lerp(input(0), input(1), num(2).clamp(0.0, 1.0)),
            (NodeKind::Random | NodeKind::Smooth | NodeKind::Envelope | NodeKind::Toggle | NodeKind::Counter, _) => {
                return self.stateful(plan, idx, ctx, inputs);
            }
            _ => {}
        }
        out
    }

    fn stateful(&mut self, plan: &GraphPlan, idx: usize, ctx: Context, inputs: &Inputs) -> Outs {
        let node = &plan.nodes[idx];
        let a = self.read(plan, node.inputs.first().unwrap_or(&Source::Const([0.0; 3])))[0];
        let b = node.inputs.get(1).map_or(0.0, |s| self.read(plan, s)[0]);
        let tick = self.tick;
        let state = self.state.entry((node.id.clone(), ctx.key)).or_default();
        if state.tick == tick && state.started {
            // Already advanced this tick (another Output shares it).
            return state.out;
        }
        let dt = inputs.dt_ms as f32;
        let first = !state.started;
        let m = &mut state.mem;
        let mut out = ZERO;
        match &node.config {
            Config::Seed(seed) => {
                if first || rising(m[0], a) {
                    m[1] = hash_unit(seed.wrapping_add(ctx.key), tick ^ (m[2] as u64));
                    m[2] += 1.0;
                }
                m[0] = a;
                out[0][0] = m[1];
            }
            Config::Times { rise_ms, fall_ms } if node.kind == NodeKind::Smooth => {
                if first {
                    m[0] = a;
                }
                let tau = if a > m[0] { *rise_ms } else { *fall_ms };
                let alpha = if tau <= 0.0 { 1.0 } else { 1.0 - (-dt / tau).exp() };
                m[0] += (a - m[0]) * alpha;
                out[0][0] = m[0];
            }
            Config::Times { rise_ms, fall_ms } => {
                // Envelope: m[0] last trigger, m[1] ms since it fired.
                if first {
                    m[1] = f32::INFINITY;
                }
                if rising(m[0], a) {
                    m[1] = 0.0;
                } else {
                    m[1] += dt;
                }
                m[0] = a;
                let t = m[1];
                out[0][0] = if t < *rise_ms {
                    t / rise_ms.max(1e-3)
                } else if *fall_ms <= 0.0 {
                    0.0
                } else {
                    (1.0 - (t - rise_ms) / fall_ms).max(0.0)
                };
            }
            Config::Steps(steps) => {
                // Counter: m[0] last trigger, m[1] last reset, m[2] count.
                if rising(m[1], b) {
                    m[2] = 0.0;
                } else if rising(m[0], a) {
                    m[2] = ((m[2] as u32 + 1) % steps) as f32;
                }
                m[0] = a;
                m[1] = b;
                out[0][0] = m[2];
                out[1][0] = if *steps > 1 { m[2] / (*steps - 1) as f32 } else { 0.0 };
            }
            _ => {
                // Toggle: m[0] last trigger, m[1] state.
                if rising(m[0], a) {
                    m[1] = 1.0 - m[1];
                }
                m[0] = a;
                out[0] = bool_val(m[1] > 0.5);
            }
        }
        state.started = true;
        state.tick = tick;
        state.out = out;
        out
    }

    fn record_preview(&mut self, plan: &GraphPlan, idx: usize, outs: &Outs) {
        let id = plan.nodes[idx].id.clone();
        self.preview.entry(id).or_insert_with(|| outs.to_vec());
    }

    /// Evaluates the graph and rewrites every Output node's layer.
    pub fn run(&mut self, plan: &GraphPlan, inputs: &Inputs, layers: &mut HashMap<String, (i64, Layer)>) {
        self.tick += 1;
        self.preview.clear();
        let count = plan.nodes.len();
        self.uniform.resize(count, ZERO);
        self.scratch.resize(count, ZERO);

        for &idx in &plan.uniform_order {
            let outs = self.compute(plan, idx, UNIFORM, inputs);
            self.uniform[idx] = outs;
            self.record_preview(plan, idx, &outs);
        }

        let mut live = Vec::with_capacity(plan.outputs.len());
        for output in &plan.outputs {
            let key = format!("{LAYER_PREFIX}{}", output.id);
            let mut values = std::mem::take(&mut layers.entry(key.clone()).or_insert_with(|| (0, Layer::default())).1.values);
            values.clear();
            let n = output.targets.len();
            for (i, item) in output.targets.iter().enumerate() {
                let item_key = ((item.fixture_number as u64) << 16) | item.cell.map_or(0xffff, |c| c as u64);
                let ctx = Context { index: i, count: n, key: item_key };
                for &dep in &output.varying_deps {
                    let outs = self.compute(plan, dep, ctx, inputs);
                    self.scratch[dep] = outs;
                    if i == 0 {
                        self.record_preview(plan, dep, &outs);
                    }
                }
                let node = &plan.nodes[output.node];
                if !self.enabled(plan, output.node) {
                    continue;
                }
                let mut item_values = ItemValues::default();
                for (write, input) in &output.writes {
                    // A disabled source counts as unconnected: nothing sent.
                    if let Source::Link { node: from, .. } = node.inputs[*input] {
                        if !self.enabled(plan, from) {
                            continue;
                        }
                    }
                    let v = self.read(plan, &node.inputs[*input]);
                    match write {
                        Write::Intensity => item_values.intensity = Some(v[0]),
                        Write::Color => item_values.color = Some(v),
                        Write::Position => item_values.position = Some([v[0], v[1]]),
                        Write::Strobe => item_values.strobe = Some(v[0]),
                        Write::Attribute(name) => item_values.extras.push((name.as_str(), v[0])),
                    }
                }
                write_item(item, &item_values, &mut values);
            }
            let layer = layers.get_mut(&key).expect("inserted above");
            layer.0 = output.priority;
            layer.1.values = values;
            live.push(key);
        }
        layers.retain(|k, _| !k.starts_with(LAYER_PREFIX) || live.contains(k));

        // Forget state of nodes that no longer exist.
        if self.tick % 256 == 0 {
            let ids: std::collections::HashSet<&str> = plan.nodes.iter().map(|n| &*n.id).collect();
            self.state.retain(|(id, _), _| ids.contains(&**id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::compile::compile;
    use crate::graph::model::{Edge, Graph, Node, NodePosition, ParamValue};
    use crate::show::library::test_fixtures::library;
    use crate::show::resolve::resolve;
    use crate::showfile::schema::{Group, PatchedFixture};
    use std::collections::BTreeMap;

    fn node(id: &str, kind: NodeKind, params: &[(&str, ParamValue)]) -> Node {
        Node {
            id: id.to_string(),
            kind,
            position: NodePosition::default(),
            params: params.iter().map(|(k, v)| (k.to_string(), v.clone())).collect::<BTreeMap<_, _>>(),
        }
    }

    fn edge(from: &str, fs: &str, to: &str, ts: &str) -> Edge {
        Edge { from_node: from.into(), from_socket: fs.into(), to_node: to.into(), to_socket: ts.into() }
    }

    fn pars(count: u32) -> Vec<PatchedFixture> {
        (1..=count)
            .map(|n| PatchedFixture {
                id: format!("f{n}"),
                fixture_id: "par".into(),
                mode_index: 0,
                address: 1 + (n as u16 - 1) * 6,
                name: format!("Par {n}"),
                universe: 1,
                fixture_number: n,
                invert_pan: false,
                invert_tilt: false,
                swap_pan_tilt: false,
            })
            .collect()
    }

    struct Rig {
        plan: GraphPlan,
        runtime: GraphRuntime,
        layers: HashMap<String, (i64, Layer)>,
        midi: MidiState,
        controls: HashMap<String, f32>,
        beats: f64,
    }

    impl Rig {
        fn new(graph: &Graph, fixtures: u32) -> Self {
            let patch = resolve(&pars(fixtures), &library());
            let groups = vec![Group { id: 1, name: "All".into(), fixture_numbers: (1..=fixtures).collect() }];
            let (plan, issues) = compile(graph, &patch, &groups);
            assert!(issues.is_empty(), "{issues:?}");
            Self { plan, runtime: GraphRuntime::default(), layers: HashMap::new(), midi: MidiState::default(), controls: HashMap::new(), beats: 0.0 }
        }

        /// Advances to `beats` (at 120 BPM) and runs a tick.
        fn at(&mut self, beats: f64) -> &HashMap<String, (i64, Layer)> {
            let dt_ms = (beats - self.beats).max(0.0) * 500.0;
            let inputs = Inputs {
                beats,
                prev_beats: self.beats,
                seconds: beats / 2.0,
                bpm: 120.0,
                dt_ms,
                audio: BandLevels::default(),
                midi: &self.midi,
                controls: &self.controls,
                timecode_seconds: 0.0,
                timecode_running: false,
            };
            self.runtime.run(&self.plan, &inputs, &mut self.layers);
            self.beats = beats;
            &self.layers
        }

        fn channel(&self, output: &str, channel: u16) -> f32 {
            self.layers[&format!("graph-out-{output}")].1.values.get(&(1, channel)).copied().unwrap_or(-1.0)
        }
    }

    fn output(id: &str) -> Node {
        node(id, NodeKind::Output, &[("group", ParamValue::Number(1.0))])
    }

    #[test]
    fn an_lfo_fanned_across_four_pars_makes_a_travelling_wave() {
        let graph = Graph {
            nodes: vec![
                node("fan", NodeKind::Fan, &[]),
                node("lfo", NodeKind::Lfo, &[("waveform", ParamValue::Text("ramp".into()))]),
                output("out"),
            ],
            edges: vec![edge("fan", "phase", "lfo", "phase"), edge("lfo", "value", "out", "intensity")],
        };
        let mut rig = Rig::new(&graph, 4);
        rig.at(0.0);
        // Phases 0, -1/4, -2/4, -3/4 of a ramp: 0, .75, .5, .25.
        let dimmers: Vec<f32> = (0..4).map(|i| rig.channel("out", 1 + i * 6).round()).collect();
        assert_eq!(dimmers, vec![0.0, 191.0, 128.0, 64.0]);
        // Uniform nodes are evaluated once; the varying LFO previews fixture 1.
        assert!(rig.runtime.preview().contains_key("lfo"));
    }

    #[test]
    fn a_custom_lfo_plays_its_drawn_curve_across_the_fan() {
        let graph = Graph {
            nodes: vec![
                node("fan", NodeKind::Fan, &[]),
                node(
                    "lfo",
                    NodeKind::Lfo,
                    &[
                        ("waveform", ParamValue::Text("custom".into())),
                        ("curve", ParamValue::Text("0:0.2, 0.5:0.8".into())),
                        ("curveMode", ParamValue::Text("step".into())),
                    ],
                ),
                output("out"),
            ],
            edges: vec![edge("fan", "phase", "lfo", "phase"), edge("lfo", "value", "out", "intensity")],
        };
        let mut rig = Rig::new(&graph, 4);
        rig.at(0.0);
        // Phases 0, .75, .5, .25 of the curve: low for the first half, high after.
        let dimmers: Vec<f32> = (0..4).map(|i| rig.channel("out", 1 + i * 6).round()).collect();
        assert_eq!(dimmers, vec![51.0, 204.0, 204.0, 51.0]);
        // The playhead rides along in the preview, past the node's one socket.
        assert_eq!(rig.runtime.preview()["lfo"][1][0], 0.0);
    }

    #[test]
    fn a_counter_on_the_beat_steps_a_switch_through_colours() {
        let graph = Graph {
            nodes: vec![
                node("time", NodeKind::Time, &[]),
                node("count", NodeKind::Counter, &[("steps", ParamValue::Number(2.0))]),
                node(
                    "switch",
                    NodeKind::Switch,
                    &[("type", ParamValue::Text("color".into())), ("in0", ParamValue::Text("#ff0000".into())), ("in1", ParamValue::Text("#0000ff".into()))],
                ),
                node("one", NodeKind::Number, &[]),
                output("out"),
            ],
            edges: vec![
                edge("time", "onBeat", "count", "trigger"),
                edge("count", "index", "switch", "index"),
                edge("switch", "value", "out", "color"),
                edge("one", "value", "out", "intensity"),
            ],
        };
        let mut rig = Rig::new(&graph, 1);
        rig.at(0.5);
        assert_eq!((rig.channel("out", 2), rig.channel("out", 4)), (255.0, 0.0)); // red
        rig.at(1.1);
        assert_eq!((rig.channel("out", 2), rig.channel("out", 4)), (0.0, 255.0)); // blue
        rig.at(1.5);
        assert_eq!(rig.channel("out", 4), 255.0); // no new beat: still blue
        rig.at(2.1);
        assert_eq!(rig.channel("out", 2), 255.0); // wrapped back to red
    }

    #[test]
    fn faders_midi_and_envelopes_drive_outputs() {
        let graph = Graph {
            nodes: vec![
                node("fader", NodeKind::Fader, &[]),
                node("cc", NodeKind::MidiCc, &[("controller", ParamValue::Number(7.0))]),
                node("mul", NodeKind::Math, &[]),
                output("out"),
            ],
            edges: vec![edge("fader", "value", "mul", "a"), edge("cc", "value", "mul", "b"), edge("mul", "value", "out", "intensity")],
        };
        let mut rig = Rig::new(&graph, 1);
        rig.controls.insert("fader".into(), 0.5);
        rig.midi.feed("test", &[0xb3, 7, 127]);
        rig.at(0.1);
        assert_eq!(rig.channel("out", 1).round(), 128.0);
        assert_eq!(rig.midi.last, Some((false, 4, 7)));
    }

    #[test]
    fn an_envelope_flashes_on_each_trigger_and_decays() {
        let graph = Graph {
            nodes: vec![
                node("time", NodeKind::Time, &[]),
                node("env", NodeKind::Envelope, &[("attack", ParamValue::Number(0.0)), ("decay", ParamValue::Number(500.0))]),
                output("out"),
            ],
            edges: vec![edge("time", "onBeat", "env", "trigger"), edge("env", "value", "out", "intensity")],
        };
        let mut rig = Rig::new(&graph, 1);
        rig.at(0.9);
        rig.at(1.0); // beat: fires
        assert_eq!(rig.channel("out", 1), 255.0);
        rig.at(1.5); // 250ms later: half way down
        assert_eq!(rig.channel("out", 1).round(), 128.0);
    }

    #[test]
    fn removed_outputs_drop_their_layers_and_unconnected_inputs_write_nothing() {
        let graph = Graph {
            nodes: vec![node("one", NodeKind::Number, &[]), output("out")],
            edges: vec![edge("one", "value", "out", "intensity")],
        };
        let mut rig = Rig::new(&graph, 1);
        rig.layers.insert("graph-out-old".into(), (0, Layer::default()));
        rig.at(0.1);
        assert!(!rig.layers.contains_key("graph-out-old"));
        assert_eq!(rig.channel("out", 1), 255.0);
        assert_eq!(rig.channel("out", 2), -1.0, "colour not connected, so red isn't written");
    }

    #[test]
    fn a_disabled_node_acts_as_if_its_links_were_unplugged() {
        let graph = Graph {
            nodes: vec![
                node("one", NodeKind::Number, &[]),
                node("button", NodeKind::Button, &[]),
                // Unconnected, B would read its param: 0.5.
                node("mul", NodeKind::Math, &[("a", ParamValue::Number(1.0)), ("b", ParamValue::Number(0.5))]),
                node("half", NodeKind::Number, &[("value", ParamValue::Number(0.5))]),
                node("off", NodeKind::Output, &[("group", ParamValue::Number(1.0)), ("enabled", ParamValue::Number(0.0))]),
                output("out"),
            ],
            edges: vec![
                edge("button", "gate", "one", "enabled"),
                edge("one", "value", "out", "strobe"),
                edge("one", "value", "mul", "b"),
                edge("mul", "value", "out", "intensity"),
                edge("half", "value", "off", "intensity"),
            ],
        };
        let mut rig = Rig::new(&graph, 1);
        rig.at(0.1);
        assert_eq!(rig.channel("out", 1).round(), 128.0, "button up: B falls back to its param");
        assert_eq!(rig.channel("out", 5), -1.0, "and strobe isn't sent");
        assert!(rig.layers["graph-out-off"].1.values.is_empty(), "a disabled output sends nothing");

        rig.controls.insert("button".into(), 1.0);
        rig.at(0.2);
        assert_eq!(rig.channel("out", 1), 255.0, "button held: B reads the Number again");
        assert_ne!(rig.channel("out", 5), -1.0, "and strobe is sent");
    }

    #[test]
    fn the_midi_log_keeps_channel_messages_and_skips_clock() {
        let mut midi = MidiState::default();
        midi.feed("test", &[0xf8]); // clock
        midi.feed("test", &[0xb1, 7, 100]);
        midi.feed("test", &[0x92, 36, 0]); // note on at velocity 0 is a note off
        midi.feed("test", &[0xe0, 0x00, 0x40]);
        let kinds: Vec<_> = midi.since(0).iter().map(|e| (e.kind, e.channel, e.number, e.value)).collect();
        assert_eq!(
            kinds,
            vec![(MidiEventKind::Cc, 2, 7, 100), (MidiEventKind::NoteOff, 3, 36, 0), (MidiEventKind::PitchBend, 1, 0, 8192)]
        );
        assert_eq!(midi.since(2).len(), 1);
        for _ in 0..100 {
            midi.feed("test", &[0xb0, 1, 1]);
        }
        assert_eq!(midi.log.len(), MIDI_LOG_LEN);
    }

    #[test]
    fn any_is_on_while_any_input_is() {
        let graph = Graph {
            nodes: vec![
                node("button", NodeKind::Button, &[]),
                node("pad", NodeKind::MidiNote, &[("note", ParamValue::Number(36.0))]),
                node("any", NodeKind::Any, &[("count", ParamValue::Number(3.0))]),
                output("out"),
            ],
            edges: vec![edge("button", "gate", "any", "in0"), edge("pad", "gate", "any", "in1"), edge("any", "gate", "out", "intensity")],
        };
        let mut rig = Rig::new(&graph, 1);
        rig.at(0.1);
        assert_eq!(rig.channel("out", 1), 0.0);
        rig.midi.feed("test", &[0x90, 36, 100]);
        rig.at(0.2);
        assert_eq!(rig.channel("out", 1), 255.0, "pad held");
        rig.midi.feed("test", &[0x80, 36, 0]);
        rig.controls.insert("button".into(), 1.0);
        rig.at(0.3);
        assert_eq!(rig.channel("out", 1), 255.0, "button held");
        rig.controls.insert("button".into(), 0.0);
        rig.at(0.4);
        assert_eq!(rig.channel("out", 1), 0.0);
    }

    #[test]
    fn compile_reports_cycles_and_type_mismatches_but_keeps_the_rest() {
        let graph = Graph {
            nodes: vec![
                node("a", NodeKind::Math, &[]),
                node("b", NodeKind::Math, &[]),
                node("shape", NodeKind::Shape, &[]),
                node("rgb", NodeKind::Rgb, &[]),
                output("out"),
            ],
            edges: vec![
                edge("a", "value", "b", "a"),
                edge("b", "value", "a", "a"),
                edge("shape", "position", "rgb", "r"),
            ],
        };
        let patch = resolve(&pars(1), &library());
        let groups = vec![Group { id: 1, name: "All".into(), fixture_numbers: vec![1] }];
        let (plan, issues) = compile(&graph, &patch, &groups);
        let messages: Vec<String> = issues.iter().map(|i| format!("{}: {}", i.node, i.message)).collect();
        assert!(messages.iter().any(|m| m.starts_with("a: Part of a loop")), "{messages:?}");
        assert!(messages.iter().any(|m| m.starts_with("rgb: Can't connect")), "{messages:?}");
        assert_eq!(plan.outputs.len(), 1);
    }
}
