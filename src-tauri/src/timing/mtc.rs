//! MIDI timing messages: MIDI clock (24 pulses per quarter note) -> BPM, and
//! MIDI Time Code (quarter-frame and full-frame messages) -> a position.

use std::collections::VecDeque;

use crate::engine::timecode::parts_to_ms;

const FPS_BY_CODE: [f32; 4] = [24.0, 25.0, 29.97, 30.0];

/// A decoded MTC position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MtcPosition {
    pub ms: f64,
    pub fps: f32,
}

#[derive(Debug, Default)]
pub struct MtcDecoder {
    pieces: [u8; 8],
    /// The next quarter-frame piece expected; `None` until a sequence
    /// has started from piece 0.
    next_piece: Option<u8>,
    last: Option<MtcPosition>,
}

impl MtcDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one raw MIDI message; returns the position it implies, if any.
    /// Quarter frames give a fresh position every quarter frame once a first
    /// full sequence has been seen.
    pub fn feed(&mut self, msg: &[u8]) -> Option<MtcPosition> {
        match msg {
            [0xF1, data] => self.quarter_frame(*data),
            // Full frame: F0 7F <device> 01 01 hh mm ss ff F7
            [0xF0, 0x7F, _, 0x01, 0x01, hh, mm, ss, ff, 0xF7] => {
                let fps = FPS_BY_CODE[((hh >> 5) & 3) as usize];
                let pos = MtcPosition { ms: parts_to_ms((hh & 0x1F) as u32, *mm as u32, *ss as u32, *ff as u32, fps), fps };
                self.next_piece = None;
                self.last = Some(pos);
                Some(pos)
            }
            _ => None,
        }
    }

    fn quarter_frame(&mut self, data: u8) -> Option<MtcPosition> {
        let piece = data >> 4;
        if piece > 7 {
            return None;
        }
        // Pieces must arrive in order; anything else restarts the sequence.
        match self.next_piece {
            Some(expected) if expected == piece => {}
            _ if piece == 0 => {}
            _ => {
                self.next_piece = None;
                return None;
            }
        }
        self.pieces[piece as usize] = data & 0x0F;
        if piece < 7 {
            self.next_piece = Some(piece + 1);
            // Mid-sequence: advance the last full position by a quarter frame.
            return self.last.as_mut().map(|last| {
                last.ms += 250.0 / last.fps as f64;
                *last
            });
        }

        self.next_piece = Some(0);
        let p = &self.pieces;
        let frames = (p[0] | (p[1] << 4)) as u32;
        let seconds = (p[2] | (p[3] << 4)) as u32;
        let minutes = (p[4] | (p[5] << 4)) as u32;
        let hours = (p[6] | ((p[7] & 1) << 4)) as u32;
        let fps = FPS_BY_CODE[((p[7] >> 1) & 3) as usize];
        // The time encoded is where piece 0 started; two frames have gone
        // by when piece 7 arrives.
        let ms = parts_to_ms(hours, minutes, seconds, frames, fps) + 2000.0 / fps as f64;
        let pos = MtcPosition { ms, fps };
        self.last = Some(pos);
        Some(pos)
    }
}

/// Tracks MIDI clock pulses (0xF8) and derives a BPM from their timestamps.
#[derive(Debug, Default)]
pub struct MidiClockTracker {
    /// Microsecond timestamps of the most recent pulses.
    pulses: VecDeque<u64>,
    since_estimate: u32,
}

const PPQN: usize = 24;
/// Two beats of pulses smooth out the jitter of a software clock.
const WINDOW: usize = PPQN * 2;

/// What a MIDI real-time message means for the tempo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClockEvent {
    Bpm(f32),
    /// Start or continue-from-start: the next pulse is beat 1.
    Start,
    Stop,
}

