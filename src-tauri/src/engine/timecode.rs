//! The show's timecode clock (BUILD_PLAN Phase 8): an external source's
//! position, read by the graph's Timecode node.
//!
//! Two sources: `Mtc` (MIDI Time Code) and `Ltc` (audio-borne linear
//! timecode). The decoders in `crate::timing` push positions in; the engine
//! tick ages them so a stopped source reads as inactive.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// An external source that hasn't sent a position for this long is treated
/// as stopped (tape stopped, cable pulled).
pub const EXTERNAL_TIMEOUT_MS: f64 = 250.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum TimecodeSource {
    None,
    Mtc,
    Ltc,
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct TimecodeStatus {
    pub source: TimecodeSource,
    pub position_ms: f64,
    /// Positions are arriving.
    pub active: bool,
    pub fps: f32,
}

#[derive(Debug, Clone)]
pub struct TimecodeClock {
    source: TimecodeSource,
    position_ms: f64,
    /// Time since the last external position.
    ext_age_ms: f64,
    fps: f32,
}

impl Default for TimecodeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl TimecodeClock {
    pub fn new() -> Self {
        Self { source: TimecodeSource::None, position_ms: 0.0, ext_age_ms: f64::INFINITY, fps: 25.0 }
    }

    pub fn set_source(&mut self, source: TimecodeSource) {
        if source == self.source {
            return;
        }
        self.source = source;
        self.ext_age_ms = f64::INFINITY;
    }

    pub fn position_ms(&self) -> f64 {
        self.position_ms
    }

    pub fn active(&self) -> bool {
        self.source != TimecodeSource::None && self.ext_age_ms < EXTERNAL_TIMEOUT_MS
    }

    pub fn status(&self) -> TimecodeStatus {
        TimecodeStatus { source: self.source, position_ms: self.position_ms, active: self.active(), fps: self.fps }
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
        self.ext_age_ms += dt_ms;
    }
}

/// `hh:mm:ss:ff` -> milliseconds at `fps`.
pub fn parts_to_ms(h: u32, m: u32, s: u32, f: u32, fps: f32) -> f64 {
    ((h * 3600 + m * 60 + s) as f64) * 1000.0 + f as f64 * 1000.0 / fps as f64
}

#[cfg(test)]
mod tests {
    use super::*;

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
