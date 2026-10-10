//! The show's timecode clock and how a Timecode cuelist follows it
//! (BUILD_PLAN Phase 8).
//!
//! One clock, three sources: `Internal` (this engine runs it: the Timeline's
//! transport), `Mtc` (MIDI Time Code) and `Ltc` (audio-borne linear
//! timecode). The decoders in `crate::timing` push positions in; the engine
//! tick advances the internal clock and asks `target_index` which cue of a
//! Timecode cuelist should be live at the current position.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::cues::Cuelist;

/// An external source that hasn't sent a position for this long is treated
/// as stopped (tape stopped, cable pulled).
pub const EXTERNAL_TIMEOUT_MS: f64 = 250.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum TimecodeSource {
    Internal,
    Mtc,
    Ltc,
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct TimecodeStatus {
    pub source: TimecodeSource,
    pub position_ms: f64,
    /// Internal: the transport is playing. External: positions are arriving.
    pub active: bool,
    pub fps: f32,
}

#[derive(Debug, Clone)]
pub struct TimecodeClock {
    source: TimecodeSource,
    position_ms: f64,
    playing: bool,
    /// Time since the last external position.
    ext_age_ms: f64,
    /// Set by a seek (or an external locate) so a stopped clock still
    /// re-evaluates its cuelists once.
    seeked: bool,
    fps: f32,
}

impl Default for TimecodeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl TimecodeClock {
    pub fn new() -> Self {
        Self {
            source: TimecodeSource::Internal,
            position_ms: 0.0,
            playing: false,
            ext_age_ms: f64::INFINITY,
            seeked: false,
            fps: 25.0,
        }
    }

    pub fn source(&self) -> TimecodeSource {
        self.source
    }

    pub fn set_source(&mut self, source: TimecodeSource) {
        if source == self.source {
            return;
        }
        self.source = source;
        self.playing = false;
        self.ext_age_ms = f64::INFINITY;
    }

    pub fn position_ms(&self) -> f64 {
        self.position_ms
    }

    pub fn active(&self) -> bool {
        match self.source {
            TimecodeSource::Internal => self.playing,
            _ => self.ext_age_ms < EXTERNAL_TIMEOUT_MS,
        }
    }

    pub fn status(&self) -> TimecodeStatus {
        TimecodeStatus { source: self.source, position_ms: self.position_ms, active: self.active(), fps: self.fps }
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.position_ms = 0.0;
        self.seeked = true;
    }

    pub fn seek(&mut self, ms: f64) {
        self.position_ms = ms.max(0.0);
        self.seeked = true;
    }

    /// A decoder's position. Ignored unless `source` is the selected one.
    pub fn set_external(&mut self, source: TimecodeSource, position_ms: f64, fps: f32) {
        if source != self.source {
            return;
        }
        self.position_ms = position_ms.max(0.0);
        self.ext_age_ms = 0.0;
        self.fps = fps;
    }

    pub fn advance(&mut self, dt_ms: f64) {
        match self.source {
            TimecodeSource::Internal => {
                if self.playing {
                    self.position_ms += dt_ms;
                }
            }
            _ => self.ext_age_ms += dt_ms,
        }
    }

    /// Whether cuelists should follow the clock this tick: while it's
    /// running, and once after any seek.
    pub fn take_follow(&mut self) -> bool {
        let follow = self.active() || self.seeked;
        self.seeked = false;
        follow
    }
}

/// Which cue of a Timecode cuelist is live at `position_ms`: the cue with
/// the latest fire time at or before it. `None` before the first timed cue
/// (or when no cue has a time).
pub fn target_index(cuelist: &Cuelist, position_ms: f64) -> Option<usize> {
    cuelist
        .cues
        .iter()
        .enumerate()
        .filter_map(|(i, c)| c.timecode_ms.filter(|&t| t <= position_ms + 0.5).map(|t| (i, t)))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
}

/// `hh:mm:ss:ff` -> milliseconds at `fps`.
pub fn parts_to_ms(h: u32, m: u32, s: u32, f: u32, fps: f32) -> f64 {
    ((h * 3600 + m * 60 + s) as f64) * 1000.0 + f as f64 * 1000.0 / fps as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cues::{Cue, CuelistKind};

    fn timed_list() -> Cuelist {
        let mut list = Cuelist::new(1, "TC".to_string(), CuelistKind::Timecode, false);
        for (n, t) in [(1.0, Some(1000.0)), (2.0, Some(5000.0)), (3.0, None), (4.0, Some(3000.0))] {
            let mut cue = Cue::new(n, format!("Cue {n}"));
            cue.timecode_ms = t;
            list.upsert_cue(cue);
        }
        list
    }

    #[test]
    fn target_is_the_latest_timed_cue_at_or_before_the_position() {
        let list = timed_list();
        assert_eq!(target_index(&list, 500.0), None);
        assert_eq!(target_index(&list, 1000.0), Some(0));
        assert_eq!(target_index(&list, 3500.0), Some(3)); // cue 4 fires at 3s even though it is numbered last
        assert_eq!(target_index(&list, 9000.0), Some(1));
    }

    #[test]
    fn internal_clock_advances_only_while_playing() {
        let mut clock = TimecodeClock::new();
        clock.advance(100.0);
        assert_eq!(clock.position_ms(), 0.0);
        clock.play();
        clock.advance(100.0);
        assert_eq!(clock.position_ms(), 100.0);
        clock.seek(5000.0);
        assert!(clock.take_follow());
        clock.pause();
        assert!(!clock.take_follow());
    }

    #[test]
    fn external_clock_goes_inactive_when_positions_stop() {
        let mut clock = TimecodeClock::new();
        clock.set_source(TimecodeSource::Mtc);
        clock.set_external(TimecodeSource::Ltc, 1000.0, 25.0); // wrong source: ignored
        assert_eq!(clock.position_ms(), 0.0);
        clock.set_external(TimecodeSource::Mtc, 1000.0, 25.0);
        assert!(clock.active());
        clock.advance(100.0);
        assert!(clock.active());
        clock.advance(200.0);
        assert!(!clock.active());
    }
}
