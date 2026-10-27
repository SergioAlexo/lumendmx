//! The engine's live state: a set of prioritized "layers" and, per universe,
//! which channels merge via HTP vs LTP.
//!
//! Deliberately channel-based, not fixture/attribute-based: whatever fills a
//! layer has already resolved fixture attributes to (universe, channel)
//! pairs, so the merge only deals in DMX addresses. See docs/ENGINE.md.

use std::collections::{HashMap, HashSet};

/// A DMX address: which universe, which of its 512 channels (1-512).
pub type ChannelKey = (u32, u16);

#[derive(Debug)]
pub struct Layer {
    /// 0.0-255.0, rounded to `u8` only when merged for output.
    pub values: HashMap<ChannelKey, f32>,
    /// Multiplies this layer's HTP (intensity) channels at merge time.
    pub intensity_scale: f32,
}

impl Default for Layer {
    fn default() -> Self {
        Self { values: HashMap::new(), intensity_scale: 1.0 }
    }
}

pub struct EngineState {
    /// Keyed by the layer's owner (e.g. a graph output node). Merged in
    /// priority order: the higher priority wins LTP channels.
    pub layers: HashMap<String, (i64, Layer)>,
    pub htp_channels: HashMap<u32, HashSet<u16>>,
    /// 0.0-1.0, multiplies every HTP (intensity) channel of the final output.
    pub grand_master: f32,
}

impl Default for EngineState {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineState {
    pub fn new() -> Self {
        Self { layers: HashMap::new(), htp_channels: HashMap::new(), grand_master: 1.0 }
    }

    pub fn is_htp(&self, universe: u32, channel: u16) -> bool {
        self.htp_channels.get(&universe).is_some_and(|set| set.contains(&channel))
    }
}
