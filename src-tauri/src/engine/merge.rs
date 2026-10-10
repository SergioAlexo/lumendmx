//! HTP/LTP merge: intensity (a fixture's dimmer channel) is HTP — the highest
//! value from any layer wins, regardless of priority. Everything else is LTP —
//! whichever contributing layer has the highest priority wins outright. The
//! programmer always has the highest priority, so "programmer > playbacks"
//! falls out of this naturally rather than needing a special case.

use super::state::{EngineState, Layer, PROGRAMMER_PRIORITY};
use crate::output::UNIVERSE_SIZE;

pub fn merge_universe(state: &EngineState, universe: u32) -> [u8; UNIVERSE_SIZE] {
    merge(state, universe, true)
}

/// The merge without any layer's effect output — what a relative effect's
/// "underlying value" is for a channel its own layer doesn't set.
pub fn merge_universe_static(state: &EngineState, universe: u32) -> [u8; UNIVERSE_SIZE] {
    merge(state, universe, false)
}

fn merge(state: &EngineState, universe: u32, with_effects: bool) -> [u8; UNIVERSE_SIZE] {
    let value_of = |layer: &Layer, key: &(u32, u16)| {
        if with_effects {
            layer.get(key)
        } else {
            layer.values.get(key).copied()
        }
    };
    let mut layers: Vec<(i64, &Layer)> =
        state.playbacks.values().map(|(priority, layer)| (*priority, layer)).collect();
    layers.push((PROGRAMMER_PRIORITY, &state.programmer));
    layers.sort_by_key(|(priority, _)| std::cmp::Reverse(*priority));

    let mut out = [0u8; UNIVERSE_SIZE];
    for (i, slot) in out.iter_mut().enumerate() {
        let channel = (i + 1) as u16;
        let key = (universe, channel);

        if state.is_htp(universe, channel) {
            let max = layers
                .iter()
                .filter_map(|(_, layer)| value_of(layer, &key).map(|v| v * layer.intensity_scale))
                .fold(0.0f32, f32::max);
            *slot = to_u8(max);
        } else if let Some(value) = layers.iter().find_map(|(_, layer)| value_of(layer, &key)) {
            *slot = to_u8(value);
        }
    }
    out
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
}
