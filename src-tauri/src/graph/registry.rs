//! Every node type: its sockets, parameters and documentation. The editor
//! UI and MCP clients both render and describe nodes from this one table
//! (`node_types`), so adding a node type here is all it takes to expose it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::model::ParamValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum NodeKind {
    Number,
    Color,
    Time,
    Lfo,
    Audio,
    MidiCc,
    MidiNote,
    Fader,
    Button,
    Timecode,
    FixtureIndex,
    Fan,
    Math,
    MapRange,
    Compare,
    Random,
    Smooth,
    Envelope,
    Toggle,
    Counter,
    Rgb,
    Hsv,
    MixColor,
    Gradient,
    Shape,
    PanTilt,
    Switch,
    Mix,
    Output,
}

impl NodeKind {
    pub const ALL: [NodeKind; 29] = [
        NodeKind::Number,
        NodeKind::Color,
        NodeKind::Time,
        NodeKind::Lfo,
        NodeKind::Audio,
        NodeKind::MidiCc,
        NodeKind::MidiNote,
        NodeKind::Fader,
        NodeKind::Button,
        NodeKind::Timecode,
        NodeKind::FixtureIndex,
        NodeKind::Fan,
        NodeKind::Math,
        NodeKind::MapRange,
        NodeKind::Compare,
        NodeKind::Random,
        NodeKind::Smooth,
        NodeKind::Envelope,
        NodeKind::Toggle,
        NodeKind::Counter,
        NodeKind::Rgb,
        NodeKind::Hsv,
        NodeKind::MixColor,
        NodeKind::Gradient,
        NodeKind::Shape,
        NodeKind::PanTilt,
        NodeKind::Switch,
        NodeKind::Mix,
        NodeKind::Output,
    ];
}

/// What flows along an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum SocketType {
    /// A level, usually 0-1.
    Number,
    /// RGB, each 0-1.
    Color,
    /// Pan and tilt, each 0-1 (0.5 is centre).
    Position,
    /// On/off; on is above 0.5.
    Trigger,
}

