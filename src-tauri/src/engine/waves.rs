//! Waveform maths the graph's generator nodes share: periodic waves,
//! pan/tilt shapes, and how a row of fixtures is fanned (groups, blocks,
//! wings). Pure functions of phase; the clock lives in `tempo`.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
    /// Drawn by the user: a `Curve`, evaluated by the LFO itself.
    Custom,
}

/// How a `Curve` gets from one point to the next.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CurveMode {
    Linear,
    /// Eases in and out of every point, never overshooting.
    Smooth,
    /// Holds each point's value until the next.
    Step,
}

/// A drawn wave: points `(x, y)` in [0,1], one cycle wide, looping from the
/// last point round to the first.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    points: Vec<(f32, f32)>,
    mode: CurveMode,
}

impl Curve {
    /// Reads `"x:y, x:y"`, clamping into [0,1] and sorting by x; unreadable
    /// pairs are skipped.
    pub fn parse(text: &str, mode: CurveMode) -> Self {
        let mut points: Vec<(f32, f32)> = text
            .split(',')
            .filter_map(|pair| {
                let (x, y) = pair.split_once(':')?;
                let (x, y) = (x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?);
                (x.is_finite() && y.is_finite()).then(|| (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)))
            })
            .collect();
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        Self { points, mode }
    }

    /// The value in [0,1] at phase `p` in [0,1). No points reads as 0.
    pub fn value(&self, p: f64) -> f32 {
        let (Some(&first), Some(&last)) = (self.points.first(), self.points.last()) else {
            return 0.0;
        };
        let p = p as f32;
        // The segment around p, wrapping past the ends into the next cycle.
        let next = self.points.iter().position(|&(x, _)| x > p);
        let (a, b) = match next {
            Some(0) => ((last.0 - 1.0, last.1), first),
            Some(i) => (self.points[i - 1], self.points[i]),
            None => (last, (first.0 + 1.0, first.1)),
        };
        let span = b.0 - a.0;
        let t = if span <= 0.0 { 1.0 } else { (p - a.0) / span };
        let t = match self.mode {
            CurveMode::Linear => t,
            CurveMode::Smooth => (1.0 - (std::f32::consts::PI * t).cos()) / 2.0,
            CurveMode::Step => 0.0,
        };
        a.1 + (b.1 - a.1) * t
    }
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

/// How a fan of fixtures is split before the phase spread runs over it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Fan {
    /// Fixtures cycle through this many phase positions (1,2,3,1,2,3…).
    /// 0 or 1 = off.
    pub groups: u32,
    /// This many adjacent fixtures share a phase position. 0 or 1 = off.
    pub blocks: u32,
    /// The fixtures are split into this many mirrored wings. 0 or 1 = off.
    pub wings: u32,
}

/// Where fixture `index` (of `count`) sits in the fan, after wings, blocks
/// and groups: `(position, positions)`, with `position < positions`.
pub fn fan_position(fan: Fan, index: usize, count: usize) -> (usize, usize) {
    let (mut index, mut count) = (index, count.max(1));

    if fan.wings > 1 {
        let wings = fan.wings as usize;
        let wing_len = count.div_ceil(wings).max(1);
        let wing = index / wing_len;
        let within = index % wing_len;
        index = if wing % 2 == 1 { wing_len - 1 - within } else { within };
        count = wing_len;
    }
    if fan.blocks > 1 {
        let blocks = fan.blocks as usize;
        index /= blocks;
        count = count.div_ceil(blocks);
    }
    if fan.groups > 1 {
        let groups = fan.groups as usize;
        index %= groups;
        count = count.min(groups);
    }
    (index.min(count - 1), count)
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
        // Drawn waves need their points; the LFO reads its `Curve` instead.
        Waveform::Custom => 0.0,
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_curve_interpolates_between_points_and_loops_round() {
        let tri = Curve::parse("0:0, 0.5:1, 1:0", CurveMode::Linear);
        assert_eq!(tri.value(0.0), 0.0);
        assert_eq!(tri.value(0.25), 0.5);
        assert_eq!(tri.value(0.5), 1.0);
        assert_eq!(tri.value(0.75), 0.5);

        // Points inside the cycle wrap: 0.75 -> 1.25 runs from 1 back down to 0.
        let inner = Curve::parse(" 0.75 : 1 ,0.25:0, junk, 2:x", CurveMode::Linear);
        assert_eq!(inner.value(0.25), 0.0);
        assert_eq!(inner.value(0.5), 0.5);
        assert_eq!(inner.value(1.0 - 1e-9), 0.5);
        assert_eq!(inner.value(0.0), 0.5);

        let step = Curve::parse("0:0.2, 0.5:0.8", CurveMode::Step);
        assert_eq!(step.value(0.1), 0.2);
        assert_eq!(step.value(0.6), 0.8);

        let smooth = Curve::parse("0:0, 0.5:1", CurveMode::Smooth);
        assert!((smooth.value(0.25) - 0.5).abs() < 1e-6);
        assert!(smooth.value(0.05) < 0.05); // eases out of the point
        assert!((0.0..=1.0).contains(&smooth.value(0.9)));

        assert_eq!(Curve::parse("", CurveMode::Linear).value(0.3), 0.0);
        assert_eq!(Curve::parse("0.4:0.7", CurveMode::Linear).value(0.9), 0.7); // one point: flat
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
    fn blocks_groups_and_wings_shape_the_fan() {
        let fan = Fan { blocks: 2, ..Fan::default() };
        assert_eq!((0..4).map(|i| fan_position(fan, i, 4)).collect::<Vec<_>>(), vec![(0, 2), (0, 2), (1, 2), (1, 2)]);

        let fan = Fan { groups: 2, ..Fan::default() };
        assert_eq!((0..4).map(|i| fan_position(fan, i, 4).0).collect::<Vec<_>>(), vec![0, 1, 0, 1]);

        let fan = Fan { wings: 2, ..Fan::default() };
        assert_eq!((0..4).map(|i| fan_position(fan, i, 4)).collect::<Vec<_>>(), vec![(0, 2), (1, 2), (1, 2), (0, 2)]);
    }
}
