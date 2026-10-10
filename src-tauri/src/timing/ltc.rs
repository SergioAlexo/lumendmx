//! Linear timecode (LTC) decoding: an 80-bit biphase-mark frame carried in
//! an audio signal. Zero crossings give transition intervals; a long
//! interval is a 0 bit, two short ones a 1. The sync word (0xBFFC in bits
//! 64-79) marks the end of each frame.

/// A decoded frame. `ms` is the frame's labelled time (its start); the
/// frame has just finished arriving, so "now" is one frame later.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LtcFrame {
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
    pub frames: u32,
    pub fps: f32,
}

const SYNC_WORD: u128 = 0xBFFC;
/// Hysteresis around zero, so noise on a silent input doesn't count.
const THRESHOLD: f32 = 0.02;

pub struct LtcDecoder {
    sample_rate: f32,
    /// Estimated samples per bit.
    bit_period: f32,
    /// Sample index of the previous zero crossing.
    last_crossing: Option<f64>,
    sample_index: u64,
    prev_sample: f32,
    /// Sign the signal last settled to (outside the hysteresis band).
    positive: Option<bool>,
    half_pending: bool,
    reg: u128,
    bits_seen: u32,
}

impl LtcDecoder {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            bit_period: sample_rate / (27.0 * 80.0),
            last_crossing: None,
            sample_index: 0,
            prev_sample: 0.0,
            positive: None,
            half_pending: false,
            reg: 0,
            bits_seen: 0,
        }
    }

    /// Feeds mono samples; calls `on_frame` for every frame completed.
    pub fn push(&mut self, samples: &[f32], mut on_frame: impl FnMut(LtcFrame)) {
        for &s in samples {
            let crossed = match self.positive {
                None => {
                    if s > THRESHOLD {
                        self.positive = Some(true);
                    } else if s < -THRESHOLD {
                        self.positive = Some(false);
                    }
                    false
                }
                Some(true) if s < -THRESHOLD => {
                    self.positive = Some(false);
                    true
                }
                Some(false) if s > THRESHOLD => {
                    self.positive = Some(true);
                    true
                }
                _ => false,
            };
            if crossed {
                // Where the line actually crossed zero, between samples.
                let frac = self.prev_sample / (self.prev_sample - s);
                let at = self.sample_index as f64 - 1.0 + frac as f64;
                if let Some(prev) = self.last_crossing {
                    if let Some(frame) = self.interval((at - prev) as f32) {
                        on_frame(frame);
                    }
                }
                self.last_crossing = Some(at);
            }
            self.prev_sample = s;
            self.sample_index += 1;
        }
    }

    fn interval(&mut self, d: f32) -> Option<LtcFrame> {
        let p = self.bit_period;
        if d > 0.75 * p && d < 1.5 * p {
            // A full-length cell: a 0 bit. (A lone half before it was noise.)
            self.half_pending = false;
            self.bit_period = 0.9 * p + 0.1 * d;
            self.bit(false)
        } else if d > 0.3 * p && d <= 0.75 * p {
            if self.half_pending {
                self.half_pending = false;
                self.bit_period = 0.9 * p + 0.1 * (2.0 * d);
                self.bit(true)
            } else {
                self.half_pending = true;
                None
            }
        } else {
            // Not a plausible cell: lost lock. Wait for the next sync.
            self.half_pending = false;
            self.bits_seen = 0;
            if d > 1.5 * p && d < 4.0 * p {
                self.bit_period = d.min(self.sample_rate / (24.0 * 80.0)).max(self.sample_rate / (30.0 * 80.0));
            }
            None
        }
    }

    fn bit(&mut self, one: bool) -> Option<LtcFrame> {
        self.reg = (self.reg >> 1) | ((one as u128) << 79);
        self.bits_seen = (self.bits_seen + 1).min(1000);
        if self.bits_seen < 80 || (self.reg >> 64) & 0xFFFF != SYNC_WORD {
            return None;
        }
        let field = |shift: u32, bits: u32| ((self.reg >> shift) & ((1 << bits) - 1)) as u32;
        let fps_est = self.sample_rate / (self.bit_period * 80.0);
        let fps = [24.0f32, 25.0, 30.0]
            .into_iter()
            .min_by(|a, b| (a - fps_est).abs().partial_cmp(&(b - fps_est).abs()).unwrap())
            .unwrap();
        self.bits_seen = 0; // the next frame needs its own 80 bits
        Some(LtcFrame {
            frames: field(0, 4) + 10 * field(8, 2),
            seconds: field(16, 4) + 10 * field(24, 3),
            minutes: field(32, 4) + 10 * field(40, 3),
            hours: field(48, 4) + 10 * field(56, 2),
            fps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::timecode::parts_to_ms;

    /// Synthesizes `count` consecutive frames starting at `start`
    /// (h, m, s, f) as a +-`amp` biphase-mark square wave.
    fn encode(start: (u32, u32, u32, u32), count: u32, fps: u32, sr: f32, amp: f32) -> Vec<f32> {
        let (h, m, s, f0) = start;
        let mut out = Vec::new();
        let mut level = amp;
        let mut t = 0.0f64; // in samples
        let bit_len = sr as f64 / (fps as f64 * 80.0);
        let mut emit_until = |t_end: f64, level: f32, out: &mut Vec<f32>| {
            while (out.len() as f64) < t_end {
                out.push(level);
            }
        };
        let mut frame_no = h * 3600 * fps + m * 60 * fps + s * fps + f0;
        for _ in 0..count {
            let total_secs = frame_no / fps;
            let (hh, mm, ss, ff) = (total_secs / 3600, total_secs / 60 % 60, total_secs % 60, frame_no % fps);
            let mut word: u128 = 0;
            let mut put = |shift: u32, value: u32| word |= (value as u128) << shift;
            put(0, ff % 10);
            put(8, ff / 10);
            put(16, ss % 10);
            put(24, ss / 10);
            put(32, mm % 10);
            put(40, mm / 10);
            put(48, hh % 10);
            put(56, hh / 10);
            word |= SYNC_WORD << 64;
            for i in 0..80 {
                let bit = (word >> i) & 1;
                level = -level; // every bit starts with a transition
                emit_until(t + bit_len / 2.0, level, &mut out);
                if bit == 1 {
                    level = -level; // mid-bit transition
                }
                t += bit_len;
                emit_until(t, level, &mut out);
            }
            frame_no += 1;
        }
        out
    }

    fn decode(samples: &[f32], sr: f32) -> Vec<LtcFrame> {
        let mut dec = LtcDecoder::new(sr);
        let mut frames = Vec::new();
        for chunk in samples.chunks(512) {
            dec.push(chunk, |f| frames.push(f));
        }
        frames
    }

    #[test]
    fn decodes_consecutive_25fps_frames() {
        let frames = decode(&encode((1, 23, 45, 20), 12, 25, 48_000.0, 0.5), 48_000.0);
        // The first frame is spent locking on; every later one decodes.
        assert!(frames.len() >= 10, "{} frames", frames.len());
        let last = frames.last().unwrap();
        assert_eq!((last.hours, last.minutes, last.seconds, last.frames), (1, 23, 46, 5)); // the 12th frame ends without a following edge, so the 11th is the last decoded
        assert_eq!(last.fps, 25.0);
        for pair in frames.windows(2) {
            let step = parts_to_ms(pair[1].hours, pair[1].minutes, pair[1].seconds, pair[1].frames, 25.0)
                - parts_to_ms(pair[0].hours, pair[0].minutes, pair[0].seconds, pair[0].frames, 25.0);
            assert!((step - 40.0).abs() < 1e-6, "{step}");
        }
    }

    #[test]
    fn decodes_30fps_at_44100_with_a_quiet_signal() {
        // 44100/(30*80) = 18.375 samples per bit: fractional cell lengths.
        let frames = decode(&encode((0, 0, 59, 25), 10, 30, 44_100.0, 0.08), 44_100.0);
        assert!(frames.len() >= 8, "{} frames", frames.len());
        let last = frames.last().unwrap();
        assert_eq!(last.fps, 30.0);
        assert_eq!((last.minutes, last.seconds, last.frames), (1, 0, 3)); // 59s 25f + 8 frames (the 10th frame has no trailing edge)
    }

    #[test]
    fn silence_decodes_nothing() {
        assert!(decode(&vec![0.001; 48_000], 48_000.0).is_empty());
    }
}
