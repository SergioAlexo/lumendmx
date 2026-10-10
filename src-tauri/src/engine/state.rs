//! The engine's live state: a set of prioritized "layers" (the programmer,
//! plus one per running legacy asset playback) and, per universe, which
//! channels merge via HTP vs LTP.
//!
//! Deliberately channel-based, not fixture/attribute-based: resolving "select
//! fixture 3's red channel" into an actual (universe, channel) pair is static
//! configuration data, not timing-critical, so that resolution stays in the
//! frontend (which already has the fixture library) and only the resolved
//! result crosses into Rust. See docs/ENGINE.md.

use std::collections::{HashMap, HashSet};

use super::effects::EffectInstance;

/// A DMX address: which universe, which of its 512 channels (1-512).
pub type ChannelKey = (u32, u16);

/// The programmer always outranks every playback layer.
pub const PROGRAMMER_PRIORITY: i64 = i64::MAX;

#[derive(Debug)]
pub struct Layer {
    /// 0.0-255.0 so fades can interpolate sub-integer without a separate
    /// float layer type; rounded to `u8` only when merged for output.
    pub values: HashMap<ChannelKey, f32>,
    /// Effects running in this layer (Phase 7), each with its crossfade
    /// weight. Set by whoever owns the layer; rendered into `fx_values`
    /// every tick.
    pub effects: Vec<LayerEffect>,
    /// This tick's effect output. Wins over `values` for the same channel.
    pub fx_values: HashMap<ChannelKey, f32>,
    /// Multiplies this layer's HTP (intensity) channels at merge time — a
    /// playback's intensity-master fader. Applied after effects, so a
    /// dimmer effect is faded by the fader too.
    pub intensity_scale: f32,
}

impl Default for Layer {
    fn default() -> Self {
        Self { values: HashMap::new(), effects: Vec::new(), fx_values: HashMap::new(), intensity_scale: 1.0 }
    }
}

impl Layer {
    /// This layer's value for `key`, effects included, before
    /// `intensity_scale`.
    #[allow(dead_code)] // the sparse merge walks the maps directly
    pub fn get(&self, key: &ChannelKey) -> Option<f32> {
        self.fx_values.get(key).or_else(|| self.values.get(key)).copied()
    }

    pub fn has_effects(&self) -> bool {
        !self.effects.is_empty()
    }

    /// Clears values and effects (CLEAR/RELEASE on the programmer).
    pub fn clear(&mut self) {
        self.values.clear();
        self.effects.clear();
        self.fx_values.clear();
    }
}

#[derive(Debug, Clone)]
pub struct LayerEffect {
    pub instance: EffectInstance,
    /// 0.0-1.0 crossfade weight (see `effects::ResolvedEffect::scale`).
    pub scale: f32,
}

pub struct EngineState {
    pub programmer: Layer,
    /// Keyed by an arbitrary caller-chosen id (today: a triggered legacy
    /// asset's id). Iterated in priority order during merge.
    pub playbacks: HashMap<String, (i64, Layer)>,
    pub htp_channels: HashMap<u32, HashSet<u16>>,
    next_playback_priority: i64,
}

impl Default for EngineState {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineState {
    pub fn new() -> Self {
        Self {
            programmer: Layer::default(),
            playbacks: HashMap::new(),
            htp_channels: HashMap::new(),
            next_playback_priority: 0,
        }
    }

    /// Playback layers are priority-ordered by trigger order: whichever was
    /// triggered most recently wins LTP ties over one triggered earlier, a
    /// reasonable default "last one wins" until Phase 6 gives cuelists their
    /// own explicit priority levels.
    pub fn next_priority(&mut self) -> i64 {
        self.next_playback_priority += 1;
        self.next_playback_priority
    }

    #[allow(dead_code)] // the sparse merge resolves HTP sets once per universe
    pub fn is_htp(&self, universe: u32, channel: u16) -> bool {
        self.htp_channels.get(&universe).is_some_and(|set| set.contains(&channel))
    }
}
