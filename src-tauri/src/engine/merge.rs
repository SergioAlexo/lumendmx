//! HTP/LTP merge: intensity (a fixture's dimmer channel) is HTP — the highest
//! value from any layer wins, regardless of priority. Everything else is LTP —
//! whichever contributing layer has the highest priority wins outright. The
//! programmer always has the highest priority, so "programmer > playbacks"
//! falls out of this naturally rather than needing a special case.

use std::collections::{HashMap, HashSet};

use super::state::{ChannelKey, EngineState, Layer, LOWLIGHT_SCALE, PROGRAMMER_PRIORITY};
use crate::output::UNIVERSE_SIZE;

/// Which layer supplied a channel's final value (Phase 12's Output/Live
/// sheet). The numeric codes are the wire format of `engine_get_universe_sources`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ChannelSource {
    /// No layer contributes: the channel sits at 0.
    Default = 0,
    Programmer = 1,
    /// A running playback (cue, legacy asset or pixel map) holds the value.
    Playback = 2,
    /// A cue playback's value tracked forward from an earlier cue rather than
    /// recorded in the current one.
    Tracked = 3,
    /// An effect is writing the channel.
    Effect = 4,
}

/// Per playback layer key (`cue-playback-N`): the channels whose value is
/// tracked from an earlier cue. Computed by the manager from the cuelists.
pub type TrackedChannels = HashMap<String, HashSet<ChannelKey>>;

/// For each of a universe's 512 channels, which layer won it in
/// `merge_universe`. Same winner rules as the merge (HTP: highest scaled
/// value, first layer in priority order on ties; LTP: highest priority).
/// Read-only and only called from the UI's polled query, never the tick.
pub fn channel_sources(state: &EngineState, universe: u32, tracked: &TrackedChannels) -> [ChannelSource; UNIVERSE_SIZE] {
    let mut layers: Vec<(Option<&str>, i64, &Layer)> =
        state.playbacks.iter().map(|(id, (priority, layer))| (Some(id.as_str()), *priority, layer)).collect();
    layers.push((None, PROGRAMMER_PRIORITY, &state.programmer));
    layers.sort_by_key(|(_, priority, _)| std::cmp::Reverse(*priority));

    let classify = |id: Option<&str>, layer: &Layer, key: &ChannelKey| {
        if layer.fx_values.contains_key(key) {
            ChannelSource::Effect
        } else if id.is_none() {
            ChannelSource::Programmer
        } else if id.and_then(|id| tracked.get(id)).is_some_and(|set| set.contains(key)) {
            ChannelSource::Tracked
        } else {
            ChannelSource::Playback
        }
    };

    let mut out = [ChannelSource::Default; UNIVERSE_SIZE];
    for (i, slot) in out.iter_mut().enumerate() {
        let channel = (i + 1) as u16;
        let key = (universe, channel);
        let winner = if state.is_htp(universe, channel) {
            let mut best: Option<(f32, Option<&str>, &Layer)> = None;
            for (id, _, layer) in &layers {
                if let Some(v) = layer.get(&key) {
                    let scaled = v * layer.intensity_scale;
                    if best.is_none_or(|(b, _, _)| scaled > b) {
                        best = Some((scaled, *id, layer));
                    }
                }
            }
            best.map(|(_, id, layer)| (id, layer))
        } else {
            layers.iter().find(|(_, _, layer)| layer.get(&key).is_some()).map(|(id, _, layer)| (*id, *layer))
        };
        if let Some((id, layer)) = winner {
            *slot = classify(id, layer, &key);
        }
    }
    out
}

/// What goes on the wire: layers merged, effects included, then the grand
/// master (HTP channels only) and highlight/lowlight applied on top.
pub fn merge_universe(state: &EngineState, universe: u32) -> [u8; UNIVERSE_SIZE] {
    merge(state, &[universe], true, true).pop().map_or([0u8; UNIVERSE_SIZE], |(_, frame)| frame)
}

/// Several universes' output frames in one pass over the layers — the
/// per-tick path. Same result as `merge_universe` for each; returns
/// `(universe, frame)` in the order given.
pub fn merge_universes(state: &EngineState, universes: &[u32]) -> Vec<(u32, [u8; UNIVERSE_SIZE])> {
    merge(state, universes, true, true)
}

