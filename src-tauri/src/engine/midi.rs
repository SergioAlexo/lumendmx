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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundControl {
    /// Follows the value: a CC's position, a note's velocity (0 when released).
    Fader,
    /// On while the note is held (or the CC is at 64 or above).
    Flash,
    /// Flips on each press.
    Toggle,
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
}

#[derive(Debug, Default)]
pub struct MidiState {
    /// Recent channel messages, oldest first.
    pub log: VecDeque<MidiEvent>,
    seq: u64,
    /// From the compiled graph.
    pub bindings: Vec<MidiBinding>,
    /// What taps the tempo, from the timing settings.
    pub tap: Option<MidiTrigger>,
    /// Notes / controllers currently held down: (message, channel 1-16, number).
    held: HashSet<(MidiMessage, u8, u8)>,
    /// Flash buttons pressed since the last tick. The tick sees them on even
    /// if a quick pad hit was already released, so no press is missed.
    pressed: HashSet<Arc<str>>,
}

impl MidiState {
    /// A message from the MIDI port `port` (its id): logs it and moves any
    /// control bound to it in `controls` (playback-bar values by node id).
    /// Returns whether it was a press of the tap trigger.
    pub fn feed(&mut self, port: &str, msg: &[u8], controls: &mut HashMap<String, f32>) -> bool {
        self.record(port, msg);
        let [status, number, value] = match msg {
            [s, n, v, ..] => [*s, *n & 0x7f, *v & 0x7f],
            _ => return false,
        };
        let channel = (status & 0x0f) + 1;
        let (message, on) = match status & 0xf0 {
            0x90 if value > 0 => (MidiMessage::Note, true),
            0x80 | 0x90 => (MidiMessage::Note, false),
            0xb0 => (MidiMessage::Cc, value >= 64),
            _ => return false,
        };
        let key = (message, channel, number);
        let press = on && self.held.insert(key);
        if !on {
            self.held.remove(&key);
        }
        // A binding's channel 0 listens on every channel.
        let matches = |m: MidiMessage, ch: u8, n: u8| m == message && n == number && (ch == 0 || ch == channel);
        let level = if on || message == MidiMessage::Cc { value as f32 / 127.0 } else { 0.0 };
        for b in &self.bindings {
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
                BoundControl::Toggle => {}
            }
        }
        press && self.tap.is_some_and(|t| matches(t.message, t.channel, t.number))
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
        MidiBinding { node: node.into(), message, channel, number, control }
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
        assert!(midi.feed("test", &[0xb3, 20, 127], &mut controls));
        assert!(!midi.feed("test", &[0xb3, 20, 100], &mut controls), "still held");
        assert!(!midi.feed("test", &[0xb3, 20, 0], &mut controls));
        assert!(midi.feed("test", &[0xb3, 20, 127], &mut controls));
        assert!(!midi.feed("test", &[0xb3, 21, 127], &mut controls), "another controller");
    }
}
