//! Live audio levels: the audio thread's analyser writes `BandLevels`, and
//! the graph's Audio node reads them every tick.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum Band {
    Low,
    Mid,
    High,
    /// Absolute input loudness, not normalized to recent peaks, so the gate
    /// is a real "this loud" threshold (a clap or shout, not the room tone).
    Loudness,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct BandLevels {
    pub low: f32,
    pub mid: f32,
    pub high: f32,
    /// Overall loudness, 0.0-1.0.
    pub rms: f32,
    /// Absolute loudness: the input's dBFS mapped -60 dB → 0.0, 0 dB → 1.0.
    pub loudness: f32,
}

impl BandLevels {
    pub fn get(&self, band: Band) -> f32 {
        match band {
            Band::Low => self.low,
            Band::Mid => self.mid,
            Band::High => self.high,
            Band::Loudness => self.loudness,
        }
    }
}