/// The merge as the show itself looks, effects included but without the
/// grand master or highlight — what recording a preset should capture.
pub fn merge_universe_unmastered(state: &EngineState, universe: u32) -> [u8; UNIVERSE_SIZE] {
    merge(state, &[universe], true, false).pop().map_or([0u8; UNIVERSE_SIZE], |(_, frame)| frame)
}

/// The merge without any layer's effect output (and no grand master or
/// highlight) — what a relative effect's "underlying value" is for a channel
/// its own layer doesn't set.
pub fn merge_universe_static(state: &EngineState, universe: u32) -> [u8; UNIVERSE_SIZE] {
    merge(state, &[universe], false, false).pop().map_or([0u8; UNIVERSE_SIZE], |(_, frame)| frame)
}

/// One channel of `merge_universe_static`, as a float — where a programmer
/// TIME fade starts from when the programmer has no value there yet.
pub fn merge_channel_static(state: &EngineState, key: ChannelKey) -> f32 {
    let layers = sorted_layers(state);
    if state.is_htp(key.0, key.1) {
        layers
            .iter()
            .filter_map(|(_, layer)| layer.values.get(&key).map(|v| v * layer.intensity_scale))
            .fold(0.0f32, f32::max)
    } else {
        layers.iter().find_map(|(_, layer)| layer.values.get(&key).copied()).unwrap_or(0.0)
    }
}

fn sorted_layers(state: &EngineState) -> Vec<(i64, &Layer)> {
    let mut layers: Vec<(i64, &Layer)> =
        state.playbacks.values().map(|(priority, layer)| (*priority, layer)).collect();
    layers.push((PROGRAMMER_PRIORITY, &state.programmer));
    layers.sort_by_key(|(priority, _)| std::cmp::Reverse(*priority));
    layers
}

/// Per-universe scratch for one merge.
struct Scratch<'a> {
    htp: Option<&'a HashSet<u16>>,
    /// Highest scaled value seen per HTP channel.
    htp_max: [f32; UNIVERSE_SIZE],
    /// LTP channel already won by a higher-priority layer.
    filled: [bool; UNIVERSE_SIZE],
    /// Merged values, kept as floats until the output stage.
    values: [f32; UNIVERSE_SIZE],
}

