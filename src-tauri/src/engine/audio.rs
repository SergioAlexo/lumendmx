//! Live audio levels: the audio thread's analyser writes `BandLevels`, and
//! the graph's Audio node reads them every tick.

use serde::Serialize;
use ts_rs::TS;

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
