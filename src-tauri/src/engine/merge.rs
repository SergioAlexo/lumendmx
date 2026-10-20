//! HTP/LTP merge: intensity (a fixture's dimmer channel) is HTP — the highest
//! value from any layer wins, regardless of priority. Everything else is LTP —
//! whichever contributing layer has the highest priority wins outright.

use super::state::{ChannelKey, EngineState, Layer};
use crate::output::UNIVERSE_SIZE;

/// What goes on the wire: every layer merged, then the grand master applied
/// to intensity channels.
pub fn merge_universe(state: &EngineState, universe: u32) -> [u8; UNIVERSE_SIZE] {
    let mut layers: Vec<(i64, &Layer)> = state.layers.values().map(|(priority, layer)| (*priority, layer)).collect();
    layers.sort_by_key(|(priority, _)| std::cmp::Reverse(*priority));

    let mut out = [0u8; UNIVERSE_SIZE];
    for (i, slot) in out.iter_mut().enumerate() {
        let channel = (i + 1) as u16;
        let key = (universe, channel);
        let mut value = channel_value(state, &layers, key);
        if state.is_htp(universe, channel) {
            value *= state.grand_master;
        }
        *slot = to_u8(value);
    }
    out
}

fn channel_value(state: &EngineState, layers: &[(i64, &Layer)], key: ChannelKey) -> f32 {
    if state.is_htp(key.0, key.1) {
        layers
            .iter()
            .filter_map(|(_, layer)| layer.values.get(&key).map(|v| v * layer.intensity_scale))
            .fold(0.0f32, f32::max)
    } else {
        layers.iter().find_map(|(_, layer)| layer.values.get(&key).copied()).unwrap_or(0.0)
    }
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
        state.layers.insert("low".into(), (1, layer(&[(5, 100.0)])));
        state.layers.insert("high".into(), (2, layer(&[(5, 200.0)])));

        let out = merge_universe(&state, 1);
        assert_eq!(out[4], 200); // higher-priority layer wins, not higher value
    }

    #[test]
    fn htp_channel_uses_max_regardless_of_priority() {
        let mut state = EngineState::new();
        state.htp_channels.entry(1).or_default().insert(1);
        state.layers.insert("low_priority_but_bright".into(), (1, layer(&[(1, 255.0)])));
        state.layers.insert("high_priority_but_dim".into(), (2, layer(&[(1, 50.0)])));

        let out = merge_universe(&state, 1);
        assert_eq!(out[0], 255); // HTP: max wins even though it's the lower-priority layer
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
        let mut l = Layer::default();
        l.values.insert((1, 1), 255.0);
        l.values.insert((2, 1), 10.0);
        state.layers.insert("a".into(), (1, l));

        assert_eq!(merge_universe(&state, 1)[0], 255);
        assert_eq!(merge_universe(&state, 2)[0], 10);
    }

    #[test]
    fn values_round_and_clamp_to_u8_range() {
        let mut state = EngineState::new();
        state.layers.insert("a".into(), (1, layer(&[(1, 254.6), (2, 300.0), (3, -5.0)])));

        let out = merge_universe(&state, 1);
        assert_eq!(out[0], 255);
        assert_eq!(out[1], 255);
        assert_eq!(out[2], 0);
    }

    #[test]
    fn intensity_scale_and_grand_master_scale_only_htp_channels() {
        let mut state = EngineState::new();
        state.htp_channels.entry(1).or_default().insert(1);
        let mut l = layer(&[(1, 200.0), (2, 200.0)]);
        l.intensity_scale = 0.5;
        state.layers.insert("a".into(), (1, l));
        state.grand_master = 0.5;

        let out = merge_universe(&state, 1);
        assert_eq!(out[0], 50);
        assert_eq!(out[1], 200);
    }
}
