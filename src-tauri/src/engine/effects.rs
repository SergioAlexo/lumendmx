//! The effects generator (BUILD_PLAN Phase 7): a waveform on any attribute
//! (dimmer, red, zoom…) or a shape on pan/tilt, fanned across an ordered
//! fixture selection.
//!
//! An effect is pure maths over (time, fixture position): `EffectDef` holds
//! the parameters, `render` turns a list of running effects into channel
//! values for one tick. It doesn't own any state of its own: the clock comes
//! from the global `Tempo`, and the fixture → channel mapping from the patch
//! index, resolved every tick so a re-patch takes effect immediately.
//!
//! Effects run *inside* a layer (the programmer, or a cue playback): the
//! owning layer's `fx_values` hold their output and win over its static
//! `values` for the same channel, so effects go through the same HTP/LTP
//! merge as everything else. See docs/ENGINE.md.

use std::collections::HashMap;
use std::f64::consts::PI;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::manager::PatchIndexEntry;
use super::state::ChannelKey;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum Waveform {
    Sine,
    /// High for the first `width`% of the cycle, low for the rest.
    Square,
    /// Falls from high to low over the cycle.
    Saw,
    /// Rises from low to high over the cycle.
    Ramp,
    /// A new random level every cycle (per fixture position).
    Random,
    /// Four equal steps from low to high.
    Step,
    /// A smooth bump over the first `width`% of the cycle, low for the rest.
    Pulse,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum PositionShape {
    Circle,
    Figure8,
    /// A wide pan sweep with a faster tilt bob (a 1:3 Lissajous figure).
    Ballyhoo,
    PanSaw,
    TiltSaw,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum EffectTarget {
    /// One attribute (as the frontend's `ChannelType` names it, e.g.
    /// "dimmer", "red", "zoom").
    #[serde(rename_all = "camelCase")]
    Attribute { attribute: String, waveform: Waveform },
    /// Pan and tilt together.
    #[serde(rename_all = "camelCase")]
    Position { shape: PositionShape },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum EffectRate {
    /// Cycles per second, independent of the show tempo.
    Hz { hz: f32 },
    /// Beats per cycle at the global BPM (1 = one cycle per beat, 4 = one
    /// cycle per bar), so the effect follows the tempo.
    Beats { beats: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum EffectMode {
    /// Swings around whatever value the attribute already has.
    Relative,
    /// Swings around `center`, ignoring the underlying value.
    Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum EffectDirection {
    Forward,
    Backward,
    /// Runs the cycle forward then back.
    Bounce,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct EffectDef {
    pub target: EffectTarget,
    pub mode: EffectMode,
    /// Peak-to-peak swing, 0-255.
    pub size: f32,
    /// The middle of the swing in `Absolute` mode, 0-255.
    pub center: f32,
    pub rate: EffectRate,
    /// Phase offset spread across the selection, in degrees: 0 = every
    /// fixture in step, 360 = evenly fanned over one full cycle.
    pub spread: f32,
    /// Phase offset for the whole effect, in degrees.
    pub phase: f32,
    /// Duty cycle for Square/Pulse, 0-100%.
    pub width: f32,
    pub direction: EffectDirection,
    /// Fixtures cycle through this many phase positions (1,2,3,1,2,3…).
    /// 0 or 1 = off.
    pub groups: u32,
    /// This many adjacent fixtures share a phase position. 0 or 1 = off.
    pub blocks: u32,
    /// The selection is split into this many mirrored wings (the phase runs
    /// outward-in/inward-out symmetrically). 0 or 1 = off.
    pub wings: u32,
}

impl EffectDef {
    /// A full-range waveform on `attribute`, one cycle per beat (tests'
    /// starting point; the UI has its own defaults).
    #[cfg(test)]
    pub fn new_attribute(attribute: &str, waveform: Waveform) -> Self {
        Self {
            target: EffectTarget::Attribute { attribute: attribute.to_string(), waveform },
            mode: EffectMode::Absolute,
            size: 255.0,
            center: 127.5,
            rate: EffectRate::Beats { beats: 1.0 },
            spread: 0.0,
            phase: 0.0,
            width: 50.0,
            direction: EffectDirection::Forward,
            groups: 0,
            blocks: 0,
            wings: 0,
        }
    }

    /// The attributes this effect drives.
    pub fn attributes(&self) -> Vec<&str> {
        match &self.target {
            EffectTarget::Attribute { attribute, .. } => vec![attribute.as_str()],
            EffectTarget::Position { .. } => vec!["pan", "tilt"],
        }
    }
}

/// A running effect: a definition applied to an ordered list of fixtures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct EffectInstance {
    pub id: u32,
    pub def: EffectDef,
    /// When set, the effect plays this Effect preset's *current* definition
    /// (falling back to `def` if the preset is gone) — so updating an Effect
    /// preset changes every cue that uses it, like every other preset.
    pub preset_id: Option<u32>,
    /// Order matters: it's the order the fan/spread runs in.
    pub fixtures: Vec<u32>,
}

/// The clock effects read their phase from (see `tempo::Tempo`).
#[derive(Debug, Clone, Copy, Default)]
pub struct EffectClock {
    pub seconds: f64,
    pub beats: f64,
}

/// One effect to render this tick, with its definition already resolved
/// (preset looked up) and its crossfade weight.
pub struct ResolvedEffect<'a> {
    pub def: &'a EffectDef,
    pub fixtures: &'a [u32],
    /// 0.0-1.0: how far the effect has faded in (0 = the underlying value,
    /// 1 = the full effect).
    pub scale: f32,
}

/// Cycles elapsed at `rate` on `clock`. Shared with pixel maps.
pub fn rate_cycles(rate: EffectRate, clock: EffectClock) -> f64 {
    match rate {
        EffectRate::Hz { hz } => clock.seconds * hz.max(0.0) as f64,
        EffectRate::Beats { beats } => {
            if beats <= 0.0 {
                0.0
            } else {
                clock.beats / beats as f64
            }
        }
    }
}

/// Cycles elapsed for `def` at `clock`, before any per-fixture offset.
fn base_cycles(def: &EffectDef, clock: EffectClock) -> f64 {
    rate_cycles(def.rate, clock) + def.phase as f64 / 360.0
}

/// Where fixture `index` (of `count`) sits in the fan, after wings, blocks
/// and groups: `(position, positions)`, with `position < positions`.
pub fn fan_position(def: &EffectDef, index: usize, count: usize) -> (usize, usize) {
    let (mut index, mut count) = (index, count.max(1));

    if def.wings > 1 {
        let wings = def.wings as usize;
        let wing_len = count.div_ceil(wings).max(1);
        let wing = index / wing_len;
        let within = index % wing_len;
        index = if wing % 2 == 1 { wing_len - 1 - within } else { within };
        count = wing_len;
    }
    if def.blocks > 1 {
        let blocks = def.blocks as usize;
        index /= blocks;
        count = count.div_ceil(blocks);
    }
    if def.groups > 1 {
        let groups = def.groups as usize;
        index %= groups;
        count = count.min(groups);
    }
    (index.min(count - 1), count)
}

/// Phase offset (in cycles) for fan position `position` of `positions`.
/// Later fixtures lag behind earlier ones, so the wave travels along the
/// selection from first to last.
fn fan_offset(def: &EffectDef, position: usize, positions: usize) -> f64 {
    -(def.spread as f64 / 360.0) * position as f64 / positions as f64
}

/// Applies direction to a phase in [0,1).
fn directed(direction: EffectDirection, p: f64) -> f64 {
    match direction {
        EffectDirection::Forward => p,
        EffectDirection::Backward => 1.0 - p,
        EffectDirection::Bounce => {
            if p < 0.5 {
                p * 2.0
            } else {
                2.0 - p * 2.0
            }
        }
    }
}

/// FNV-1a of the attribute name, so Random effects on red, green and blue
/// pick independent values instead of the same one (which made grey).
fn attribute_seed(attribute: &str) -> u64 {
    attribute.bytes().fold(0xCBF2_9CE4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01B3))
}

/// A deterministic pseudo-random value in [-1,1] for (cycle, seed).
fn hash_unit(cycle: i64, seed: u64) -> f32 {
    let mut x = (cycle as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ seed.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x ^= x >> 33;
    (x as f64 / u64::MAX as f64 * 2.0 - 1.0) as f32
}

/// The waveform's value in [-1,1] at phase `p` in [0,1). `cycle` and `seed`
/// only matter for `Random`.
pub fn waveform_value(waveform: Waveform, p: f64, width: f32, cycle: i64, seed: u64) -> f32 {
    let w = (width.clamp(1.0, 100.0) / 100.0) as f64;
    let v = match waveform {
        Waveform::Sine => (2.0 * PI * p).sin(),
        Waveform::Square => {
            if p < w {
                1.0
            } else {
                -1.0
            }
        }
        Waveform::Saw => 1.0 - 2.0 * p,
        Waveform::Ramp => 2.0 * p - 1.0,
        Waveform::Random => return hash_unit(cycle, seed),
        Waveform::Step => (p * 4.0).floor().min(3.0) / 3.0 * 2.0 - 1.0,
        Waveform::Pulse => {
            if p < w {
                (PI * p / w).sin() * 2.0 - 1.0
            } else {
                -1.0
            }
        }
    };
    v as f32
}

/// (pan, tilt) offsets in [-1,1] at phase `p` in [0,1).
pub fn shape_value(shape: PositionShape, p: f64) -> (f32, f32) {
    let a = 2.0 * PI * p;
    let (pan, tilt) = match shape {
        PositionShape::Circle => (a.sin(), a.cos()),
        PositionShape::Figure8 => (a.sin(), (2.0 * a).sin()),
        PositionShape::Ballyhoo => (a.sin(), (3.0 * a).sin()),
        PositionShape::PanSaw => (2.0 * p - 1.0, 0.0),
        PositionShape::TiltSaw => (0.0, 2.0 * p - 1.0),
    };
    (pan as f32, tilt as f32)
}

/// One channel's value: `wave` (in [-1,1]) applied to `base` per the
/// effect's mode and size, then blended in by `scale`.
fn apply(def: &EffectDef, base: f32, wave: f32, scale: f32) -> f32 {
    let swing = def.size.clamp(0.0, 255.0) / 2.0 * wave;
    let target = match def.mode {
        EffectMode::Relative => base + swing,
        EffectMode::Absolute => def.center + swing,
    };
    (base + (target - base) * scale.clamp(0.0, 1.0)).clamp(0.0, 255.0)
}

/// Renders `effects` into `out`, in order. Each channel's starting value is
/// whatever an earlier effect in the list already wrote to `out`, else
/// `base(key)` — so two effects on the same channel stack.
pub fn render(
    effects: &[ResolvedEffect],
    patch: &HashMap<u32, &PatchIndexEntry>,
    clock: EffectClock,
    base: &dyn Fn(ChannelKey) -> f32,
    out: &mut HashMap<ChannelKey, f32>,
) {
    for effect in effects {
        let def = effect.def;
        let cycles = base_cycles(def, clock);
        let count = effect.fixtures.len();
        for (index, fixture_number) in effect.fixtures.iter().enumerate() {
            let Some(entry) = patch.get(fixture_number) else { continue };
            let (position, positions) = fan_position(def, index, count);
            let total = cycles + fan_offset(def, position, positions);
            let cycle = total.floor();
            let p = directed(def.direction, total - cycle);

            let mut write = |attribute: &str, wave: f32| {
                if let Some(&channel) = entry.attribute_channels.get(attribute) {
                    let key = (entry.universe, channel);
                    let start = out.get(&key).copied().unwrap_or_else(|| base(key));
                    out.insert(key, apply(def, start, wave, effect.scale));
                }
            };
            match &def.target {
                EffectTarget::Attribute { attribute, waveform } => {
                    let seed = attribute_seed(attribute) ^ position as u64;
                    write(attribute, waveform_value(*waveform, p, def.width, cycle as i64, seed));
                }
                EffectTarget::Position { shape } => {
                    let (pan, tilt) = shape_value(*shape, p);
                    write("pan", pan);
                    write("tilt", tilt);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn par(number: u32, dimmer: u16) -> PatchIndexEntry {
        PatchIndexEntry {
            fixture_number: number,
            universe: 1,
            intensity_channel: Some(dimmer),
            attribute_channels: [
                ("dimmer".to_string(), dimmer),
                ("red".to_string(), dimmer + 1),
                ("pan".to_string(), dimmer + 2),
                ("tilt".to_string(), dimmer + 3),
            ]
            .into_iter()
            .collect(),
            cells: Vec::new(),
        }
    }

    fn four_pars() -> Vec<PatchIndexEntry> {
        (0..4).map(|i| par(i + 1, 1 + i as u16 * 10)).collect()
    }

    fn render_once(def: &EffectDef, fixtures: &[u32], patch: &[PatchIndexEntry], clock: EffectClock) -> HashMap<ChannelKey, f32> {
        let index: HashMap<u32, &PatchIndexEntry> = patch.iter().map(|e| (e.fixture_number, e)).collect();
        let mut out = HashMap::new();
        render(&[ResolvedEffect { def, fixtures, scale: 1.0 }], &index, clock, &|_| 100.0, &mut out);
        out
    }

    #[test]
    fn waveforms_hit_their_expected_points() {
        assert!((waveform_value(Waveform::Sine, 0.25, 50.0, 0, 0) - 1.0).abs() < 1e-6);
        assert!((waveform_value(Waveform::Sine, 0.75, 50.0, 0, 0) + 1.0).abs() < 1e-6);
        assert_eq!(waveform_value(Waveform::Square, 0.1, 50.0, 0, 0), 1.0);
        assert_eq!(waveform_value(Waveform::Square, 0.6, 50.0, 0, 0), -1.0);
        assert_eq!(waveform_value(Waveform::Square, 0.6, 75.0, 0, 0), 1.0); // wider duty
        assert_eq!(waveform_value(Waveform::Ramp, 0.0, 50.0, 0, 0), -1.0);
        assert_eq!(waveform_value(Waveform::Saw, 0.0, 50.0, 0, 0), 1.0);
        assert_eq!(waveform_value(Waveform::Step, 0.0, 50.0, 0, 0), -1.0);
        assert_eq!(waveform_value(Waveform::Step, 0.99, 50.0, 0, 0), 1.0);
        assert!((waveform_value(Waveform::Pulse, 0.05, 10.0, 0, 0) - 1.0).abs() < 1e-6); // peak mid-pulse
        assert_eq!(waveform_value(Waveform::Pulse, 0.5, 10.0, 0, 0), -1.0);
    }

    #[test]
    fn random_is_stable_within_a_cycle_and_varies_across_cycles_and_seeds() {
        let a = waveform_value(Waveform::Random, 0.1, 50.0, 7, 1);
        assert_eq!(a, waveform_value(Waveform::Random, 0.9, 50.0, 7, 1));
        assert!((-1.0..=1.0).contains(&a));
        let others: Vec<f32> = (0..8).map(|c| waveform_value(Waveform::Random, 0.0, 50.0, c, 1)).collect();
        assert!(others.windows(2).any(|w| w[0] != w[1]));
        assert_ne!(a, waveform_value(Waveform::Random, 0.1, 50.0, 7, 2));
    }

    #[test]
    fn absolute_sine_on_dimmer_swings_full_range() {
        let def = EffectDef::new_attribute("dimmer", Waveform::Sine); // 1 beat/cycle
        let patch = four_pars();
        let peak = render_once(&def, &[1], &patch, EffectClock { beats: 0.25, seconds: 0.0 });
        let trough = render_once(&def, &[1], &patch, EffectClock { beats: 0.75, seconds: 0.0 });
        assert_eq!(peak[&(1, 1)].round(), 255.0);
        assert_eq!(trough[&(1, 1)].round(), 0.0);
    }

    #[test]
    fn relative_mode_swings_around_the_base_value() {
        let mut def = EffectDef::new_attribute("red", Waveform::Sine);
        def.mode = EffectMode::Relative;
        def.size = 40.0;
        let patch = four_pars();
        let peak = render_once(&def, &[1], &patch, EffectClock { beats: 0.25, seconds: 0.0 });
        assert_eq!(peak[&(1, 2)].round(), 120.0); // base 100 + 40/2
    }

    #[test]
    fn hz_rate_ignores_the_beat_clock() {
        let mut def = EffectDef::new_attribute("dimmer", Waveform::Ramp);
        def.rate = EffectRate::Hz { hz: 2.0 };
        let patch = four_pars();
        // 0.125s at 2Hz = a quarter cycle; ramp at 0.25 = -0.5 -> 127.5 - 63.75
        let out = render_once(&def, &[1], &patch, EffectClock { beats: 99.9, seconds: 0.125 });
        assert!((out[&(1, 1)] - 63.75).abs() < 1e-3);
    }

    #[test]
    fn spread_fans_the_phase_evenly_across_the_selection() {
        let mut def = EffectDef::new_attribute("dimmer", Waveform::Ramp);
        def.spread = 360.0;
        let patch = four_pars();
        let out = render_once(&def, &[1, 2, 3, 4], &patch, EffectClock { beats: 0.0, seconds: 0.0 });
        // Phases 0, -0.25, -0.5, -0.75 → wrapped 0, .75, .5, .25 on a ramp.
        let values: Vec<f32> = [1u16, 11, 21, 31].iter().map(|&ch| out[&(1, ch)]).collect();
        let expected = [0.0, 0.75, 0.5, 0.25].map(|p: f32| 127.5 + 127.5 * (2.0 * p - 1.0));
        for (v, e) in values.iter().zip(expected) {
            assert!((v - e).abs() < 1e-3, "{values:?} vs {expected:?}");
        }
    }

    #[test]
    fn fixture_order_drives_the_fan_not_fixture_number() {
        let mut def = EffectDef::new_attribute("dimmer", Waveform::Ramp);
        def.spread = 360.0;
        let patch = four_pars();
        let forward = render_once(&def, &[1, 2, 3, 4], &patch, EffectClock::default());
        let reversed = render_once(&def, &[4, 3, 2, 1], &patch, EffectClock::default());
        assert_eq!(forward[&(1, 1)], reversed[&(1, 31)]);
        assert_eq!(forward[&(1, 31)], reversed[&(1, 1)]);
    }

    #[test]
    fn blocks_groups_and_wings_shape_the_fan() {
        let mut def = EffectDef::new_attribute("dimmer", Waveform::Sine);
        def.blocks = 2;
        assert_eq!((0..4).map(|i| fan_position(&def, i, 4)).collect::<Vec<_>>(), vec![(0, 2), (0, 2), (1, 2), (1, 2)]);

        def.blocks = 0;
        def.groups = 2;
        assert_eq!((0..4).map(|i| fan_position(&def, i, 4).0).collect::<Vec<_>>(), vec![0, 1, 0, 1]);

        def.groups = 0;
        def.wings = 2;
        assert_eq!((0..4).map(|i| fan_position(&def, i, 4)).collect::<Vec<_>>(), vec![(0, 2), (1, 2), (1, 2), (0, 2)]);
    }

    #[test]
    fn position_shape_drives_pan_and_tilt() {
        let mut def = EffectDef::new_attribute("dimmer", Waveform::Sine);
        def.target = EffectTarget::Position { shape: PositionShape::Circle };
        def.mode = EffectMode::Relative;
        def.size = 100.0;
        let patch = four_pars();
        let out = render_once(&def, &[1], &patch, EffectClock::default()); // p = 0: sin 0, cos 1
        assert_eq!(out[&(1, 3)].round(), 100.0); // pan at base
        assert_eq!(out[&(1, 4)].round(), 150.0); // tilt at base + 50
        assert!(!out.contains_key(&(1, 1))); // dimmer untouched
    }

    #[test]
    fn scale_blends_between_base_and_effect() {
        let def = EffectDef::new_attribute("dimmer", Waveform::Sine);
        let patch = four_pars();
        let index: HashMap<u32, &PatchIndexEntry> = patch.iter().map(|e| (e.fixture_number, e)).collect();
        let mut out = HashMap::new();
        let clock = EffectClock { beats: 0.25, seconds: 0.0 }; // peak: 255
        render(&[ResolvedEffect { def: &def, fixtures: &[1], scale: 0.5 }], &index, clock, &|_| 100.0, &mut out);
        assert_eq!(out[&(1, 1)].round(), 178.0); // halfway from 100 to 255 (177.5)
    }

    #[test]
    fn bounce_runs_forward_then_back() {
        assert_eq!(directed(EffectDirection::Bounce, 0.25), 0.5);
        assert_eq!(directed(EffectDirection::Bounce, 0.75), 0.5);
        assert_eq!(directed(EffectDirection::Backward, 0.25), 0.75);
    }

    #[test]
    fn random_effects_on_different_attributes_pick_independent_values() {
        // The test par has red and pan; any two attributes show it.
        let patch = four_pars();
        let clock = EffectClock { seconds: 0.0, beats: 0.5 };
        let red = render_once(&EffectDef::new_attribute("red", Waveform::Random), &[1], &patch, clock);
        let pan = render_once(&EffectDef::new_attribute("pan", Waveform::Random), &[1], &patch, clock);
        assert_ne!(red[&(1, 2)], pan[&(1, 3)]);
    }
}