/// Walks each layer's (sparse) values once, highest priority first, instead
/// of probing every layer for every one of the 512 channels: cost scales with
/// how many channels are actually driven, not with universes × 512 × layers.
/// `output_stage` applies highlight/lowlight and the grand master on top.
fn merge(
    state: &EngineState,
    universes: &[u32],
    with_effects: bool,
    output_stage: bool,
) -> Vec<(u32, [u8; UNIVERSE_SIZE])> {
    let layers = sorted_layers(state);

    let mut slots: HashMap<u32, usize> = HashMap::with_capacity(universes.len());
    let mut scratch: Vec<Scratch> = Vec::with_capacity(universes.len());
    for &universe in universes {
        slots.entry(universe).or_insert_with(|| {
            scratch.push(Scratch {
                htp: state.htp_channels.get(&universe),
                htp_max: [0.0; UNIVERSE_SIZE],
                filled: [false; UNIVERSE_SIZE],
                values: [0.0; UNIVERSE_SIZE],
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
                scratch.values[i] = value;
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
        .map(|&universe| {
            let scratch = &mut scratch[slots[&universe]];
            let mut values = scratch.values;
            if let Some(set) = scratch.htp {
                for &channel in set {
                    if channel >= 1 && channel as usize <= UNIVERSE_SIZE {
                        values[channel as usize - 1] = scratch.htp_max[channel as usize - 1];
                    }
                }
            }
            if output_stage {
                if let Some(highlight) = &state.highlight {
                    for (i, value) in values.iter_mut().enumerate() {
                        let key = (universe, (i + 1) as u16);
                        if let Some(&lit) = highlight.lit.get(&key) {
                            *value = lit;
                        } else if highlight.dimmed.contains(&key) {
                            *value *= LOWLIGHT_SCALE;
                        }
                    }
                }
                if let Some(set) = scratch.htp {
                    for &channel in set {
                        if channel >= 1 && channel as usize <= UNIVERSE_SIZE {
                            values[channel as usize - 1] *= state.grand_master;
                        }
                    }
                }
            }
            let mut out = [0u8; UNIVERSE_SIZE];
            for (slot, value) in out.iter_mut().zip(values) {
                *slot = to_u8(value);
            }
            (universe, out)
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
    fn grand_master_scales_htp_channels_only_and_not_the_static_view() {
        let mut state = EngineState::new();
        state.htp_channels.entry(1).or_default().insert(1);
        state.programmer.values.insert((1, 1), 200.0);
        state.programmer.values.insert((1, 2), 200.0);
        state.grand_master = 0.5;

        let out = merge_universe(&state, 1);
        assert_eq!(out[0], 100);
        assert_eq!(out[1], 200); // colour is not mastered
        assert_eq!(merge_universe_unmastered(&state, 1)[0], 200);
        assert_eq!(merge_universe_static(&state, 1)[0], 200);
        assert_eq!(merge_channel_static(&state, (1, 1)), 200.0);
    }

    #[test]
    fn highlight_overrides_selected_and_dims_others_without_touching_layers() {
        use crate::engine::state::Highlight;
        let mut state = EngineState::new();
        state.htp_channels.entry(1).or_default().extend([1, 11]);
        state.programmer.values.insert((1, 1), 40.0); // selected fixture, dim
        state.programmer.values.insert((1, 11), 200.0); // other fixture
        state.highlight = Some(Highlight {
            lit: [((1, 1), 255.0), ((1, 2), 255.0)].into_iter().collect(),
            dimmed: [(1, 11)].into_iter().collect(),
        });

        let out = merge_universe(&state, 1);
        assert_eq!(out[0], 255);
        assert_eq!(out[1], 255); // open white on a channel nothing set
        assert_eq!(out[10], 50); // 200 * 0.25
        // The underlying data is untouched, so turning highlight off restores it.
        assert_eq!(state.programmer.values[&(1, 1)], 40.0);
        state.highlight = None;
        assert_eq!(merge_universe(&state, 1)[0], 40);
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
    fn sources_attribute_each_channel_to_its_winning_layer() {
        let mut state = EngineState::new();
        state.htp_channels.entry(1).or_default().insert(1);
        state.playbacks.insert("cue-playback-1".into(), (1, layer(&[(1, 100.0), (2, 50.0), (3, 60.0)])));
        state.programmer.values.insert((1, 2), 10.0);
        let mut fx = layer(&[]);
        fx.fx_values.insert((1, 4), 7.0);
        state.playbacks.insert("fx".into(), (0, fx));
        let tracked: TrackedChannels = [("cue-playback-1".to_string(), [(1u32, 3u16)].into())].into();

        let sources = channel_sources(&state, 1, &tracked);
        assert_eq!(sources[0], ChannelSource::Playback); // HTP from the playback
        assert_eq!(sources[1], ChannelSource::Programmer); // programmer outranks
        assert_eq!(sources[2], ChannelSource::Tracked);
        assert_eq!(sources[3], ChannelSource::Effect);
        assert_eq!(sources[4], ChannelSource::Default);
    }

    #[test]
    fn htp_source_follows_the_brightest_layer_not_the_priority() {
        let mut state = EngineState::new();
        state.htp_channels.entry(1).or_default().insert(1);
        state.playbacks.insert("bright".into(), (1, layer(&[(1, 255.0)])));
        state.programmer.values.insert((1, 1), 20.0);
        let sources = channel_sources(&state, 1, &TrackedChannels::new());
        assert_eq!(sources[0], ChannelSource::Playback);
        assert_eq!(merge_universe(&state, 1)[0], 255);
    }

    #[test]
    fn programmer_effects_count_as_effect_and_other_universes_are_default() {
        let mut state = EngineState::new();
        state.programmer.fx_values.insert((1, 9), 5.0);
        let sources = channel_sources(&state, 1, &TrackedChannels::new());
        assert_eq!(sources[8], ChannelSource::Effect);
        assert!(channel_sources(&state, 2, &TrackedChannels::new()).iter().all(|s| *s == ChannelSource::Default));
    }
}
