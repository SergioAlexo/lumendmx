//! MIDI from the controller ports: a log of recent messages for the Setup
//! monitor and MIDI Learn, and the Button / Fader bindings. A bound control
//! moves its playback-bar value, so the graph (and the bar) see a MIDI pad
//! exactly like a click.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// How many recent messages the MIDI monitor keeps.
pub const MIDI_LOG_LEN: usize = 64;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum MidiMessage {
    Note,
    Cc,
}

/// A pad or button on a MIDI controller: a note, or a CC that's pressed at
/// 64 and above.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct MidiTrigger {
    pub message: MidiMessage,
    /// 1-16, or 0 for any channel.
    pub channel: u8,
    pub number: u8,
}

/// What a bound control does with its MIDI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoundControl {
    /// Follows the value: a CC's position, a note's velocity (0 when released).
    Fader,
    /// On while the note is held (or the CC is at 64 or above).
    Flash,
    /// Flips on each press.
    Toggle,
    /// A Fader's step button: while held, moves the value by this much per second
    /// (negative for the down button). Lets a controller with only buttons drive a fader.
    Nudge(f32),
    /// A Fader's sweep pad: while held, moves the value by this much per second, in a
    /// direction that flips with each press. One pad does what Up and Down do.
    Sweep(f32),
}

/// With speed following velocity, the slowest a very soft hit goes, as a share of full speed.
const MIN_VELOCITY_SPEED: f32 = 0.1;

/// The control id the grand master goes by in the controls map, so the fader bindings can move it.
pub const GM_KEY: &str = "@gm";

/// How long the grand master takes to cross its range under a held Up, Down or Sweep pad, unless set.
const GM_TRAVEL_S: f32 = 2.0;

/// The MIDI bindings of the grand master, the same four a Fader has.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct GrandMasterMidi {
    /// A knob or fader (a CC) that sets it.
    pub knob: Option<MidiTrigger>,
    /// Buttons that move it while held.
    pub up: Option<MidiTrigger>,
    pub down: Option<MidiTrigger>,
    /// One pad that moves it while held, the opposite way each press.
    pub sweep: Option<MidiTrigger>,
    /// Seconds for Up, Down and Sweep to cross the whole range (at full velocity, if that is on).
    pub travel: f32,
    /// A harder hit on those pads moves it faster.
    pub velocity: bool,
}

impl Default for GrandMasterMidi {
    fn default() -> Self {
        Self { knob: None, up: None, down: None, sweep: None, travel: GM_TRAVEL_S, velocity: false }
    }
}

impl GrandMasterMidi {
    pub fn bindings(&self) -> Vec<MidiBinding> {
        let rate = 1.0 / self.travel.max(0.1);
        [(self.knob, BoundControl::Fader), (self.up, BoundControl::Nudge(rate)), (self.down, BoundControl::Nudge(-rate)), (self.sweep, BoundControl::Sweep(rate))]
            .into_iter()
            .filter_map(|(trigger, control)| {
                let t = trigger?;
                Some(MidiBinding { node: Arc::from(GM_KEY), message: t.message, channel: t.channel, number: t.number, control, velocity: self.velocity })
            })
            .collect()
    }
}

/// A Button or Fader node listening to one note or controller.
#[derive(Debug, Clone, PartialEq)]
pub struct MidiBinding {
    pub node: Arc<str>,
    pub message: MidiMessage,
    /// 1-16, or 0 for any channel.
    pub channel: u8,
    pub number: u8,
    pub control: BoundControl,
    /// For Nudge and Sweep: scale the speed by how hard the pad was hit.
    pub velocity: bool,
}

/// How a MIDI pad drives the blackout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum BlackoutMode {
    /// Each press flips it.
    #[default]
    Toggle,
    /// On while the pad is held.
    Flash,
}

/// What a message did to the blackout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlackoutAction {
    Toggle,
    On,
    Off,
}

/// Which of the global triggers a message pressed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pressed {
    pub tap: bool,
    pub blackout: Option<BlackoutAction>,
}