impl MidiClockTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, stamp_us: u64, msg: &[u8]) -> Option<ClockEvent> {
        match msg {
            [0xF8] => {
                if self.pulses.back().is_some_and(|&last| stamp_us <= last) {
                    return None;
                }
                self.pulses.push_back(stamp_us);
                if self.pulses.len() > WINDOW {
                    self.pulses.pop_front();
                }
                self.since_estimate += 1;
                if self.pulses.len() > PPQN / 2 && self.since_estimate >= 6 {
                    self.since_estimate = 0;
                    let span = (self.pulses.back()? - self.pulses.front()?) as f64;
                    let interval = span / (self.pulses.len() - 1) as f64;
                    return Some(ClockEvent::Bpm((60_000_000.0 / (interval * PPQN as f64)) as f32));
                }
                None
            }
            [0xFA] => {
                self.pulses.clear();
                self.since_estimate = 0;
                Some(ClockEvent::Start)
            }
            [0xFC] => Some(ClockEvent::Stop),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 8 quarter-frame messages for `hh:mm:ss:ff` at 25 fps.
    fn quarter_frames(h: u8, m: u8, s: u8, f: u8) -> Vec<[u8; 2]> {
        let rate = 1u8; // 25 fps
        let nibbles = [f & 0xF, f >> 4, s & 0xF, s >> 4, m & 0xF, m >> 4, h & 0xF, (h >> 4) | (rate << 1)];
        nibbles.iter().enumerate().map(|(i, n)| [0xF1, ((i as u8) << 4) | n]).collect()
    }

    #[test]
    fn eight_quarter_frames_decode_to_a_position_two_frames_on() {
        let mut dec = MtcDecoder::new();
        let mut last = None;
        for qf in quarter_frames(1, 2, 3, 10) {
            last = dec.feed(&qf).or(last);
        }
        let pos = last.unwrap();
        assert_eq!(pos.fps, 25.0);
        // 01:02:03:10 + 2 frames = 01:02:03:12
        assert!((pos.ms - parts_to_ms(1, 2, 3, 12, 25.0)).abs() < 1e-6, "{}", pos.ms);
    }

    #[test]
    fn position_advances_a_quarter_frame_per_message_while_running() {
        let mut dec = MtcDecoder::new();
        let mut positions = Vec::new();
        // Two full sequences, 2 frames apart (as a 25fps MTC stream sends them).
        for seq in [quarter_frames(0, 0, 1, 0), quarter_frames(0, 0, 1, 2)] {
            for qf in seq {
                if let Some(p) = dec.feed(&qf) {
                    positions.push(p.ms);
                }
            }
        }
        // First sequence only completes at its last message; the second then
        // yields one position per quarter frame, each 10ms (1/4 of 40ms) on,
        // landing exactly on the completed time.
        assert_eq!(positions.len(), 9);
        for pair in positions[..8].windows(2) {
            assert!((pair[1] - pair[0] - 10.0).abs() < 1e-6);
        }
        assert!((positions[8] - parts_to_ms(0, 0, 1, 4, 25.0)).abs() < 1e-6);
    }

    #[test]
    fn out_of_order_quarter_frames_restart_the_sequence() {
        let mut dec = MtcDecoder::new();
        let seq = quarter_frames(0, 0, 5, 0);
        dec.feed(&seq[0]);
        dec.feed(&seq[1]);
        assert_eq!(dec.feed(&seq[4]), None); // a gap: dropped, nothing decoded
        assert_eq!(dec.feed(&seq[7]), None);
    }

    #[test]
    fn full_frame_sysex_locates_immediately() {
        let mut dec = MtcDecoder::new();
        let msg = [0xF0, 0x7F, 0x7F, 0x01, 0x01, 0x20 | 2, 30, 15, 5, 0xF7]; // 02:30:15:05 @ 25fps
        let pos = dec.feed(&msg).unwrap();
        assert!((pos.ms - parts_to_ms(2, 30, 15, 5, 25.0)).abs() < 1e-6);
    }

    #[test]
    fn midi_clock_pulses_give_the_bpm() {
        let mut tracker = MidiClockTracker::new();
        // 128 BPM: a beat is 468_750us, a pulse 19_531.25us.
        let pulse_us = 60_000_000.0 / 128.0 / 24.0;
        let mut bpm = None;
        for i in 0..100u64 {
            if let Some(ClockEvent::Bpm(b)) = tracker.feed((i as f64 * pulse_us) as u64, &[0xF8]) {
                bpm = Some(b);
            }
        }
        assert!((bpm.unwrap() - 128.0).abs() < 0.1);
        assert_eq!(tracker.feed(0, &[0xFA]), Some(ClockEvent::Start));
    }
}
