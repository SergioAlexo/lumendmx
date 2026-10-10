//! The global tempo: one BPM for the whole show, set manually or by tap, and
//! a continuously-integrated beat position that BPM-synced effects and pixel
//! maps read their phase from (BUILD_PLAN Phase 7).
//!
//! The beat position is advanced by the engine tick (`advance`), not derived
//! from wall-clock time × BPM, so changing the BPM changes the *speed* from
//! that moment on without making every synced effect jump to a different
//! phase. Phase 8 adds more ways to drive the BPM (audio, Ableton Link, MIDI
//! clock); they all just call `set_bpm`.

use serde::Serialize;
use ts_rs::TS;

pub const DEFAULT_BPM: f32 = 120.0;
pub const MIN_BPM: f32 = 20.0;
pub const MAX_BPM: f32 = 300.0;

/// Taps further apart than this start a new measurement rather than
/// averaging with the old one.
const TAP_RESET_MS: f64 = 2000.0;
/// How many recent tap intervals are averaged.
const TAP_WINDOW: usize = 4;

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct TempoStatus {
    pub bpm: f32,
    /// Beats elapsed since the engine started (fractional).
    pub beat: f64,
}

#[derive(Debug, Clone)]
pub struct Tempo {
    bpm: f32,
    /// Beats elapsed, integrated tick by tick.
    beat: f64,
    /// Seconds elapsed, for effects whose rate is in Hz rather than beats.
    seconds: f64,
    /// Timestamps (ms, caller's clock) of recent taps.
    taps: Vec<f64>,
}

impl Default for Tempo {
    fn default() -> Self {
        Self::new()
    }
}

impl Tempo {
    pub fn new() -> Self {
        Self { bpm: DEFAULT_BPM, beat: 0.0, seconds: 0.0, taps: Vec::new() }
    }

    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    pub fn beat(&self) -> f64 {
        self.beat
    }

    pub fn seconds(&self) -> f64 {
        self.seconds
    }

    pub fn status(&self) -> TempoStatus {
        TempoStatus { bpm: self.bpm, beat: self.beat }
    }

    pub fn set_bpm(&mut self, bpm: f32) {
        self.bpm = bpm.clamp(MIN_BPM, MAX_BPM);
    }

    /// Called by the engine tick.
    pub fn advance(&mut self, dt_ms: f64) {
        self.seconds += dt_ms / 1000.0;
        self.beat += dt_ms / 60_000.0 * self.bpm as f64;
    }

    /// Snaps the beat grid to the nearest whole beat (an external clock's
    /// start message, or a tap).
    pub fn align_beat(&mut self) {
        self.beat = self.beat.round();
    }

    /// Registers a tap at `now_ms` and, once there are at least two taps
    /// close enough together, sets the BPM from the average of the recent
    /// intervals. A tap also re-aligns the beat grid so the tap lands on a
    /// beat — tapping along to music puts synced effects on the downbeat,
    /// not just at the right speed. Returns the (possibly unchanged) BPM.
    pub fn tap(&mut self, now_ms: f64) -> f32 {
        if self.taps.last().is_some_and(|&last| now_ms - last > TAP_RESET_MS || now_ms <= last) {
            self.taps.clear();
        }
        self.taps.push(now_ms);
        if self.taps.len() > TAP_WINDOW + 1 {
            self.taps.remove(0);
        }
        if self.taps.len() >= 2 {
            let span = self.taps[self.taps.len() - 1] - self.taps[0];
            let interval = span / (self.taps.len() - 1) as f64;
            self.set_bpm((60_000.0 / interval) as f32);
        }
        self.beat = self.beat.round();
        self.bpm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_integrates_beats_at_the_current_bpm() {
        let mut tempo = Tempo::new();
        tempo.set_bpm(120.0);
        tempo.advance(1000.0); // 1s at 120 BPM = 2 beats
        assert!((tempo.beat() - 2.0).abs() < 1e-9);

        tempo.set_bpm(60.0);
        tempo.advance(1000.0); // +1 beat, no jump from the BPM change
        assert!((tempo.beat() - 3.0).abs() < 1e-9);
        assert!((tempo.seconds() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn tapping_sets_bpm_from_the_average_interval() {
        let mut tempo = Tempo::new();
        for i in 0..4 {
            tempo.tap(1000.0 + i as f64 * 500.0); // 500ms apart = 120 BPM
        }
        assert!((tempo.bpm() - 120.0).abs() < 0.01);

        for i in 0..5 {
            tempo.tap(10_000.0 + i as f64 * 400.0); // after a long gap: 150 BPM
        }
        assert!((tempo.bpm() - 150.0).abs() < 0.01);
    }

    #[test]
    fn a_single_tap_does_not_change_bpm_but_aligns_the_beat() {
        let mut tempo = Tempo::new();
        tempo.advance(1300.0); // 2.6 beats at 120
        tempo.tap(0.0);
        assert_eq!(tempo.bpm(), DEFAULT_BPM);
        assert_eq!(tempo.beat(), 3.0);
    }

    #[test]
    fn bpm_is_clamped() {
        let mut tempo = Tempo::new();
        tempo.set_bpm(1000.0);
        assert_eq!(tempo.bpm(), MAX_BPM);
        tempo.set_bpm(0.0);
        assert_eq!(tempo.bpm(), MIN_BPM);
    }
}