#[derive(Debug, Default)]
pub struct MidiState {
    /// Recent channel messages, oldest first.
    pub log: VecDeque<MidiEvent>,
    seq: u64,
    /// From the compiled graph.
    pub bindings: Vec<MidiBinding>,
    /// The grand master's, from the timing settings.
    pub gm_bindings: Vec<MidiBinding>,
    /// What taps the tempo, from the timing settings.
    pub tap: Option<MidiTrigger>,
    /// What toggles the blackout, from the timing settings.
    pub blackout: Option<MidiTrigger>,
    pub blackout_mode: BlackoutMode,
    /// Which way each sweep pad's fader moved on its last press (+1 up, -1 down).
    sweep_dir: HashMap<Arc<str>, f32>,
    /// Notes / controllers currently held down: (message, channel 1-16, number), with how hard
    /// each was hit (0-1).
    held: HashMap<(MidiMessage, u8, u8), f32>,
    /// Flash buttons pressed since the last tick. The tick sees them on even
    /// if a quick pad hit was already released, so no press is missed.
    pressed: HashSet<Arc<str>>,
}

impl MidiState {
    /// A message from the MIDI port `port` (its id): logs it and moves any
    /// control bound to it in `controls` (playback-bar values by node id).
    /// Returns which global trigger (tap, blackout) it pressed, if any.
    pub fn feed(&mut self, port: &str, msg: &[u8], controls: &mut HashMap<String, f32>) -> Pressed {
        self.record(port, msg);
        let [status, number, value] = match msg {
            [s, n, v, ..] => [*s, *n & 0x7f, *v & 0x7f],
            _ => return Pressed::default(),
        };
        let channel = (status & 0x0f) + 1;
        let (message, on) = match status & 0xf0 {
            0x90 if value > 0 => (MidiMessage::Note, true),
            0x80 | 0x90 => (MidiMessage::Note, false),
            0xb0 => (MidiMessage::Cc, value >= 64),
            _ => return Pressed::default(),
        };
        let key = (message, channel, number);
        let press = on && !self.held.contains_key(&key);
        if press {
            self.held.insert(key, value as f32 / 127.0);
        }
        let release = !on && self.held.remove(&key).is_some();
        // A binding's channel 0 listens on every channel.
        let matches = |m: MidiMessage, ch: u8, n: u8| m == message && n == number && (ch == 0 || ch == channel);
        let level = if on || message == MidiMessage::Cc { value as f32 / 127.0 } else { 0.0 };
        for b in self.bindings.iter().chain(&self.gm_bindings) {
            if !matches(b.message, b.channel, b.number) {
                continue;
            }
            let id = b.node.to_string();
            match b.control {
                BoundControl::Fader => {
                    controls.insert(id, level);
                }
                BoundControl::Flash => {
                    controls.insert(id, if on { 1.0 } else { 0.0 });
                    if press {
                        self.pressed.insert(b.node.clone());
                    }
                }
                BoundControl::Toggle if press => {
                    let v = controls.entry(id).or_insert(0.0);
                    *v = if *v > 0.5 { 0.0 } else { 1.0 };
                }
                BoundControl::Sweep(_) if press => {
                    // Away from an end it is at, else the opposite of last time (up the first time).
                    let v = controls.get(&id).copied().unwrap_or(0.0);
                    let last = self.sweep_dir.get(&b.node).copied().unwrap_or(-1.0);
                    let dir = if v >= 1.0 {
                        -1.0
                    } else if v <= 0.0 {
                        1.0
                    } else {
                        -last
                    };
                    self.sweep_dir.insert(b.node.clone(), dir);
                }
                BoundControl::Toggle | BoundControl::Nudge(_) | BoundControl::Sweep(_) => {}
            }
        }
        let hit = |edge: bool, trigger: Option<MidiTrigger>| edge && trigger.is_some_and(|t| matches(t.message, t.channel, t.number));
        let blackout = match self.blackout_mode {
            BlackoutMode::Toggle => hit(press, self.blackout).then_some(BlackoutAction::Toggle),
            BlackoutMode::Flash if hit(press, self.blackout) => Some(BlackoutAction::On),
            BlackoutMode::Flash => hit(release, self.blackout).then_some(BlackoutAction::Off),
        };
        Pressed { tap: hit(press, self.tap), blackout }
    }

