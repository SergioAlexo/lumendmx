//! HTP/LTP merge: intensity (a fixture's dimmer channel) is HTP — the highest
//! value from any layer wins, regardless of priority. Everything else is LTP —
//! whichever contributing layer has the highest priority wins outright. The
//! programmer always has the highest priority, so "programmer > playbacks"
//! falls out of this naturally rather than needing a special case.

use std::collections::HashMap;

use super::state::{ChannelKey, EngineState, Layer, PROGRAMMER_PRIORITY};
use crate::output::UNIVERSE_SIZE;

pub fn merge_universe(state: &EngineState, universe: u32) -> [u8; UNIVERSE_SIZE] {
    merge(state, &[universe], true).pop().map_or([0u8; UNIVERSE_SIZE], |(_, frame)| frame)
}

/// The merge without any layer's effect output — what a relative effect's
/// "underlying value" is for a channel its own layer doesn't set.
pub fn merge_universe_static(state: &EngineState, universe: u32) -> [u8; UNIVERSE_SIZE] {
    merge(state, &[universe], false).pop().map_or([0u8; UNIVERSE_SIZE], |(_, frame)| frame)
}

/// Merges several universes in one pass over the layers (the per-tick path).
/// Returns `(universe, frame)` in the order given.
pub fn merge_universes(state: &EngineState, universes: &[u32]) -> Vec<(u32, [u8; UNIVERSE_SIZE])> {
    merge(state, universes, true)
}

/// Per-universe scratch for one merge.
struct Scratch<'a> {
    htp: Option<&'a std::collections::HashSet<u16>>,
    /// Highest scaled value seen per HTP channel.
    htp_max: [f32; UNIVERSE_SIZE],
    /// LTP channel already won by a higher-priority layer.
    filled: [bool; UNIVERSE_SIZE],
    out: [u8; UNIVERSE_SIZE],
}

/// Walks each layer's (sparse) values once, highest priority first, instead
/// of probing every layer for every one of the 512 channels: cost scales with
/// how many channels are actually driven, not with universes x 512 x layers.
fn merge(state: &EngineState, universes: &[u32], with_effects: bool) -> Vec<(u32, [u8; UNIVERSE_SIZE])> {
    let mut layers: Vec<(i64, &Layer)> =
        state.playbacks.values().map(|(priority, layer)| (*priority, layer)).collect();
    layers.push((PROGRAMMER_PRIORITY, &state.programmer));
    layers.sort_by_key(|(priority, _)| std::cmp::Reverse(*priority));

    let mut slots: HashMap<u32, usize> = HashMap::with_capacity(universes.len());
    let mut scratch: Vec<Scratch> = Vec::with_capacity(universes.len());
    for &universe in universes {
        slots.entry(universe).or_insert_with(|| {
            scratch.push(Scratch {
                htp: state.htp_channels.get(&universe),
                htp_max: [0.0; UNIVERSE_SIZE],
                filled: [false; UNIVERSE_SIZE],
                out: [0u8; UNIVERSE_SIZE],
            });
            scratch.len() - 1
        });
    }

    for (_, layer) in &layers {
        let mut apply = |key: &ChannelKey, value: f32| {
            let (universe, channel) = *key;
            if channel == 0 || channel as usize > UNIVERSE_SIZE {
                return;
            }
            let Some(&slot) = slots.get(&universe) else { return };
            let scratch = &mut scratch[slot];
            let i = channel as usize - 1;
            if scratch.htp.is_some_and(|set| set.contains(&channel)) {
                let scaled = value * layer.intensity_scale;
                if scaled > scratch.htp_max[i] {
                    scratch.htp_max[i] = scaled;
                }
            } else if !scratch.filled[i] {
                scratch.filled[i] = true;
                scratch.out[i] = to_u8(value);
            }
        };
        if with_effects {
            for (key, value) in &layer.fx_values {
                apply(key, *value);
            }
            for (key, value) in &layer.values {
                if !layer.fx_values.contains_key(key) {
                    apply(key, *value);
                }
            }
        } else {
            for (key, value) in &layer.values {
                apply(key, *value);
            }
        }
    }

    universes
        .iter()
        .map(|universe| {
            let scratch = &mut scratch[slots[universe]];
            let mut out = scratch.out;
            if let Some(set) = scratch.htp {
                for &channel in set {
                    if channel >= 1 && channel as usize <= UNIVERSE_SIZE {
                        out[channel as usize - 1] = to_u8(scratch.htp_max[channel as usize - 1]);
                    }
                }
            }
            (*universe, out)
        })
        .collect()
}

fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(entries: &[(u16, f32)]) -> Layer {
        Layer { values: entries.iter().map(|&(ch, v)| ((1, ch), v)).collect(), ..Default::default() }
    }

    #[test]
    fn ltp_channel_uses_highest_priority_layer() {
        let mut state = EngineState::new();
        state.playbacks.insert("low".into(), (1, layer(&[(5, 100.0)])));
        state.playbacks.insert("high".into(), (2, layer(&[(5, 200.0)])));

        let out = merge_universe(&state, 1);
        assert_eq!(out[4], 200); // higher-priority playback wins, not higher value
    }

    #[test]
    fn htp_channel_uses_max_regardless_of_priority() {
        let mut state = EngineState::new();
        state.htp_channels.entry(1).or_default().insert(1);
        state.playbacks.insert("low_priority_but_bright".into(), (1, layer(&[(1, 255.0)])));
        state.playbacks.insert("high_priority_but_dim".into(), (2, layer(&[(1, 50.0)])));

        let out = merge_universe(&state, 1);
        assert_eq!(out[0], 255); // HTP: max wins even though it's the lower-priority layer
    }

    #[test]
    fn programmer_always_outranks_playbacks_for_ltp() {
        let mut state = EngineState::new();
        state.playbacks.insert("cue".into(), (1_000_000, layer(&[(10, 100.0)])));
        state.programmer.values.insert((1, 10), 42.0);

        let out = merge_universe(&state, 1);
        assert_eq!(out[9], 42);
    }

    #[test]
    fn untouched_channel_defaults_to_zero() {
        let state = EngineState::new();
        let out = merge_universe(&state, 1);
        assert_eq!(out, [0u8; UNIVERSE_SIZE]);
    }

    #[test]
    fn different_universes_do_not_leak_into_each_other() {
        let mut state = EngineState::new();
        state.programmer.values.insert((1, 1), 255.0);
        state.programmer.values.insert((2, 1), 10.0);

        assert_eq!(merge_universe(&state, 1)[0], 255);
        assert_eq!(merge_universe(&state, 2)[0], 10);
    }

    #[test]
    fn values_round_and_clamp_to_u8_range() {
        let mut state = EngineState::new();
        state.programmer.values.insert((1, 1), 254.6);
        state.programmer.values.insert((1, 2), 300.0);
        state.programmer.values.insert((1, 3), -5.0);

        let out = merge_universe(&state, 1);
        assert_eq!(out[0], 255);
        assert_eq!(out[1], 255);
        assert_eq!(out[2], 0);
    }

    #[test]
    fn effect_values_win_over_the_same_layers_static_values() {
        let mut state = EngineState::new();
        let mut l = layer(&[(5, 100.0)]);
        l.fx_values.insert((1, 5), 180.0);
        state.playbacks.insert("fx".into(), (1, l));

        assert_eq!(merge_universe(&state, 1)[4], 180);
        assert_eq!(merge_universe_static(&state, 1)[4], 100);
    }

    #[test]
    fn intensity_scale_applies_to_htp_channels_only() {
        let mut state = EngineState::new();
        state.htp_channels.entry(1).or_default().insert(1);
        let mut l = layer(&[(1, 200.0), (2, 200.0)]);
        l.fx_values.insert((1, 1), 250.0);
        l.intensity_scale = 0.5;
        state.playbacks.insert("pb".into(), (1, l));

        let out = merge_universe(&state, 1);
        assert_eq!(out[0], 125); // effect value, then scaled
        assert_eq!(out[1], 200); // LTP channel unscaled
    }

    #[test]
    fn merge_universes_matches_per_universe_merges() {
        let mut state = EngineState::new();
        state.htp_channels.entry(1).or_default().insert(1);
        state.htp_channels.entry(2).or_default().insert(7);
        state.playbacks.insert("a".into(), (1, layer(&[(1, 90.0), (5, 10.0)])));
        let mut l = Layer::default();
        l.values.insert((2, 7), 200.0);
        l.values.insert((2, 8), 33.0);
        l.fx_values.insert((2, 8), 44.0);
        l.intensity_scale = 0.5;
        state.playbacks.insert("b".into(), (2, l));
        state.programmer.values.insert((1, 5), 77.0);

        let all = merge_universes(&state, &[2, 1, 3]);
        assert_eq!(all.iter().map(|(u, _)| *u).collect::<Vec<_>>(), vec![2, 1, 3]);
        for (universe, frame) in &all {
            assert_eq!(*frame, merge_universe(&state, *universe), "universe {universe}");
        }
        assert_eq!(all[0].1[6], 100); // HTP 200 * 0.5
        assert_eq!(all[0].1[7], 44); // effect value beats the static one
        assert_eq!(all[1].1[4], 77); // programmer wins LTP
    }
}
