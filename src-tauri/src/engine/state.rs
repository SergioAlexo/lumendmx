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
    /// Programmer TIME: values still fading toward their target. The tick
    /// writes the in-between value into `values` every frame.
    pub fades: HashMap<ChannelKey, ValueFade>,
}

/// A programmer value fading from `from` to `to` over `duration_ms`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueFade {
    pub from: f32,
    pub to: f32,
    pub elapsed_ms: f64,
    pub duration_ms: f64,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
            effects: Vec::new(),
            fx_values: HashMap::new(),
            intensity_scale: 1.0,
            fades: HashMap::new(),
        }
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
        self.fades.clear();
    }

    /// Sets `key` to `value`. With `fade_ms > 0` the value instead fades in
    /// from its current value (`current`, used when the layer has none yet).
    pub fn set_value(&mut self, key: ChannelKey, value: f32, fade_ms: f64, current: f32) {
        if fade_ms > 0.0 {
            let from = self.values.get(&key).copied().unwrap_or(current);
            self.values.insert(key, from);
            self.fades.insert(key, ValueFade { from, to: value, elapsed_ms: 0.0, duration_ms: fade_ms });
        } else {
            self.fades.remove(&key);
            self.values.insert(key, value);
        }
    }

    /// Advances programmer-TIME fades by `dt_ms`.
    pub fn advance_fades(&mut self, dt_ms: f64) {
        if self.fades.is_empty() {
            return;
        }
        let values = &mut self.values;
        self.fades.retain(|key, fade| {
            fade.elapsed_ms += dt_ms;
            let t = (fade.elapsed_ms / fade.duration_ms).clamp(0.0, 1.0) as f32;
            values.insert(*key, fade.from + (fade.to - fade.from) * t);
            t < 1.0
        });
    }
}

/// Non-destructive highlight: shown on top of the merged output, never
/// written into the programmer or any playback (so it can't be recorded).
#[derive(Debug, Clone, Default)]
pub struct Highlight {
    /// Channels forced to a value on the selected fixtures (intensity and
    /// white/RGB at full: "full / open white").
    pub lit: HashMap<ChannelKey, f32>,
    /// Intensity channels of every other fixture, dimmed ("lowlight").
    pub dimmed: HashSet<ChannelKey>,
}

/// How much of its level a lowlighted fixture keeps.
pub const LOWLIGHT_SCALE: f32 = 0.25;

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
    /// Blind mode: programmer edits go to `blind` (which never outputs)
    /// instead of `programmer`.
    pub blind_mode: bool,
    pub blind: Layer,
    pub highlight: Option<Highlight>,
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
        Self {
            programmer: Layer::default(),
            playbacks: HashMap::new(),
            htp_channels: HashMap::new(),
            next_playback_priority: 0,
            blind_mode: false,
            blind: Layer::default(),
            highlight: None,
            grand_master: 1.0,
        }
    }

    /// The layer manual programmer edits land in: the live programmer, or
    /// the blind one while blind mode is on.
    pub fn edit_layer(&mut self) -> &mut Layer {
        if self.blind_mode {
            &mut self.blind
        } else {
            &mut self.programmer
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