    /// Moves every Fader whose step or sweep pad is held. Called each tick.
    pub fn advance_held(&self, dt_ms: f32, controls: &mut HashMap<String, f32>) {
        for b in self.bindings.iter().chain(&self.gm_bindings) {
            let per_second = match b.control {
                BoundControl::Nudge(per_second) => per_second,
                BoundControl::Sweep(rate) => rate * self.sweep_dir.get(&b.node).copied().unwrap_or(1.0),
                _ => continue,
            };
            let hit = self.held.iter().filter(|(&(m, ch, n), _)| m == b.message && n == b.number && (b.channel == 0 || b.channel == ch)).map(|(_, &v)| v).reduce(f32::max);
            if let Some(hit) = hit {
                // A soft hit is slow but still moves.
                let scale = if b.velocity { hit.max(MIN_VELOCITY_SPEED) } else { 1.0 };
                let v = controls.entry(b.node.to_string()).or_insert(0.0);
                *v = (*v + per_second * scale * dt_ms / 1000.0).clamp(0.0, 1.0);
            }
        }
    }

    /// Flash buttons pressed since the last call.
    pub fn take_pressed(&mut self) -> HashSet<Arc<str>> {
        std::mem::take(&mut self.pressed)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(node: &str, message: MidiMessage, channel: u8, number: u8, control: BoundControl) -> MidiBinding {
        MidiBinding { node: node.into(), message, channel, number, control, velocity: false }
    }

    #[test]
    fn the_log_keeps_channel_messages_and_skips_clock() {
        let mut midi = MidiState::default();
        let mut controls = HashMap::new();
        midi.feed("test", &[0xf8], &mut controls); // clock
        midi.feed("test", &[0xb1, 7, 100], &mut controls);
        midi.feed("test", &[0x92, 36, 0], &mut controls); // note on at velocity 0 is a note off
        midi.feed("test", &[0xe0, 0x00, 0x40], &mut controls);
        let kinds: Vec<_> = midi.since(0).iter().map(|e| (e.kind, e.channel, e.number, e.value)).collect();
        assert_eq!(
            kinds,
            vec![(MidiEventKind::Cc, 2, 7, 100), (MidiEventKind::NoteOff, 3, 36, 0), (MidiEventKind::PitchBend, 1, 0, 8192)]
        );
        assert_eq!(midi.since(2).len(), 1);
        for _ in 0..100 {
            midi.feed("test", &[0xb0, 1, 1], &mut controls);
        }
        assert_eq!(midi.log.len(), MIDI_LOG_LEN);
    }

    #[test]
    fn bindings_move_faders_and_press_buttons() {
        let mut midi = MidiState::default();
        midi.bindings = vec![
            binding("fader", MidiMessage::Cc, 0, 7, BoundControl::Fader),
            binding("flash", MidiMessage::Note, 2, 36, BoundControl::Flash),
            binding("toggle", MidiMessage::Note, 0, 40, BoundControl::Toggle),
        ];
        let mut controls = HashMap::new();

        midi.feed("test", &[0xb5, 7, 127], &mut controls); // any channel
        assert_eq!(controls["fader"], 1.0);

        midi.feed("test", &[0x90, 36, 100], &mut controls); // channel 1: not bound
        assert!(!controls.contains_key("flash"));
        midi.feed("test", &[0x91, 36, 100], &mut controls);
        assert_eq!(controls["flash"], 1.0);
        midi.feed("test", &[0x81, 36, 0], &mut controls);
        assert_eq!(controls["flash"], 0.0);
        assert_eq!(midi.take_pressed().len(), 1, "a quick hit is still seen by the next tick");
        assert!(midi.take_pressed().is_empty());

        midi.feed("test", &[0x90, 40, 100], &mut controls);
        midi.feed("test", &[0x90, 40, 100], &mut controls); // no note off in between: not a new press
        assert_eq!(controls["toggle"], 1.0);
        midi.feed("test", &[0x80, 40, 0], &mut controls);
        midi.feed("test", &[0x90, 40, 100], &mut controls);
        assert_eq!(controls["toggle"], 0.0);
    }

    #[test]
    fn the_tap_trigger_fires_once_per_press() {
        let mut midi = MidiState { tap: Some(MidiTrigger { message: MidiMessage::Cc, channel: 0, number: 20 }), ..Default::default() };
        let mut controls = HashMap::new();
        assert!(midi.feed("test", &[0xb3, 20, 127], &mut controls).tap);
        assert!(!midi.feed("test", &[0xb3, 20, 100], &mut controls).tap, "still held");
        assert!(!midi.feed("test", &[0xb3, 20, 0], &mut controls).tap);
        assert!(midi.feed("test", &[0xb3, 20, 127], &mut controls).tap);
        assert!(!midi.feed("test", &[0xb3, 21, 127], &mut controls).tap, "another controller");
    }

    #[test]
    fn a_held_step_button_ramps_its_fader() {
        let bind = |number, per_second| MidiBinding {
            node: "f".into(),
            message: MidiMessage::Note,
            channel: 0,
            number,
            control: BoundControl::Nudge(per_second),
            velocity: false,
        };
        let mut midi = MidiState { bindings: vec![bind(60, 0.5), bind(61, -0.5)], ..Default::default() };
        let mut controls = HashMap::new();
        midi.advance_held(500.0, &mut controls);
        assert!(controls.is_empty(), "nothing held");
        midi.feed("test", &[0x90, 60, 100], &mut controls);
        midi.advance_held(500.0, &mut controls);
        assert_eq!(controls["f"], 0.25);
        midi.feed("test", &[0x80, 60, 0], &mut controls);
        midi.advance_held(500.0, &mut controls);
        assert_eq!(controls["f"], 0.25, "released");
        midi.feed("test", &[0x91, 61, 100], &mut controls); // down, on another channel
        midi.advance_held(2000.0, &mut controls);
        assert_eq!(controls["f"], 0.0, "stops at the bottom");
    }

    #[test]
    fn a_sweep_pad_alternates_direction_and_flips_at_the_ends() {
        let mut midi = MidiState {
            bindings: vec![MidiBinding { node: "f".into(), message: MidiMessage::Note, channel: 0, number: 60, control: BoundControl::Sweep(1.0), velocity: false }],
            ..Default::default()
        };
        let mut controls = HashMap::new();
        let hold = |midi: &mut MidiState, controls: &mut HashMap<String, f32>, ms: f32| {
            midi.feed("test", &[0x90, 60, 100], controls);
            midi.advance_held(ms, controls);
            midi.feed("test", &[0x80, 60, 0], controls);
        };
        hold(&mut midi, &mut controls, 500.0);
        assert_eq!(controls["f"], 0.5, "the first press goes up");
        hold(&mut midi, &mut controls, 200.0);
        assert_eq!(controls["f"], 0.3, "the next goes down");
        hold(&mut midi, &mut controls, 100.0);
        assert!((controls["f"] - 0.4).abs() < 1e-6, "and the next up again");
        hold(&mut midi, &mut controls, 5000.0);
        assert_eq!(controls["f"], 0.0, "then down, stopping at the bottom");
        hold(&mut midi, &mut controls, 5000.0);
        assert_eq!(controls["f"], 1.0, "at the bottom the next press goes up, stopping at the top");
        hold(&mut midi, &mut controls, 300.0);
        assert!((controls["f"] - 0.7).abs() < 1e-6, "at the top the next press goes down");
        midi.advance_held(500.0, &mut controls);
        assert!((controls["f"] - 0.7).abs() < 1e-6, "released");
    }

    #[test]
    fn the_grand_master_takes_the_same_bindings_as_a_fader() {
        let trigger = |number| Some(MidiTrigger { message: MidiMessage::Note, channel: 0, number });
        let gm = GrandMasterMidi { up: trigger(60), down: trigger(61), ..Default::default() };
        let mut midi = MidiState { gm_bindings: gm.bindings(), ..Default::default() };
        let mut controls = HashMap::from([(GM_KEY.to_string(), 0.5)]);
        midi.feed("test", &[0x90, 60, 100], &mut controls);
        midi.advance_held(1000.0, &mut controls);
        assert_eq!(controls[GM_KEY], 1.0, "up for a second: half the range in two seconds, clamped");
        midi.feed("test", &[0x80, 60, 0], &mut controls);
        midi.feed("test", &[0x90, 61, 100], &mut controls);
        midi.advance_held(1000.0, &mut controls);
        assert_eq!(controls[GM_KEY], 0.5);

        // A longer travel is slower.
        let slow = GrandMasterMidi { up: trigger(60), travel: 10.0, ..Default::default() };
        let mut midi = MidiState { gm_bindings: slow.bindings(), ..Default::default() };
        let mut controls = HashMap::from([(GM_KEY.to_string(), 0.0)]);
        midi.feed("test", &[0x90, 60, 100], &mut controls);
        midi.advance_held(1000.0, &mut controls);
        assert!((controls[GM_KEY] - 0.1).abs() < 1e-6);
    }

    #[test]
    fn a_harder_hit_moves_a_velocity_pad_faster() {
        let pad = |velocity| MidiBinding {
            node: "f".into(),
            message: MidiMessage::Note,
            channel: 0,
            number: 60,
            control: BoundControl::Nudge(1.0),
            velocity,
        };
        let run = |velocity: bool, hit: u8| {
            let mut midi = MidiState { bindings: vec![pad(velocity)], ..Default::default() };
            let mut controls = HashMap::new();
            midi.feed("test", &[0x90, 60, hit], &mut controls);
            midi.advance_held(100.0, &mut controls);
            controls["f"]
        };
        assert!((run(true, 127) - 0.1).abs() < 1e-6, "full velocity: full speed");
        assert!((run(true, 64) - 0.1 * 64.0 / 127.0).abs() < 1e-6, "about half");
        assert!((run(true, 1) - 0.01).abs() < 1e-6, "a very soft hit still moves, at a tenth");
        assert!((run(false, 64) - 0.1).abs() < 1e-6, "off: velocity doesn't matter");
    }

    #[test]
    fn the_blackout_trigger_is_separate_from_tap() {
        let mut midi = MidiState {
            tap: Some(MidiTrigger { message: MidiMessage::Note, channel: 0, number: 36 }),
            blackout: Some(MidiTrigger { message: MidiMessage::Note, channel: 0, number: 37 }),
            ..Default::default()
        };
        let mut controls = HashMap::new();
        assert_eq!(midi.feed("test", &[0x90, 37, 100], &mut controls), Pressed { tap: false, blackout: Some(BlackoutAction::Toggle) });
        assert_eq!(midi.feed("test", &[0x90, 37, 100], &mut controls), Pressed::default(), "still held");
        assert_eq!(midi.feed("test", &[0x90, 36, 100], &mut controls), Pressed { tap: true, blackout: None });
    }

    #[test]
    fn a_flash_blackout_is_on_only_while_held() {
        let mut midi = MidiState {
            blackout: Some(MidiTrigger { message: MidiMessage::Note, channel: 0, number: 37 }),
            blackout_mode: BlackoutMode::Flash,
            ..Default::default()
        };
        let mut controls = HashMap::new();
        assert_eq!(midi.feed("test", &[0x90, 37, 100], &mut controls).blackout, Some(BlackoutAction::On));
        assert_eq!(midi.feed("test", &[0x90, 37, 100], &mut controls).blackout, None, "still held");
        assert_eq!(midi.feed("test", &[0x80, 37, 0], &mut controls).blackout, Some(BlackoutAction::Off));
        assert_eq!(midi.feed("test", &[0x80, 37, 0], &mut controls).blackout, None, "already released");
        assert_eq!(midi.feed("test", &[0x80, 38, 0], &mut controls).blackout, None, "another note");
    }
}
