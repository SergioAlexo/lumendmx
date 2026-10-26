//! Every node type: its sockets, parameters and documentation. The editor
//! UI and MCP clients both render and describe nodes from this one table
//! (`node_types`), so adding a node type here is all it takes to expose it.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::model::ParamValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum NodeKind {
    Number,
    Color,
    Time,
    Lfo,
    Audio,
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
    Any,
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
    Frame,
}

impl NodeKind {
    pub const ALL: [NodeKind; 29] = [
        NodeKind::Number,
        NodeKind::Color,
        NodeKind::Time,
        NodeKind::Lfo,
        NodeKind::Audio,
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
        NodeKind::Any,
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
        NodeKind::Frame,
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
    /// A drawn wave: points `"x:y, x:y"`, each 0-1.
    Curve,
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
    /// "Input", "Fixtures", "Math", "Colour", "Position", "Switching", "Output", "Layout".
    pub category: String,
    pub description: String,
    /// Sockets with default params. Switch, Mix, Any and Output change
    /// theirs with their params; see the graph's per-node sockets.
    pub inputs: Vec<SocketInfo>,
    pub outputs: Vec<SocketInfo>,
    pub params: Vec<ParamInfo>,
}

pub const WAVEFORMS: [&str; 8] = ["sine", "square", "saw", "ramp", "random", "step", "pulse", "custom"];
pub const CURVE_MODES: [&str; 3] = ["linear", "smooth", "step"];
/// A Custom LFO's wave until it's drawn: a triangle.
pub const DEFAULT_CURVE: &str = "0:0, 0.5:1, 1:0";
pub const SHAPES: [&str; 5] = ["circle", "figure8", "ballyhoo", "panSaw", "tiltSaw"];
pub const MATH_OPS: [&str; 10] = ["add", "subtract", "multiply", "divide", "min", "max", "power", "modulo", "invert", "abs"];
pub const COMPARE_OPS: [&str; 3] = ["greater", "less", "equal"];
pub const VALUE_TYPES: [&str; 3] = ["number", "color", "position"];
pub const FRAME_COLORS: [&str; 7] = ["gray", "red", "orange", "yellow", "green", "blue", "purple"];
/// Most inputs a Switch can have.
pub const MAX_SWITCH_INPUTS: usize = 8;
/// Most values a Switch can pass through side by side.
pub const MAX_SWITCH_CHANNELS: usize = 8;
/// Most inputs an Any node grows to.
pub const MAX_ANY_INPUTS: usize = 64;
/// Most outputs any node has.
pub const MAX_OUTPUTS: usize = 8;

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

/// MIDI messages a Button or Fader can be bound to.
pub const MIDI_BINDINGS: [&str; 3] = ["none", "note", "cc"];

/// A Button's or Fader's MIDI binding: off until chosen (or learned).
fn midi_params() -> Vec<ParamInfo> {
    vec![
        choice("midi", "MIDI", &MIDI_BINDINGS, "none"),
        number("midiChannel", "MIDI channel", 0.0, 16.0, 1.0, 0.0),
        number("midiNumber", "MIDI number", 0.0, 127.0, 1.0, 0.0),
    ]
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

/// A Switch's settings: how many inputs and channels, and each channel's
/// type and name (channel 1 is "type"/"label", the rest "type1"/"label1"...).
fn switch_params() -> Vec<ParamInfo> {
    let mut params = vec![number("count", "Inputs", 2.0, MAX_SWITCH_INPUTS as f64, 1.0, 2.0), number("channels", "Channels", 1.0, MAX_SWITCH_CHANNELS as f64, 1.0, 1.0)];
    for c in 0..MAX_SWITCH_CHANNELS {
        let (type_id, label_id) = if c == 0 { ("type".to_string(), "label".to_string()) } else { (format!("type{c}"), format!("label{c}")) };
        params.push(choice(&type_id, &format!("Type {}", c + 1), &VALUE_TYPES, if c == 0 { "color" } else { "number" }));
        params.push(text_param(&label_id, &format!("Name {}", c + 1), &format!("Channel {}, shown on its sockets", c + 1), ""));
    }
    params
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
            "A repeating wave from 0 to 1, at a rate in cycles per beat (follows the BPM) or per second. Phase shifts it, in cycles: feed a Fan into it to make the wave travel across fixtures. Wave Custom plays the drawn Curve: points \"x:y\" (x across one cycle, y the value, both 0-1), joined by Between points.",
            vec![input("phase", "Phase", Number, n(0.0))],
            vec![output("value", "Value", Number)],
            [
                vec![choice("waveform", "Wave", &WAVEFORMS, "sine")],
                rate_params(),
                vec![
                    number("width", "Width %", 1.0, 100.0, 1.0, 50.0),
                    ParamInfo { id: "curve".into(), label: "Curve".into(), kind: ParamKind::Curve, default: text(DEFAULT_CURVE) },
                    choice("curveMode", "Between points", &CURVE_MODES, "linear"),
                ],
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
        NodeKind::Fader => (
            "Fader",
            "Input",
            "A fader on the playback bar, 0-1. Bind it to a knob or fader on a MIDI controller (a CC) and the MIDI moves it.",
            vec![],
            vec![output("value", "Value", Number)],
            [vec![text_param("label", "Label", "Shown on the playback bar", "Fader")], midi_params()].concat(),
        ),
        NodeKind::Button => (
            "Button",
            "Input",
            "A button on the playback bar: on while held (flash) or on/off per press (toggle). Bind it to a pad or key on a MIDI controller (a note, or a CC that's on at 64 and above) and the MIDI presses it.",
            vec![],
            vec![output("gate", "Gate", Trigger)],
            [
                vec![text_param("label", "Label", "Shown on the playback bar", "Button"), choice("mode", "Mode", &["flash", "toggle"], "flash")],
                midi_params(),
            ]
            .concat(),
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
        NodeKind::Any => (
            "Any",
            "Math",
            "On while any of its inputs is on: say two Buttons driving the same thing. There's always a free input at the bottom; connect to it and another appears.",
            vec![],
            vec![],
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
            "Picks a colour along a row of colour stops; Position 0 is the start, 1 the end. Stops are \"#rrggbb@place\" (place 0-1) and spread evenly where they have no place; wrapped, the last blends round to the first.",
            vec![input("t", "Position", Number, n(0.0))],
            vec![output("color", "Color", Color)],
            vec![text_param("stops", "Stops", "Colours with their place, comma-separated: #ff0000@0, #00ff00@0.5", "#ff0000, #ffff00, #00ff00, #00ffff, #0000ff, #ff00ff"), bool_param("wrap", "Wrap around", true)],
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
            "Passes one of its inputs through: Index 0 picks the first, 1 the second ... Drive Index from a Counter or a Fader to change looks live. With more than one Channel, every input is a row of values (say intensity and colour) and the whole row switches together.",
            vec![],
            vec![],
            switch_params(),
        ),
        NodeKind::Mix => (
            "Mix",
            "Switching",
            "Crossfades from A (Amount 0) to B (Amount 1).",
            vec![],
            vec![],
            vec![choice("type", "Type", &VALUE_TYPES, "number")],
        ),
        NodeKind::Frame => (
            "Frame",
            "Layout",
            "A labelled box to group nodes visually; it does nothing to the output. Dragging it moves the nodes inside it. Select nodes and press Cmd/Ctrl+J to frame them. Collapsed, it hides the nodes inside and shows the sockets they expose, under the names given, so a frame works like a node of your own. A node inside exposes sockets with its \"exposed\" param, JSON from socket to name: {\"in:rate\": \"Speed\", \"out:value\": \"\"} (empty: the socket's own name).",
            vec![],
            vec![],
            vec![
                text_param("label", "Label", "Shown at the top", "Frame"),
                choice("color", "Color", &FRAME_COLORS, "gray"),
                number("width", "Width", 100.0, 10000.0, 1.0, 400.0),
                number("height", "Height", 80.0, 10000.0, 1.0, 300.0),
                bool_param("collapsed", "Collapsed", false),
            ],
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

/// How many values a Switch passes side by side.
pub fn switch_channels(params: &BTreeMap<String, ParamValue>) -> usize {
    params.get("channels").and_then(ParamValue::as_f32).unwrap_or(1.0).clamp(1.0, MAX_SWITCH_CHANNELS as f32) as usize
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
        .filter(|s| !s.is_empty() && s != ENABLED && !OUTPUT_SOCKETS.iter().any(|(id, _, _)| id == s))
        .collect()
}

/// How many inputs an Any node has: its "count" param, which the graph
/// edits keep at one more than the last one connected.
pub fn any_inputs(params: &BTreeMap<String, ParamValue>) -> usize {
    params.get("count").and_then(ParamValue::as_f32).unwrap_or(2.0).clamp(2.0, MAX_ANY_INPUTS as f32) as usize
}

/// Sockets of the node types whose sockets depend on their params; empty
/// for the rest (use `node_type`).
pub fn sockets(kind: NodeKind, params: &BTreeMap<String, ParamValue>) -> (Vec<SocketInfo>, Vec<SocketInfo>) {
    match kind {
        NodeKind::Switch => {
            let count = params.get("count").and_then(ParamValue::as_f32).unwrap_or(2.0).clamp(2.0, MAX_SWITCH_INPUTS as f32) as usize;
            let channels = switch_channels(params);
            // Channel 0 keeps the plain ids (in0, value), the others add a suffix.
            let channel: Vec<(SocketType, String)> = (0..channels)
                .map(|c| {
                    let (type_key, label_key, default) = if c == 0 { ("type".to_string(), "label".to_string(), "color") } else { (format!("type{c}"), format!("label{c}"), "number") };
                    let type_name = param_text(params, &type_key, default);
                    let name = param_text(params, &label_key, "").trim();
                    let name = if name.is_empty() { type_name[..1].to_uppercase() + &type_name[1..] } else { name.to_string() };
                    (value_type(type_name), name)
                })
                .collect();
            let mut inputs = vec![input("index", "Index", SocketType::Number, n(0.0))];
            for i in 0..count {
                for (c, (ty, name)) in channel.iter().enumerate() {
                    let id = if c == 0 { format!("in{i}") } else { format!("in{i}_{c}") };
                    let label = if channels == 1 { format!("{i}") } else { format!("{name} {i}") };
                    inputs.push(input(&id, &label, *ty, value_default(*ty)));
                }
            }
            let outputs = channel
                .iter()
                .enumerate()
                .map(|(c, (ty, name))| output(&if c == 0 { "value".to_string() } else { format!("value{c}") }, if channels == 1 { "Value" } else { name }, *ty))
                .collect();
            (inputs, outputs)
        }
        NodeKind::Any => {
            let inputs = (0..any_inputs(params)).map(|i| input_only(&format!("in{i}"), &format!("{}", i + 1), SocketType::Trigger)).collect();
            (inputs, vec![output("gate", "Gate", SocketType::Trigger)])
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

/// The input every node has, last: while it's off, whatever the node feeds
/// behaves as if the link wasn't there.
pub const ENABLED: &str = "enabled";

/// A node's actual sockets, given its params, Enabled included.
pub fn node_sockets(kind: NodeKind, params: &BTreeMap<String, ParamValue>) -> (Vec<SocketInfo>, Vec<SocketInfo>) {
    let (mut inputs, outputs) = sockets(kind, params);
    let outputs = if inputs.is_empty() && outputs.is_empty() {
        let info = node_type(kind);
        inputs = info.inputs;
        info.outputs
    } else {
        outputs
    };
    // A Frame isn't evaluated, so there's nothing to disable.
    if kind != NodeKind::Frame {
        inputs.push(input(ENABLED, "Enabled", SocketType::Trigger, n(1.0)));
    }
    (inputs, outputs)
}

pub fn node_types() -> Vec<NodeTypeInfo> {
    NodeKind::ALL.iter().map(|&k| node_type(k)).collect()
}

/// docs/NODES.md: the node reference, rendered from this registry.
#[cfg(test)]
fn nodes_markdown() -> String {
    let value = |v: &ParamValue| match v {
        ParamValue::Bool(b) => b.to_string(),
        ParamValue::Number(n) => n.to_string(),
        ParamValue::Text(t) if t.is_empty() => "(empty)".to_string(),
        ParamValue::Text(t) => format!("`{t}`"),
    };
    let ty = |t: SocketType| format!("{t:?}").to_lowercase();
    let mut out = String::from(
        "# Node reference\n\nGenerated from `src-tauri/src/graph/registry.rs` by `cargo test` (run `UPDATE_DOCS=1 cargo test nodes_doc` after changing a node). \
         Values are 0-1 unless noted. An unconnected input takes its value from a param with the input's id, else the default shown. \
         Every node also has an `enabled` input (trigger, default on, not listed below): while it's off, everything the node feeds \
         behaves as if the link wasn't there.\n",
    );
    let mut category = String::new();
    for info in node_types() {
        if info.category != category {
            category = info.category.clone();
            out.push_str(&format!("\n## {category}\n"));
        }
        out.push_str(&format!("\n### {} (`{}`)\n\n{}\n", info.label, serde_json::to_value(info.kind).unwrap().as_str().unwrap(), info.description));
        let sockets = |label: &str, list: &[SocketInfo], out: &mut String| {
            if list.is_empty() {
                return;
            }
            out.push_str(&format!("\n{label}:\n"));
            for s in list {
                let default = s.default.as_ref().map(|d| format!(", default {}", value(d))).unwrap_or_default();
                out.push_str(&format!("- `{}` {} ({}{default})\n", s.id, s.label, ty(s.ty)));
            }
        };
        sockets("Inputs", &info.inputs, &mut out);
        sockets("Outputs", &info.outputs, &mut out);
        if !info.params.is_empty() {
            out.push_str("\nParams:\n");
            for p in &info.params {
                let kind = match &p.kind {
                    ParamKind::Number { min, max, .. } => format!("number {min} to {max}"),
                    ParamKind::Bool => "on/off".to_string(),
                    ParamKind::Text { hint } => format!("text: {hint}"),
                    ParamKind::Choice { options } => options.join(" / "),
                    ParamKind::Color => "colour".to_string(),
                    ParamKind::Group => "group id (0 = none)".to_string(),
                    ParamKind::Curve => "points `x:y`, comma-separated".to_string(),
                };
                out.push_str(&format!("- `{}` {}: {kind}, default {}\n", p.id, p.label, value(&p.default)));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_doc_is_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/NODES.md");
        let rendered = nodes_markdown();
        if std::env::var("UPDATE_DOCS").is_ok() {
            std::fs::write(&path, &rendered).unwrap();
        }
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(current == rendered, "docs/NODES.md is out of date: run `UPDATE_DOCS=1 cargo test nodes_doc`");
    }

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
        assert_eq!(inputs.len(), 5);
        assert_eq!(outputs[0].ty, SocketType::Position);

        // Two channels: every input becomes a row of an intensity and a colour.
        let mut params = BTreeMap::new();
        params.insert("type".to_string(), text("number"));
        params.insert("type1".to_string(), text("color"));
        params.insert("label".to_string(), text("Intensity"));
        params.insert("count".to_string(), n(3.0));
        params.insert("channels".to_string(), n(2.0));
        let (inputs, outputs) = node_sockets(NodeKind::Switch, &params);
        let ids: Vec<&str> = inputs.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["index", "in0", "in0_1", "in1", "in1_1", "in2", "in2_1", "enabled"]);
        assert_eq!(inputs[1].label, "Intensity 0");
        assert_eq!(inputs[2].ty, SocketType::Color);
        assert_eq!(outputs.iter().map(|s| (s.id.as_str(), s.ty)).collect::<Vec<_>>(), vec![("value", SocketType::Number), ("value1", SocketType::Color)]);

        let mut params = BTreeMap::new();
        params.insert("attributes".to_string(), text("Zoom, gobo, color"));
        let (inputs, _) = node_sockets(NodeKind::Output, &params);
        let ids: Vec<&str> = inputs.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["intensity", "color", "position", "strobe", "zoom", "gobo", "enabled"]);
    }

    #[test]
    fn socket_types_convert_only_where_it_makes_sense() {
        assert!(SocketType::Number.feeds(SocketType::Color));
        assert!(SocketType::Trigger.feeds(SocketType::Number));
        assert!(!SocketType::Color.feeds(SocketType::Position));
        assert!(!SocketType::Position.feeds(SocketType::Number));
    }
}