impl SocketType {
    /// Whether an output of type `self` may feed an input of type `to`.
    /// Numbers and triggers interchange; a number feeds a colour (grey) or
    /// a position (both axes); a colour feeds a number (its brightness).
    pub fn feeds(self, to: SocketType) -> bool {
        use SocketType::*;
        self == to
            || matches!((self, to), (Number, Trigger) | (Trigger, Number) | (Number, Color) | (Trigger, Color) | (Number, Position) | (Color, Number))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct SocketInfo {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub ty: SocketType,
    /// An unconnected input's value (overridable by a param with the
    /// socket's id). `None` for outputs, and for inputs that do nothing
    /// unless connected (an Output node's attributes).
    pub default: Option<ParamValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum ParamKind {
    Number { min: f64, max: f64, step: f64 },
    Bool,
    /// Free text; `hint` describes the format.
    Text { hint: String },
    Choice { options: Vec<String> },
    /// "#rrggbb".
    Color,
    /// A group id (0: none).
    Group,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct ParamInfo {
    pub id: String,
    pub label: String,
    #[serde(flatten)]
    pub kind: ParamKind,
    pub default: ParamValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct NodeTypeInfo {
    pub kind: NodeKind,
    pub label: String,
    /// "Input", "Fixtures", "Math", "Colour", "Position", "Switching", "Output".
    pub category: String,
    pub description: String,
    /// Sockets with default params. Switch, Mix and Output change theirs
    /// with their params; see the graph's per-node sockets.
    pub inputs: Vec<SocketInfo>,
    pub outputs: Vec<SocketInfo>,
    pub params: Vec<ParamInfo>,
}

pub const WAVEFORMS: [&str; 7] = ["sine", "square", "saw", "ramp", "random", "step", "pulse"];
pub const SHAPES: [&str; 5] = ["circle", "figure8", "ballyhoo", "panSaw", "tiltSaw"];
pub const MATH_OPS: [&str; 10] = ["add", "subtract", "multiply", "divide", "min", "max", "power", "modulo", "invert", "abs"];
pub const COMPARE_OPS: [&str; 3] = ["greater", "less", "equal"];
pub const VALUE_TYPES: [&str; 3] = ["number", "color", "position"];
/// Most inputs a Switch can have.
pub const MAX_SWITCH_INPUTS: usize = 8;
/// Most outputs any node has.
pub const MAX_OUTPUTS: usize = 5;

fn n(v: f64) -> ParamValue {
    ParamValue::Number(v)
}

fn text(v: &str) -> ParamValue {
    ParamValue::Text(v.to_string())
}

fn input(id: &str, label: &str, ty: SocketType, default: ParamValue) -> SocketInfo {
    SocketInfo { id: id.into(), label: label.into(), ty, default: Some(default) }
}

/// An input that does nothing unless connected.
fn input_only(id: &str, label: &str, ty: SocketType) -> SocketInfo {
    SocketInfo { id: id.into(), label: label.into(), ty, default: None }
}

fn output(id: &str, label: &str, ty: SocketType) -> SocketInfo {
    SocketInfo { id: id.into(), label: label.into(), ty, default: None }
}

fn number(id: &str, label: &str, min: f64, max: f64, step: f64, default: f64) -> ParamInfo {
    ParamInfo { id: id.into(), label: label.into(), kind: ParamKind::Number { min, max, step }, default: n(default) }
}

fn choice(id: &str, label: &str, options: &[&str], default: &str) -> ParamInfo {
    ParamInfo {
        id: id.into(),
        label: label.into(),
        kind: ParamKind::Choice { options: options.iter().map(|s| s.to_string()).collect() },
        default: text(default),
    }
}

fn text_param(id: &str, label: &str, hint: &str, default: &str) -> ParamInfo {
    ParamInfo { id: id.into(), label: label.into(), kind: ParamKind::Text { hint: hint.into() }, default: text(default) }
}

fn bool_param(id: &str, label: &str, default: bool) -> ParamInfo {
    ParamInfo { id: id.into(), label: label.into(), kind: ParamKind::Bool, default: ParamValue::Bool(default) }
}

fn rate_params() -> Vec<ParamInfo> {
    vec![
        number("rate", "Rate", 0.0, 64.0, 0.25, 1.0),
        choice("unit", "Per", &["beats", "hz"], "beats"),
    ]
}

fn value_type(ty: &str) -> SocketType {
    match ty {
        "color" => SocketType::Color,
        "position" => SocketType::Position,
        _ => SocketType::Number,
    }
}

fn value_default(ty: SocketType) -> ParamValue {
    match ty {
        SocketType::Color => text("#000000"),
        _ => n(0.0),
    }
}

/// The node type's description and default sockets.
pub fn node_type(kind: NodeKind) -> NodeTypeInfo {
    use SocketType::*;
    let (label, category, description, inputs, outputs, params): (&str, &str, &str, Vec<SocketInfo>, Vec<SocketInfo>, Vec<ParamInfo>) = match kind {
        NodeKind::Number => (
            "Number",
            "Input",
            "A fixed number.",
            vec![],
            vec![output("value", "Value", Number)],
            vec![number("value", "Value", -1000.0, 1000.0, 0.01, 1.0)],
        ),
        NodeKind::Color => (
            "Color",
            "Input",
            "A fixed colour.",
            vec![],
            vec![output("color", "Color", Color)],
            vec![ParamInfo { id: "color".into(), label: "Color".into(), kind: ParamKind::Color, default: text("#ffffff") }],
        ),
        NodeKind::Time => (
            "Time",
            "Input",
            "The show clock. Beat and Bar run 0-1 over each beat and each 4-beat bar at the show BPM; On beat fires once per beat.",
            vec![],
            vec![
                output("beat", "Beat", Number),
                output("bar", "Bar", Number),
                output("onBeat", "On beat", Trigger),
                output("bpm", "BPM", Number),
                output("seconds", "Seconds", Number),
            ],
            vec![],
        ),
        NodeKind::Lfo => (
            "LFO",
            "Input",
            "A repeating wave from 0 to 1, at a rate in cycles per beat (follows the BPM) or per second. Phase shifts it, in cycles: feed a Fan into it to make the wave travel across fixtures.",
            vec![input("phase", "Phase", Number, n(0.0))],
            vec![output("value", "Value", Number)],
            [
                vec![choice("waveform", "Wave", &WAVEFORMS, "sine")],
                rate_params(),
                vec![number("width", "Width %", 1.0, 100.0, 1.0, 50.0)],
            ]
            .concat(),
        ),
        NodeKind::Audio => (
            "Audio",
            "Input",
            "Live levels of the audio input (turn on audio analysis in Setup), each 0-1.",
            vec![],
            vec![
                output("low", "Low", Number),
                output("mid", "Mid", Number),
                output("high", "High", Number),
                output("loudness", "Loudness", Number),
            ],
            vec![number("gain", "Gain", 0.0, 4.0, 0.05, 1.0)],
        ),
        NodeKind::MidiCc => (
            "MIDI CC",
            "Input",
            "A MIDI controller (knob, fader) on the MIDI port chosen in Setup, 0-1. Channel 0 listens on every channel.",
            vec![],
            vec![output("value", "Value", Number)],
            vec![number("channel", "Channel", 0.0, 16.0, 1.0, 0.0), number("controller", "CC", 0.0, 127.0, 1.0, 1.0)],
        ),
        NodeKind::MidiNote => (
            "MIDI Note",
            "Input",
            "A MIDI note (pad, key): Gate is on while it's held, Velocity is how hard it was hit (0-1). Channel 0 listens on every channel.",
            vec![],
            vec![output("gate", "Gate", Trigger), output("velocity", "Velocity", Number)],
            vec![number("channel", "Channel", 0.0, 16.0, 1.0, 0.0), number("note", "Note", 0.0, 127.0, 1.0, 36.0)],
        ),
        NodeKind::Fader => (
            "Fader",
            "Input",
            "A fader on the playback bar, 0-1.",
            vec![],
            vec![output("value", "Value", Number)],
            vec![text_param("label", "Label", "Shown on the playback bar", "Fader")],
        ),
        NodeKind::Button => (
            "Button",
            "Input",
            "A button on the playback bar: on while held (flash) or on/off per press (toggle).",
            vec![],
            vec![output("gate", "Gate", Trigger)],
            vec![text_param("label", "Label", "Shown on the playback bar", "Button"), choice("mode", "Mode", &["flash", "toggle"], "flash")],
        ),
        NodeKind::Timecode => (
            "Timecode",
            "Input",
            "The incoming timecode (MTC or LTC, chosen in Setup): position in seconds, and whether it's running.",
            vec![],
            vec![output("seconds", "Seconds", Number), output("running", "Running", Trigger)],
            vec![],
        ),
        NodeKind::FixtureIndex => (
            "Fixture Index",
            "Fixtures",
            "Which fixture is being lit: its place in the Output's target list (Index from 0, Position 0-1 from first to last), and how many there are. Makes every fixture behave differently.",
            vec![],
            vec![output("index", "Index", Number), output("position", "Position", Number), output("count", "Count", Number)],
            vec![],
        ),
        NodeKind::Fan => (
            "Fan",
            "Fixtures",
            "A per-fixture phase offset: Spread 1 spaces the fixtures evenly over one whole cycle. Feed it into an LFO or Shape's Phase. Groups/Blocks/Wings split the fan like a console's effect engine.",
            vec![input("spread", "Spread", Number, n(1.0))],
            vec![output("phase", "Phase", Number)],
            vec![
                number("groups", "Groups", 0.0, 64.0, 1.0, 0.0),
                number("blocks", "Blocks", 0.0, 64.0, 1.0, 0.0),
                number("wings", "Wings", 0.0, 16.0, 1.0, 0.0),
            ],
        ),
        NodeKind::Math => (
            "Math",
            "Math",
            "Combines two numbers. Invert is 1 - A; Abs ignores B.",
            vec![input("a", "A", Number, n(0.0)), input("b", "B", Number, n(1.0))],
            vec![output("value", "Value", Number)],
            vec![choice("op", "Operation", &MATH_OPS, "multiply"), bool_param("clamp", "Clamp 0-1", false)],
        ),
        NodeKind::MapRange => (
            "Map Range",
            "Math",
            "Rescales a value from one range to another.",
            vec![input("value", "Value", Number, n(0.0))],
            vec![output("value", "Value", Number)],
            vec![
                number("fromMin", "From min", -1000.0, 1000.0, 0.01, 0.0),
                number("fromMax", "From max", -1000.0, 1000.0, 0.01, 1.0),
                number("toMin", "To min", -1000.0, 1000.0, 0.01, 0.0),
                number("toMax", "To max", -1000.0, 1000.0, 0.01, 1.0),
                bool_param("clamp", "Clamp", true),
            ],
        ),
        NodeKind::Compare => (
            "Compare",
            "Math",
            "On when A is greater than / less than / equal to B.",
            vec![input("a", "A", Number, n(0.0)), input("b", "B", Number, n(0.5))],
            vec![output("value", "Result", Trigger)],
            vec![choice("op", "Test", &COMPARE_OPS, "greater")],
        ),
        NodeKind::Random => (
            "Random",
            "Math",
            "A random value 0-1, different for every fixture, picking a new one each time Trigger turns on.",
            vec![input("trigger", "Trigger", Trigger, n(0.0))],
            vec![output("value", "Value", Number)],
            vec![number("seed", "Seed", 0.0, 9999.0, 1.0, 0.0)],
        ),
        NodeKind::Smooth => (
            "Smooth",
            "Math",
            "Follows its input, taking Attack ms to rise and Release ms to fall.",
            vec![input("value", "Value", Number, n(0.0))],
            vec![output("value", "Value", Number)],
            vec![number("attack", "Attack ms", 0.0, 10000.0, 10.0, 50.0), number("release", "Release ms", 0.0, 10000.0, 10.0, 300.0)],
        ),
        NodeKind::Envelope => (
            "Envelope",
            "Math",
            "When Trigger turns on, rises to 1 over Attack ms and falls back to 0 over Decay ms. A flash on every beat or hit.",
            vec![input("trigger", "Trigger", Trigger, n(0.0))],
            vec![output("value", "Value", Number)],
            vec![number("attack", "Attack ms", 0.0, 10000.0, 5.0, 10.0), number("decay", "Decay ms", 0.0, 10000.0, 10.0, 250.0)],
        ),
        NodeKind::Toggle => (
            "Toggle",
            "Math",
            "Flips on/off each time Trigger turns on.",
            vec![input("trigger", "Trigger", Trigger, n(0.0))],
            vec![output("value", "State", Trigger)],
            vec![],
        ),
        NodeKind::Counter => (
            "Counter",
            "Math",
            "Counts 0, 1, 2 ... Steps-1 and round again, one step each time Trigger turns on; Reset goes back to 0. Feed Index into a Switch to step through looks.",
            vec![input("trigger", "Trigger", Trigger, n(0.0)), input("reset", "Reset", Trigger, n(0.0))],
            vec![output("index", "Index", Number), output("position", "Position", Number)],
            vec![number("steps", "Steps", 1.0, 64.0, 1.0, 4.0)],
        ),
        NodeKind::Rgb => (
            "RGB",
            "Colour",
            "Builds a colour from red, green and blue (0-1).",
            vec![input("r", "Red", Number, n(1.0)), input("g", "Green", Number, n(1.0)), input("b", "Blue", Number, n(1.0))],
            vec![output("color", "Color", Color)],
            vec![],
        ),
        NodeKind::Hsv => (
            "HSV",
            "Colour",
            "Builds a colour from hue (0-1 around the colour wheel), saturation and value.",
            vec![input("h", "Hue", Number, n(0.0)), input("s", "Saturation", Number, n(1.0)), input("v", "Value", Number, n(1.0))],
            vec![output("color", "Color", Color)],
            vec![],
        ),
        NodeKind::MixColor => (
            "Mix Color",
            "Colour",
            "Blends from colour A (Amount 0) to B (Amount 1).",
            vec![input("a", "A", Color, text("#ff0000")), input("b", "B", Color, text("#0000ff")), input("t", "Amount", Number, n(0.5))],
            vec![output("color", "Color", Color)],
            vec![],
        ),
        NodeKind::Gradient => (
            "Gradient",
            "Colour",
            "Picks a colour along evenly spaced stops; Position 0 is the first stop, 1 the last.",
            vec![input("t", "Position", Number, n(0.0))],
            vec![output("color", "Color", Color)],
            vec![text_param("stops", "Stops", "Colours, comma-separated: #ff0000, #00ff00", "#ff0000, #ffff00, #00ff00, #00ffff, #0000ff, #ff00ff"), bool_param("wrap", "Wrap around", true)],
        ),
        NodeKind::Shape => (
            "Shape",
            "Position",
            "Moves pan/tilt around a shape at a rate per beat or per second, centred on Center with the given Size. Phase shifts it in cycles, like an LFO.",
            vec![
                input("phase", "Phase", Number, n(0.0)),
                input("size", "Size", Number, n(0.25)),
                input("center", "Center", Position, n(0.5)),
            ],
            vec![output("position", "Position", Position)],
            [vec![choice("shape", "Shape", &SHAPES, "circle")], rate_params()].concat(),
        ),
        NodeKind::PanTilt => (
            "Pan/Tilt",
            "Position",
            "Builds a position from pan and tilt (0-1, 0.5 is centre).",
            vec![input("pan", "Pan", Number, n(0.5)), input("tilt", "Tilt", Number, n(0.5))],
            vec![output("position", "Position", Position)],
            vec![],
        ),
        NodeKind::Switch => (
            "Switch",
            "Switching",
            "Passes one of its inputs through: Index 0 picks the first, 1 the second ... Drive Index from a Counter, MIDI or a Fader to change looks live.",
            vec![],
            vec![],
            vec![choice("type", "Type", &VALUE_TYPES, "color"), number("count", "Inputs", 2.0, MAX_SWITCH_INPUTS as f64, 1.0, 2.0)],
        ),
        NodeKind::Mix => (
            "Mix",
            "Switching",
            "Crossfades from A (Amount 0) to B (Amount 1).",
            vec![],
            vec![],
            vec![choice("type", "Type", &VALUE_TYPES, "number")],
        ),
        NodeKind::Output => (
            "Fixture Output",
            "Output",
            "Sends values to fixtures: a group, or fixture numbers like \"1-8, 12\". Only connected inputs are sent. Inputs are evaluated once per fixture, so Fixture Index and Fan vary them across the targets. Where two outputs drive the same fixture, intensity takes the highest and everything else the higher Priority.",
            vec![],
            vec![],
            vec![
                ParamInfo { id: "group".into(), label: "Group".into(), kind: ParamKind::Group, default: n(0.0) },
                text_param("fixtures", "Fixtures", "Fixture numbers when no group: 1-8, 12", ""),
                bool_param("cells", "Each cell separately", false),
                number("priority", "Priority", 0.0, 100.0, 1.0, 0.0),
                text_param("attributes", "More attributes", "Extra inputs, comma-separated: zoom, gobo, white", ""),
            ],
        ),
    };
    let mut info = NodeTypeInfo {
        kind,
        label: label.into(),
        category: category.into(),
        description: description.into(),
        inputs,
        outputs,
        params,
    };
    let (inputs, outputs) = sockets(kind, &BTreeMap::new());
    if !inputs.is_empty() || !outputs.is_empty() {
        info.inputs = inputs;
        info.outputs = outputs;
    }
    info
}

fn param_text<'a>(params: &'a BTreeMap<String, ParamValue>, id: &str, default: &'a str) -> &'a str {
    params.get(id).and_then(ParamValue::as_text).unwrap_or(default)
}

/// Output-node inputs that are always there; "attributes" adds more.
pub const OUTPUT_SOCKETS: [(&str, &str, SocketType); 4] = [
    ("intensity", "Intensity", SocketType::Number),
    ("color", "Color", SocketType::Color),
    ("position", "Position", SocketType::Position),
    ("strobe", "Strobe", SocketType::Number),
];

/// The extra attribute names an Output node's "attributes" param lists.
pub fn extra_attributes(params: &BTreeMap<String, ParamValue>) -> Vec<String> {
    param_text(params, "attributes", "")
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty() && !OUTPUT_SOCKETS.iter().any(|(id, _, _)| id == s))
        .collect()
}

/// Sockets of the node types whose sockets depend on their params; empty
/// for the rest (use `node_type`).
pub fn sockets(kind: NodeKind, params: &BTreeMap<String, ParamValue>) -> (Vec<SocketInfo>, Vec<SocketInfo>) {
    match kind {
        NodeKind::Switch => {
            let ty = value_type(param_text(params, "type", "color"));
            let count = params.get("count").and_then(ParamValue::as_f32).unwrap_or(2.0).clamp(2.0, MAX_SWITCH_INPUTS as f32) as usize;
            let mut inputs = vec![input("index", "Index", SocketType::Number, n(0.0))];
            inputs.extend((0..count).map(|i| input(&format!("in{i}"), &format!("{i}"), ty, value_default(ty))));
            (inputs, vec![output("value", "Value", ty)])
        }
        NodeKind::Mix => {
            let ty = value_type(param_text(params, "type", "number"));
            (
                vec![input("a", "A", ty, value_default(ty)), input("b", "B", ty, value_default(ty)), input("t", "Amount", SocketType::Number, n(0.5))],
                vec![output("value", "Value", ty)],
            )
        }
        NodeKind::Output => {
            let mut inputs: Vec<SocketInfo> = OUTPUT_SOCKETS.iter().map(|(id, label, ty)| input_only(id, label, *ty)).collect();
            inputs.extend(extra_attributes(params).iter().map(|a| input_only(a, a, SocketType::Number)));
            (inputs, vec![])
        }
        _ => (vec![], vec![]),
    }
}

/// A node's actual sockets, given its params.
pub fn node_sockets(kind: NodeKind, params: &BTreeMap<String, ParamValue>) -> (Vec<SocketInfo>, Vec<SocketInfo>) {
    let (inputs, outputs) = sockets(kind, params);
    if inputs.is_empty() && outputs.is_empty() {
        let info = node_type(kind);
        (info.inputs, info.outputs)
    } else {
        (inputs, outputs)
    }
}

pub fn node_types() -> Vec<NodeTypeInfo> {
    NodeKind::ALL.iter().map(|&k| node_type(k)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_node_type_is_described_and_fits_the_output_limit() {
        for info in node_types() {
            assert!(!info.description.is_empty(), "{:?}", info.kind);
            assert!(info.outputs.len() <= MAX_OUTPUTS, "{:?}", info.kind);
            let ids: std::collections::HashSet<&str> = info.inputs.iter().map(|s| s.id.as_str()).collect();
            assert_eq!(ids.len(), info.inputs.len(), "duplicate input ids on {:?}", info.kind);
        }
    }

    #[test]
    fn switch_and_output_sockets_follow_their_params() {
        let mut params = BTreeMap::new();
        params.insert("type".to_string(), text("position"));
        params.insert("count".to_string(), n(3.0));
        let (inputs, outputs) = node_sockets(NodeKind::Switch, &params);
        assert_eq!(inputs.len(), 4);
        assert_eq!(outputs[0].ty, SocketType::Position);

        let mut params = BTreeMap::new();
        params.insert("attributes".to_string(), text("Zoom, gobo, color"));
        let (inputs, _) = node_sockets(NodeKind::Output, &params);
        let ids: Vec<&str> = inputs.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["intensity", "color", "position", "strobe", "zoom", "gobo"]);
    }

    #[test]
    fn socket_types_convert_only_where_it_makes_sense() {
        assert!(SocketType::Number.feeds(SocketType::Color));
        assert!(SocketType::Trigger.feeds(SocketType::Number));
        assert!(!SocketType::Color.feeds(SocketType::Position));
        assert!(!SocketType::Position.feeds(SocketType::Number));
    }
}
