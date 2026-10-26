//! Graph → `GraphPlan`, the form the engine tick evaluates: nodes in
//! dependency order with their params parsed, each input either a constant
//! or a link to another node's output, and every Output node's targets
//! resolved to DMX channels. Recompiled on every edit to the graph, patch or
//! groups. Problems (bad edges, cycles, unknown fixtures) are reported per
//! node and leave the rest of the graph running.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use serde::Serialize;
use ts_rs::TS;

use super::model::{Graph, Node, ParamValue};
use super::registry::{self, NodeKind, SocketInfo, SocketType, DEFAULT_CURVE, MAX_SWITCH_INPUTS};
use super::write::{parse_hex, TargetItem};
use crate::engine::waves::{Curve, CurveMode, Fan, PositionShape, Waveform};
use crate::show::resolve::ResolvedPatch;
use crate::showfile::schema::Group;

/// A socket value. Numbers and triggers use the first lane, positions the
/// first two, colours all three.
pub type Val = [f32; 3];

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct GraphIssue {
    pub node: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Conversion {
    None,
    /// Number → colour (grey) or position (both axes).
    Spread,
    /// Colour → number: its brightest component.
    Brightness,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Const(Val),
    /// `fallback` is what the input reads while the linked node is disabled:
    /// its value as if unconnected.
    Link { node: usize, socket: usize, conversion: Conversion, fallback: Val },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rate {
    Beats(f32),
    Hz(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Config {
    None,
    Value(Val),
    /// `curve` is read once here and only used by `Waveform::Custom`.
    Lfo { waveform: Waveform, rate: Rate, width: f32, curve: Curve },
    Gain(f32),
    Midi { channel: u8, number: u8 },
    Fan(Fan),
    Math { op: MathOp, clamp: bool },
    MapRange { from: (f32, f32), to: (f32, f32), clamp: bool },
    Compare(CompareOp),
    Seed(u64),
    Times { rise_ms: f32, fall_ms: f32 },
    Steps(u32),
    Gradient { stops: Vec<Val>, wrap: bool },
    Shape { shape: PositionShape, rate: Rate },
    /// Switch: how many value inputs follow the index. Any: how many inputs.
    Count(usize),
    /// Index into `GraphPlan::outputs`.
    Output(usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MathOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Min,
    Max,
    Power,
    Modulo,
    Invert,
    Abs,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompareOp {
    Greater,
    Less,
    Equal,
}

#[derive(Debug, Clone)]
pub struct CompiledNode {
    pub id: Arc<str>,
    pub kind: NodeKind,
    /// One per input socket, in socket order.
    pub inputs: Vec<Source>,
    pub config: Config,
    /// Depends on which fixture is being evaluated, so it's evaluated per
    /// target fixture rather than once per tick.
    pub varying: bool,
}

/// Which input feeds which part of a fixture.
#[derive(Debug, Clone, PartialEq)]
pub enum Write {
    Intensity,
    Color,
    Position,
    Strobe,
    Attribute(String),
}

#[derive(Debug, Clone)]
pub struct CompiledOutput {
    pub node: usize,
    pub id: Arc<str>,
    pub priority: i64,
    pub targets: Vec<TargetItem>,
    /// Connected inputs only: (what it drives, input index).
    pub writes: Vec<(Write, usize)>,
    /// Varying nodes this output depends on, in evaluation order.
    pub varying_deps: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct GraphPlan {
    pub nodes: Vec<CompiledNode>,
    /// Non-varying nodes, in evaluation order.
    pub uniform_order: Vec<usize>,
    pub outputs: Vec<CompiledOutput>,
}

fn param<'a>(node: &'a Node, id: &str) -> Option<&'a ParamValue> {
    node.params.get(id)
}

fn number(node: &Node, id: &str, default: f32) -> f32 {
    param(node, id).and_then(ParamValue::as_f32).filter(|v| v.is_finite()).unwrap_or(default)
}

fn text<'a>(node: &'a Node, id: &str, default: &'a str) -> &'a str {
    param(node, id).and_then(ParamValue::as_text).unwrap_or(default)
}

fn flag(node: &Node, id: &str, default: bool) -> bool {
    param(node, id).and_then(ParamValue::as_bool).unwrap_or(default)
}

fn rate(node: &Node) -> Rate {
    let r = number(node, "rate", 1.0).max(0.0);
    if text(node, "unit", "beats") == "hz" {
        Rate::Hz(r)
    } else {
        Rate::Beats(r)
    }
}

fn waveform(name: &str) -> Waveform {
    match name {
        "square" => Waveform::Square,
        "saw" => Waveform::Saw,
        "ramp" => Waveform::Ramp,
        "random" => Waveform::Random,
        "step" => Waveform::Step,
        "pulse" => Waveform::Pulse,
        "custom" => Waveform::Custom,
        _ => Waveform::Sine,
    }
}

fn curve_mode(name: &str) -> CurveMode {
    match name {
        "smooth" => CurveMode::Smooth,
        "step" => CurveMode::Step,
        _ => CurveMode::Linear,
    }
}

fn shape(name: &str) -> PositionShape {
    match name {
        "figure8" => PositionShape::Figure8,
        "ballyhoo" => PositionShape::Ballyhoo,
        "panSaw" => PositionShape::PanSaw,
        "tiltSaw" => PositionShape::TiltSaw,
        _ => PositionShape::Circle,
    }
}

fn math_op(name: &str) -> MathOp {
    match name {
        "add" => MathOp::Add,
        "subtract" => MathOp::Subtract,
        "divide" => MathOp::Divide,
        "min" => MathOp::Min,
        "max" => MathOp::Max,
        "power" => MathOp::Power,
        "modulo" => MathOp::Modulo,
        "invert" => MathOp::Invert,
        "abs" => MathOp::Abs,
        _ => MathOp::Multiply,
    }
}

/// "1-8, 12" → [1..=8, 12]. Returns the numbers and any parts it couldn't read.
pub fn parse_fixture_list(text: &str) -> (Vec<u32>, Vec<String>) {
    let mut numbers = Vec::new();
    let mut bad = Vec::new();
    for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let parsed = match part.split_once('-') {
            Some((a, b)) => a.trim().parse::<u32>().ok().zip(b.trim().parse::<u32>().ok()),
            None => part.parse::<u32>().ok().map(|n| (n, n)),
        };
        match parsed {
            Some((a, b)) if a <= b && b - a < 10_000 => numbers.extend(a..=b),
            Some((a, b)) if a > b && a - b < 10_000 => numbers.extend((b..=a).rev()),
            _ => bad.push(part.to_string()),
        }
    }
    (numbers, bad)
}

/// The value of an unconnected input: the node's param for it, else the
/// socket's default.
fn constant(node: &Node, socket: &SocketInfo) -> Val {
    let value = node.params.get(&socket.id).or(socket.default.as_ref());
    match (socket.ty, value) {
        (SocketType::Color, Some(ParamValue::Text(t))) => parse_hex(t).unwrap_or([0.0; 3]),
        (SocketType::Color, Some(v)) => [v.as_f32().unwrap_or(0.0); 3],
        (SocketType::Position, Some(v)) => {
            let x = v.as_f32().unwrap_or(0.5);
            [x, x, 0.0]
        }
        (_, Some(v)) => [v.as_f32().unwrap_or(0.0), 0.0, 0.0],
        (_, None) => [0.0; 3],
    }
}

fn config(node: &Node) -> Config {
    match node.kind {
        NodeKind::Number => Config::Value([number(node, "value", 1.0), 0.0, 0.0]),
        NodeKind::Color => Config::Value(parse_hex(text(node, "color", "#ffffff")).unwrap_or([1.0; 3])),
        NodeKind::Lfo => Config::Lfo {
            waveform: waveform(text(node, "waveform", "sine")),
            rate: rate(node),
            width: number(node, "width", 50.0),
            curve: Curve::parse(text(node, "curve", DEFAULT_CURVE), curve_mode(text(node, "curveMode", "linear"))),
        },
        NodeKind::Audio => Config::Gain(number(node, "gain", 1.0)),
        NodeKind::MidiCc => Config::Midi { channel: number(node, "channel", 0.0) as u8, number: number(node, "controller", 1.0) as u8 },
        NodeKind::MidiNote => Config::Midi { channel: number(node, "channel", 0.0) as u8, number: number(node, "note", 36.0) as u8 },
        NodeKind::Fan => Config::Fan(Fan {
            groups: number(node, "groups", 0.0) as u32,
            blocks: number(node, "blocks", 0.0) as u32,
            wings: number(node, "wings", 0.0) as u32,
        }),
        NodeKind::Math => Config::Math { op: math_op(text(node, "op", "multiply")), clamp: flag(node, "clamp", false) },
        NodeKind::MapRange => Config::MapRange {
            from: (number(node, "fromMin", 0.0), number(node, "fromMax", 1.0)),
            to: (number(node, "toMin", 0.0), number(node, "toMax", 1.0)),
            clamp: flag(node, "clamp", true),
        },
        NodeKind::Compare => Config::Compare(match text(node, "op", "greater") {
            "less" => CompareOp::Less,
            "equal" => CompareOp::Equal,
            _ => CompareOp::Greater,
        }),
        NodeKind::Random => Config::Seed(number(node, "seed", 0.0) as u64),
        NodeKind::Smooth => Config::Times { rise_ms: number(node, "attack", 50.0), fall_ms: number(node, "release", 300.0) },
        NodeKind::Envelope => Config::Times { rise_ms: number(node, "attack", 10.0), fall_ms: number(node, "decay", 250.0) },
        NodeKind::Counter => Config::Steps(number(node, "steps", 4.0).max(1.0) as u32),
        NodeKind::Gradient => Config::Gradient {
            stops: text(node, "stops", "#000000, #ffffff").split(',').filter_map(parse_hex).collect(),
            wrap: flag(node, "wrap", true),
        },
        NodeKind::Shape => Config::Shape { shape: shape(text(node, "shape", "circle")), rate: rate(node) },
        NodeKind::Any => Config::Count(registry::any_inputs(&node.params)),
        NodeKind::Switch => Config::Count(number(node, "count", 2.0).clamp(2.0, MAX_SWITCH_INPUTS as f32) as usize),
        _ => Config::None,
    }
}

/// Resolves an Output node's targets. Returns the items and any problems.
fn targets(node: &Node, patch: &ResolvedPatch, groups: &[Group]) -> (Vec<TargetItem>, Vec<String>) {
    let mut problems = Vec::new();
    let group_id = number(node, "group", 0.0) as u32;
    let numbers = if group_id > 0 {
        match groups.iter().find(|g| g.id == group_id) {
            Some(g) => g.fixture_numbers.clone(),
            None => {
                problems.push(format!("No group {group_id}"));
                Vec::new()
            }
        }
    } else {
        let (numbers, bad) = parse_fixture_list(text(node, "fixtures", ""));
        if !bad.is_empty() {
            problems.push(format!("Can't read fixtures \"{}\"", bad.join(", ")));
        }
        numbers
    };
    let cells_separately = flag(node, "cells", false);

    let mut items = Vec::new();
    let mut missing = Vec::new();
    for number in numbers {
        let Some(f) = patch.fixture(number) else {
            missing.push(number.to_string());
            continue;
        };
        let base = TargetItem {
            fixture_number: number,
            cell: None,
            universe: f.universe,
            dimmers: Vec::new(),
            hold_full: Vec::new(),
            color_sets: Vec::new(),
            attributes: HashMap::new(),
            invert_pan: f.invert_pan,
            invert_tilt: f.invert_tilt,
            swap_pan_tilt: f.swap_pan_tilt,
        };
        let whole_dimmer = f.attributes.get("dimmer").cloned();
        if cells_separately && !f.cells.is_empty() {
            for (i, cell) in f.cells.iter().enumerate() {
                let mut attributes = f.attributes.clone();
                attributes.extend(cell.iter().map(|(k, v)| (k.clone(), v.clone())));
                let own = cell.get("dimmer").cloned();
                items.push(TargetItem {
                    cell: Some(i),
                    dimmers: own.iter().cloned().collect(),
                    hold_full: if own.is_none() { whole_dimmer.iter().cloned().collect() } else { Vec::new() },
                    color_sets: vec![cell.clone()],
                    attributes,
                    ..base.clone()
                });
            }
        } else {
            let dimmers = match &whole_dimmer {
                Some(d) => vec![d.clone()],
                None => f.cells.iter().filter_map(|c| c.get("dimmer").cloned()).collect(),
            };
            let whole_has_color = ["red", "green", "blue", "white", "cyan", "magenta", "yellow", "colorwheel"]
                .iter()
                .any(|a| f.attributes.contains_key(*a));
            let color_sets = if whole_has_color || f.cells.is_empty() { vec![f.attributes.clone()] } else { f.cells.clone() };
            let mut attributes = f.cells.first().cloned().unwrap_or_default();
            attributes.extend(f.attributes.iter().map(|(k, v)| (k.clone(), v.clone())));
            items.push(TargetItem { dimmers, color_sets, attributes, ..base });
        }
    }
    if !missing.is_empty() {
        problems.push(format!("Fixture{} {} not patched", if missing.len() > 1 { "s" } else { "" }, missing.join(", ")));
    }
    (items, problems)
}

pub fn compile(graph: &Graph, patch: &ResolvedPatch, groups: &[Group]) -> (GraphPlan, Vec<GraphIssue>) {
    let mut issues = Vec::new();
    let mut issue = |node: &str, message: String| issues.push(GraphIssue { node: node.to_string(), message });

    let index: HashMap<&str, usize> = graph.nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();
    let sockets: Vec<(Vec<SocketInfo>, Vec<SocketInfo>)> =
        graph.nodes.iter().map(|n| registry::node_sockets(n.kind, &n.params)).collect();

    // Resolve edges to (from node, from socket) per (to node, to socket).
    let mut links: HashMap<(usize, usize), (usize, usize, Conversion)> = HashMap::new();
    for e in &graph.edges {
        let (Some(&from), Some(&to)) = (index.get(e.from_node.as_str()), index.get(e.to_node.as_str())) else {
            issue(&e.to_node, format!("Link from missing node {}", e.from_node));
            continue;
        };
        let Some(out_idx) = sockets[from].1.iter().position(|s| s.id == e.from_socket) else {
            issue(&e.to_node, format!("{} has no output \"{}\"", e.from_node, e.from_socket));
            continue;
        };
        let Some(in_idx) = sockets[to].0.iter().position(|s| s.id == e.to_socket) else {
            issue(&e.to_node, format!("No input \"{}\"", e.to_socket));
            continue;
        };
        let (from_ty, to_ty) = (sockets[from].1[out_idx].ty, sockets[to].0[in_idx].ty);
        if !from_ty.feeds(to_ty) {
            issue(&e.to_node, format!("Can't connect a {from_ty:?} output to the {to_ty:?} input \"{}\"", e.to_socket));
            continue;
        }
        let conversion = match (from_ty, to_ty) {
            (SocketType::Color, SocketType::Number) => Conversion::Brightness,
            (SocketType::Number | SocketType::Trigger, SocketType::Color | SocketType::Position) => Conversion::Spread,
            _ => Conversion::None,
        };
        links.insert((to, in_idx), (from, out_idx, conversion));
    }

    // Topological order (Kahn). Nodes left over sit on a cycle.
    let count = graph.nodes.len();
    let mut indegree = vec![0usize; count];
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (&(to, _), &(from, _, _)) in &links {
        indegree[to] += 1;
        dependents[from].push(to);
    }
    let mut queue: VecDeque<usize> = (0..count).filter(|&i| indegree[i] == 0).collect();
    let mut order = Vec::with_capacity(count);
    while let Some(i) = queue.pop_front() {
        order.push(i);
        for &d in &dependents[i] {
            indegree[d] -= 1;
            if indegree[d] == 0 {
                queue.push_back(d);
            }
        }
    }
    let ordered: HashSet<usize> = order.iter().copied().collect();
    for (i, node) in graph.nodes.iter().enumerate() {
        if !ordered.contains(&i) {
            issue(&node.id, "Part of a loop; its links are ignored".to_string());
        }
    }
    // Drop links into or out of cycle nodes, then append those nodes.
    links.retain(|&(to, _), &mut (from, _, _)| ordered.contains(&to) && ordered.contains(&from));
    order.extend((0..count).filter(|i| !ordered.contains(i)));

    let mut nodes: Vec<CompiledNode> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| CompiledNode {
            id: Arc::from(node.id.as_str()),
            kind: node.kind,
            inputs: sockets[i]
                .0
                .iter()
                .enumerate()
                .map(|(s, socket)| match links.get(&(i, s)) {
                    Some(&(from, out, conversion)) => Source::Link { node: from, socket: out, conversion, fallback: constant(node, socket) },
                    None => Source::Const(constant(node, socket)),
                })
                .collect(),
            config: config(node),
            varying: false,
        })
        .collect();

    for &i in &order {
        let source_varying = nodes[i].inputs.iter().any(|s| matches!(s, Source::Link { node, .. } if nodes[*node].varying));
        nodes[i].varying = matches!(nodes[i].kind, NodeKind::FixtureIndex | NodeKind::Fan | NodeKind::Random) || source_varying;
        // Only fixture-independent nodes run without an Output to drive them.
        if nodes[i].kind == NodeKind::TapTempo && nodes[i].varying {
            issue(&graph.nodes[i].id, "Tap can't depend on the fixture (Fixture Index, Fan, Random); it won't tap".to_string());
        }
    }

    let position: HashMap<usize, usize> = order.iter().enumerate().map(|(p, &i)| (i, p)).collect();
    let mut outputs = Vec::new();
    for &i in &order {
        let node = &graph.nodes[i];
        if node.kind != NodeKind::Output {
            continue;
        }
        let (items, problems) = targets(node, patch, groups);
        for p in problems {
            issue(&node.id, p);
        }
        let writes = sockets[i]
            .0
            .iter()
            .enumerate()
            .filter(|(s, socket)| socket.id != registry::ENABLED && links.contains_key(&(i, *s)))
            .map(|(s, socket)| {
                let write = match socket.id.as_str() {
                    "intensity" => Write::Intensity,
                    "color" => Write::Color,
                    "position" => Write::Position,
                    "strobe" => Write::Strobe,
                    other => Write::Attribute(other.to_string()),
                };
                (write, s)
            })
            .collect();
        // Every varying node upstream of this output.
        let mut deps = Vec::new();
        let mut stack = vec![i];
        let mut seen = HashSet::new();
        while let Some(n) = stack.pop() {
            for src in &nodes[n].inputs {
                if let Source::Link { node, .. } = src {
                    if nodes[*node].varying && seen.insert(*node) {
                        deps.push(*node);
                        stack.push(*node);
                    }
                }
            }
        }
        deps.sort_by_key(|n| position[n]);
        nodes[i].config = Config::Output(outputs.len());
        outputs.push(CompiledOutput {
            node: i,
            id: nodes[i].id.clone(),
            priority: number(node, "priority", 0.0) as i64,
            targets: items,
            writes,
            varying_deps: deps,
        });
    }

    let uniform_order =
        order.iter().copied().filter(|&i| !nodes[i].varying && !matches!(nodes[i].kind, NodeKind::Output | NodeKind::Frame)).collect();
    (GraphPlan { nodes, uniform_order, outputs }, issues)
}
