//! A running playback: GO/Back/Pause/Release through a cuelist, with a
//! time-based crossfade between cues, a fader (intensity master, manual
//! crossfade, or rate), and — for a Chase cuelist — BPM-driven auto-advance.
//! Renders into a plain channel-value map every tick, the same shape as a
//! legacy asset's layer, so `manager::EngineManager`'s tick loop treats both
//! uniformly (see `manager.rs`'s "cue playbacks" section).
//!
//! Cue timing: every channel in a transition has its own delay and fade.
//! Intensity (HTP) channels use the cue's in-timing when they rise and its
//! out-timing when they fall; every other attribute uses the cue's
//! attribute timing, which defaults to the in-timing. Cues can auto-follow
//! (`follow_ms` / `wait_ms`), link to another cue with an optional loop
//! count, and be marked (move-in-black).
//!
//! Hook for other drivers (e.g. timecode): `go_to(cuelist, presets, index)`
//! jumps to any cue with that cue's own timing.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::cues::{resolve_cue_state, ChaseDirection, Cuelist, CuelistKind};
use super::effects::EffectInstance;
use super::presets::PresetStore;
use super::state::{ChannelKey, LayerEffect};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum FaderMode {
    /// The fader (0.0-1.0) scales this playback's HTP/intensity channels;
    /// everything else plays at the cue's recorded value.
    IntensityMaster,
    /// The fader position *is* the crossfade position between the previous
    /// and current cue (manual crossfade), instead of time-based.
    Crossfade,
    /// The fader is this playback's speed: 0-100% maps to 0x-2x (50% is
    /// normal), scaling its fades, delays, follows and chase steps.
    Rate,
}

/// What a playback needs to know about the patch to time and mark cues:
/// which channels are intensity, and each fixture attribute's intensity
/// channel. Rebuilt by the manager whenever the patch or HTP set changes.
#[derive(Debug, Default)]
pub struct PlayInfo {
    pub htp: HashMap<u32, HashSet<u16>>,
    /// Non-intensity attribute channel -> its fixture's intensity channel.
    pub intensity_of: HashMap<ChannelKey, ChannelKey>,
}

impl PlayInfo {
    pub fn is_htp(&self, key: ChannelKey) -> bool {
        self.htp.get(&key.0).is_some_and(|set| set.contains(&key.1))
    }
}

/// One channel moving from `from` to `to`.
#[derive(Debug, Clone, Copy)]
struct Transition {
    from: f32,
    to: f32,
    delay_ms: f64,
    fade_ms: f64,
}

pub struct PlaybackRuntime {
    pub id: u32,
    pub cuelist_id: u32,
    /// Determines LTP precedence against other playbacks and the legacy
    /// playback layers sharing the same merge — see `manager.rs`.
    pub priority: i64,
    /// Which bank (page) and column this playback is assigned to.
    pub page: u32,
    pub column: u32,
    pub fader_mode: FaderMode,
    pub fader: f32, // 0.0-1.0
    pub current_index: Option<usize>,
    pub paused: bool,
    /// The global rate master (1.0 = normal); scales a chase's step speed.
    pub rate_master: f32,
    pub info: Arc<PlayInfo>,
    transitions: HashMap<ChannelKey, Transition>,
    /// The outgoing and incoming cue's effects, crossfaded like the values.
    from_effects: Vec<EffectInstance>,
    to_effects: Vec<EffectInstance>,
    /// The current cue's own in-timing (what effects crossfade over).
    delay_ms: f64,
    fade_ms: f64,
    /// When the slowest channel of the current transition is done.
    total_ms: f64,
    elapsed_ms: f64,
    /// Time since the last `go()`, for follows.
    step_elapsed_ms: f64,
    /// Chase step timer: like `step_elapsed_ms` but also scaled by the rate
    /// master.
    chase_elapsed_ms: f64,
    chase_forward: bool,
    /// How many times each cue's link has been taken.
    loop_counts: HashMap<usize, u32>,
    /// Move-in-black: next-cue attribute values already applied while their
    /// fixtures are dark, and which cue index they were computed for.
    marks: HashMap<ChannelKey, f32>,
    marks_for: Option<usize>,
    /// The current cue has nothing left to follow into.
    follow_done: bool,
}

impl PlaybackRuntime {
    pub fn new(id: u32, cuelist_id: u32, priority: i64) -> Self {
        Self {
            id,
            cuelist_id,
            priority,
            page: 0,
            column: 0,
            fader_mode: FaderMode::IntensityMaster,
            fader: 1.0,
            current_index: None,
            paused: false,
            rate_master: 1.0,
            info: Arc::new(PlayInfo::default()),
            transitions: HashMap::new(),
            from_effects: Vec::new(),
            to_effects: Vec::new(),
            delay_ms: 0.0,
            fade_ms: 1.0,
            total_ms: 1.0,
            elapsed_ms: 0.0,
            step_elapsed_ms: 0.0,
            chase_elapsed_ms: 0.0,
            chase_forward: true,
            loop_counts: HashMap::new(),
            marks: HashMap::new(),
            marks_for: None,
            follow_done: false,
        }
    }

    /// How far the current crossfade has got, 0.0-1.0 (for effects).
    fn progress(&self) -> f32 {
        if self.fader_mode == FaderMode::Crossfade {
            self.fader
        } else {
            ((self.elapsed_ms - self.delay_ms) / self.fade_ms).clamp(0.0, 1.0) as f32
        }
    }

    /// Time speed of this playback: 1.0 except in rate mode.
    pub fn speed(&self) -> f64 {
        match self.fader_mode {
            FaderMode::Rate => (self.fader * 2.0) as f64,
            _ => 1.0,
        }
    }

    /// This tick's blended (universe,channel)->value state. The fader isn't
    /// applied here: in intensity-master mode it's the layer's
    /// `intensity_scale` (see `intensity_scale`), applied at merge time so
    /// it also scales effects on intensity channels.
    pub fn render_values(&self) -> HashMap<ChannelKey, f32> {
        let manual = self.fader_mode == FaderMode::Crossfade;
        let mut out: HashMap<ChannelKey, f32> = self
            .transitions
            .iter()
            .map(|(&key, t)| {
                let p = if manual {
                    self.fader
                } else {
                    ((self.elapsed_ms - t.delay_ms) / t.fade_ms).clamp(0.0, 1.0) as f32
                };
                (key, t.from + (t.to - t.from) * p)
            })
            .collect();
        out.extend(self.marks.iter().map(|(&k, &v)| (k, v)));
        out
    }

    /// What this playback's layer's HTP channels are multiplied by.
    pub fn intensity_scale(&self) -> f32 {
        match self.fader_mode {
            FaderMode::IntensityMaster => self.fader,
            FaderMode::Crossfade | FaderMode::Rate => 1.0,
        }
    }

    /// This tick's effects with their crossfade weights: the incoming cue's
    /// fade in as the outgoing cue's fade out. An effect both cues share
    /// (the same recorded instance) just keeps running at full weight.
    pub fn effects(&self) -> Vec<LayerEffect> {
        let t = self.progress();
        let mut effects: Vec<LayerEffect> = self
            .from_effects
            .iter()
            .filter(|e| !self.to_effects.contains(e))
            .map(|e| LayerEffect { instance: e.clone(), scale: 1.0 - t })
            .filter(|e| e.scale > 0.0)
            .collect();
        effects.extend(self.to_effects.iter().map(|e| LayerEffect {
            instance: e.clone(),
            scale: if self.from_effects.contains(e) { 1.0 } else { t },
        }));
        effects
    }

    /// Jumps to cue `target_index` and starts its transition. The stable
    /// entry point for anything that fires cues (manual GO, follows, links,
    /// timecode).
    pub fn go_to(&mut self, cuelist: &Cuelist, presets: &PresetStore, target_index: usize) {
        self.go(cuelist, presets, target_index);
    }

    pub fn go(&mut self, cuelist: &Cuelist, presets: &PresetStore, target_index: usize) {
        let from = self.render_values();
        let to = resolve_cue_state(cuelist, target_index, presets);
        let cue = cuelist.cues.get(target_index);
        let (delay_in, fade_in, delay_out, fade_out, attr_delay, attr_fade) =
            cue.map_or((0.0, 1.0, 0.0, 1.0, 0.0, 1.0), |c| {
                (
                    c.delay_in_ms.max(0.0),
                    c.fade_in_ms.max(1.0),
                    c.delay_out_ms.max(0.0),
                    c.fade_out_ms.max(1.0),
                    c.attr_delay_ms.unwrap_or(c.delay_in_ms).max(0.0),
                    c.attr_fade_ms.unwrap_or(c.fade_in_ms).max(1.0),
                )
            });

        let mut keys: HashSet<ChannelKey> = from.keys().copied().collect();
        keys.extend(to.keys().copied());
        self.transitions = keys
            .into_iter()
            .map(|key| {
                let f = from.get(&key).copied().unwrap_or(0.0);
                let t = to.get(&key).copied().unwrap_or(0.0);
                let (delay_ms, fade_ms) = if self.info.is_htp(key) {
                    if t >= f {
                        (delay_in, fade_in)
                    } else {
                        (delay_out, fade_out)
                    }
                } else {
                    (attr_delay, attr_fade)
                };
                (key, Transition { from: f, to: t, delay_ms, fade_ms })
            })
            .collect();
        self.total_ms = self
            .transitions
            .values()
            .map(|t| t.delay_ms + t.fade_ms)
            .fold(delay_in + fade_in, f64::max)
            .max(1.0);
        self.delay_ms = delay_in;
        self.fade_ms = fade_in;
        self.from_effects = std::mem::take(&mut self.to_effects);
        self.to_effects = cue.map(|c| c.effects.clone()).unwrap_or_default();
        self.elapsed_ms = 0.0;
        self.step_elapsed_ms = 0.0;
        self.chase_elapsed_ms = 0.0;
        self.marks.clear();
        self.marks_for = None;
        self.follow_done = false;
        self.current_index = Some(target_index);
    }

    /// The cue a GO from `index` leads to: the cue's link (while its loop
    /// count lasts) or else the next in order.
    fn next_target(&mut self, cuelist: &Cuelist, index: usize) -> Option<usize> {
        if let Some(cue) = cuelist.cues.get(index) {
            if let Some(target) = cue.link_to.and_then(|n| cuelist.index_of(n)) {
                match cue.loop_count {
                    None => return Some(target),
                    Some(max) => {
                        let used = self.loop_counts.entry(index).or_insert(0);
                        if *used < max {
                            *used += 1;
                            return Some(target);
                        }
                        *used = 0; // exhausted: carry on, and let a later pass loop again
                    }
                }
            }
        }
        (index + 1 < cuelist.cues.len()).then_some(index + 1)
    }

    pub fn go_next(&mut self, cuelist: &Cuelist, presets: &PresetStore) {
        let next = match self.current_index {
            None => (!cuelist.cues.is_empty()).then_some(0),
            Some(i) => self.next_target(cuelist, i),
        };
        if let Some(next) = next {
            self.go(cuelist, presets, next);
        }
    }

    pub fn go_back(&mut self, cuelist: &Cuelist, presets: &PresetStore) {
        if let Some(index) = self.current_index {
            if index > 0 {
                self.go(cuelist, presets, index - 1);
            }
        }
    }

    /// Fades everything this playback is contributing down to nothing and
    /// forgets its cue position.
    pub fn release(&mut self, release_fade_ms: f64) {
        let fade_ms = release_fade_ms.max(1.0);
        self.transitions = self
            .render_values()
            .into_iter()
            .map(|(key, from)| (key, Transition { from, to: 0.0, delay_ms: 0.0, fade_ms }))
            .collect();
        self.from_effects = std::mem::take(&mut self.to_effects);
        self.delay_ms = 0.0;
        self.fade_ms = fade_ms;
        self.total_ms = fade_ms;
        self.elapsed_ms = 0.0;
        self.marks.clear();
        self.marks_for = None;
        self.loop_counts.clear();
        self.current_index = None;
    }

    /// Advances crossfade time and, for a Chase, auto-GOes to the next cue
    /// once a full beat interval has elapsed. For other cuelists, fires the
    /// current cue's auto-follow / wait and prepares marks.
    pub fn tick(&mut self, dt_ms: f64, cuelist: &Cuelist, presets: &PresetStore) {
        if self.paused {
            return;
        }
        let dt = dt_ms * self.speed();
        self.elapsed_ms += dt;
        self.step_elapsed_ms += dt;
        self.chase_elapsed_ms += dt * self.rate_master as f64;

        if let CuelistKind::Chase { bpm, direction } = cuelist.kind {
            if bpm <= 0.0 || cuelist.cues.is_empty() {
                return;
            }
            if self.current_index.is_none() {
                self.go(cuelist, presets, 0);
                return;
            }
            let beat_ms = 60_000.0 / bpm as f64;
            if self.chase_elapsed_ms >= beat_ms {
                let next = self.next_chase_index(cuelist.cues.len(), direction);
                self.go(cuelist, presets, next);
            }
            return;
        }

        let Some(index) = self.current_index else { return };
        let Some(cue) = cuelist.cues.get(index) else { return };
        if cuelist.kind == CuelistKind::Submaster {
            return;
        }

        self.update_marks(cuelist, presets, index);

        if !self.follow_done {
            let by_follow = cue.follow_ms.is_some_and(|f| self.step_elapsed_ms >= f);
            let by_wait = cue.wait_ms.is_some_and(|w| self.elapsed_ms >= self.total_ms + w);
            if by_follow || by_wait {
                match self.next_target(cuelist, index) {
                    Some(next) => self.go(cuelist, presets, next),
                    None => self.follow_done = true,
                }
            }
        }
    }

    /// Move-in-black: once cue `index` has finished fading, any fixture that
    /// is dark in it gets the next cue's non-intensity attributes applied
    /// early, if the next cue is marked.
    fn update_marks(&mut self, cuelist: &Cuelist, presets: &PresetStore, index: usize) {
        if self.marks_for == Some(index) || self.elapsed_ms < self.total_ms {
            return;
        }
        self.marks_for = Some(index);
        if !cuelist.cues.get(index + 1).is_some_and(|c| c.mark) {
            return;
        }
        let rendered = self.render_values();
        for (key, value) in resolve_cue_state(cuelist, index + 1, presets) {
            let Some(intensity) = self.info.intensity_of.get(&key) else { continue };
            let dark = rendered.get(intensity).copied().unwrap_or(0.0) < 0.5;
            if dark && rendered.get(&key).copied().unwrap_or(0.0) != value {
                self.marks.insert(key, value);
            }
        }
    }

    fn next_chase_index(&mut self, len: usize, direction: ChaseDirection) -> usize {
        let current = self.current_index.unwrap_or(0);
        match direction {
            ChaseDirection::Forward => (current + 1) % len,
            ChaseDirection::Bounce => {
                if len <= 1 {
                    return 0;
                }
                if self.chase_forward {
                    let next = current + 1;
                    if next >= len - 1 {
                        self.chase_forward = false;
                        next.min(len - 1)
                    } else {
                        next
                    }
                } else if current == 0 {
                    // Hit the bottom this step: reverse and move in the new
                    // (forward) direction immediately, not the old one.
                    self.chase_forward = true;
                    1.min(len - 1)
                } else {
                    current - 1
                }
            }
            ChaseDirection::Random => {
                let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
                (nanos as usize) % len
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cues::{Cue, CueValue};

    fn cue_with(number: f64, channel: u16, value: f32, fade_ms: f64) -> Cue {
        let mut cue = Cue::new(number, format!("Cue {number}"));
        cue.values.insert((1, channel), CueValue::Literal { value });
        cue.fade_in_ms = fade_ms;
        cue
    }

    #[test]
    fn go_crossfades_linearly_over_the_target_cues_fade_time() {
        let mut list = Cuelist::new(1, "L".to_string(), CuelistKind::Standard, true);
        list.upsert_cue(cue_with(1.0, 1, 200.0, 100.0));
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.go(&list, &presets, 0);
        pb.tick(50.0, &list, &presets);
        let mid = pb.render_values();
        assert_eq!(mid[&(1, 1)].round() as u8, 100); // halfway through a 100ms fade to 200

        pb.tick(50.0, &list, &presets);
        let done = pb.render_values();
        assert_eq!(done[&(1, 1)].round() as u8, 200);
    }

    #[test]
    fn intensity_master_fader_becomes_the_layers_intensity_scale() {
        let mut list = Cuelist::new(1, "L".to_string(), CuelistKind::Standard, true);
        let mut cue = Cue::new(1.0, "Cue 1".to_string());
        cue.values.insert((1, 1), CueValue::Literal { value: 200.0 }); // intensity
        cue.values.insert((1, 2), CueValue::Literal { value: 200.0 }); // color, not HTP
        cue.fade_in_ms = 1.0;
        list.upsert_cue(cue);
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.fader = 0.5;
        pb.go(&list, &presets, 0);
        pb.tick(10.0, &list, &presets);

        // The fader becomes the layer's intensity scale; the merge applies
        // it to HTP channels only (see merge.rs's tests).
        assert_eq!(pb.intensity_scale(), 0.5);
        let rendered = pb.render_values();
        assert_eq!(rendered[&(1, 1)], 200.0);
        assert_eq!(rendered[&(1, 2)], 200.0);
        pb.fader_mode = FaderMode::Crossfade;
        assert_eq!(pb.intensity_scale(), 1.0);
    }

    #[test]
    fn go_back_returns_to_the_previous_cue() {
        let mut list = Cuelist::new(1, "L".to_string(), CuelistKind::Standard, true);
        list.upsert_cue(cue_with(1.0, 1, 100.0, 1.0));
        list.upsert_cue(cue_with(2.0, 1, 200.0, 1.0));
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.go(&list, &presets, 1);
        pb.tick(10.0, &list, &presets);
        assert_eq!(pb.render_values()[&(1, 1)], 200.0);

        pb.go_back(&list, &presets);
        pb.tick(10.0, &list, &presets);
        assert_eq!(pb.render_values()[&(1, 1)], 100.0);
    }

    #[test]
    fn release_fades_to_nothing_and_forgets_cue_position() {
        let mut list = Cuelist::new(1, "L".to_string(), CuelistKind::Standard, true);
        list.upsert_cue(cue_with(1.0, 1, 200.0, 1.0));
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.go(&list, &presets, 0);
        pb.tick(10.0, &list, &presets);
        pb.release(100.0);
        pb.tick(100.0, &list, &presets);

        assert_eq!(pb.current_index, None);
        assert_eq!(pb.render_values().get(&(1, 1)), Some(&0.0));
    }

    #[test]
    fn chase_auto_advances_forward_at_the_bpm_derived_interval() {
        let mut list = Cuelist::new(
            1,
            "Chase".to_string(),
            CuelistKind::Chase { bpm: 120.0, direction: ChaseDirection::Forward },
            true,
        );
        // 120 BPM = 500ms/beat.
        list.upsert_cue(cue_with(1.0, 1, 100.0, 1.0));
        list.upsert_cue(cue_with(2.0, 1, 200.0, 1.0));
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.tick(1.0, &list, &presets); // starts on cue 0
        assert_eq!(pb.current_index, Some(0));

        pb.tick(500.0, &list, &presets); // one full beat -> advance to cue 1
        assert_eq!(pb.current_index, Some(1));

        pb.tick(500.0, &list, &presets); // wraps back to cue 0 (Forward loops)
        assert_eq!(pb.current_index, Some(0));
    }

    #[test]
    fn chase_bounce_reverses_at_each_end() {
        let mut list = Cuelist::new(
            1,
            "Chase".to_string(),
            CuelistKind::Chase { bpm: 600.0, direction: ChaseDirection::Bounce }, // 100ms/beat
            true,
        );
        for i in 0..3 {
            list.upsert_cue(cue_with(i as f64 + 1.0, 1, i as f32, 1.0));
        }
        let presets = PresetStore::new();
        let mut pb = PlaybackRuntime::new(1, 1, 10);

        let mut indices = Vec::new();
        pb.tick(1.0, &list, &presets);
        indices.push(pb.current_index.unwrap());
        for _ in 0..5 {
            pb.tick(100.0, &list, &presets);
            indices.push(pb.current_index.unwrap());
        }
        // 0 -> 1 -> 2 -> 1 -> 0 -> 1 : bounces at both ends instead of wrapping.
        assert_eq!(indices, vec![0, 1, 2, 1, 0, 1]);
    }

    #[test]
    fn paused_playback_does_not_advance() {
        let mut list = Cuelist::new(
            1,
            "Chase".to_string(),
            CuelistKind::Chase { bpm: 600.0, direction: ChaseDirection::Forward },
            true,
        );
        list.upsert_cue(cue_with(1.0, 1, 1.0, 1.0));
        list.upsert_cue(cue_with(2.0, 1, 2.0, 1.0));
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.tick(1.0, &list, &presets);
        pb.paused = true;
        pb.tick(1000.0, &list, &presets);
        assert_eq!(pb.current_index, Some(0));
    }

    #[test]
    fn cue_effects_crossfade_and_shared_effects_keep_running() {
        use crate::engine::effects::{EffectDef, Waveform};
        let instance = |id| EffectInstance {
            id,
            def: EffectDef::new_attribute("dimmer", Waveform::Sine),
            preset_id: None,
            fixtures: vec![1],
        };
        let mut list = Cuelist::new(1, "L".to_string(), CuelistKind::Standard, true);
        let mut c1 = cue_with(1.0, 1, 0.0, 100.0);
        c1.effects = vec![instance(1), instance(2)];
        let mut c2 = cue_with(2.0, 1, 0.0, 100.0);
        c2.effects = vec![instance(2), instance(3)];
        list.upsert_cue(c1);
        list.upsert_cue(c2);
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.go(&list, &presets, 0);
        pb.tick(100.0, &list, &presets);
        pb.go(&list, &presets, 1);
        pb.tick(25.0, &list, &presets); // a quarter of the way into cue 2

        let scales: HashMap<u32, f32> = pb.effects().iter().map(|e| (e.instance.id, e.scale)).collect();
        assert_eq!(scales[&1], 0.75); // only in cue 1: fading out
        assert_eq!(scales[&2], 1.0); // in both: uninterrupted
        assert_eq!(scales[&3], 0.25); // only in cue 2: fading in

        pb.release(100.0);
        pb.tick(100.0, &list, &presets);
        assert!(pb.effects().is_empty());
    }

    /// Channel 1 is a dimmer (HTP); channel 2 is its fixture's pan.
    fn patch_info() -> Arc<PlayInfo> {
        Arc::new(PlayInfo {
            htp: [(1, [1u16].into_iter().collect())].into_iter().collect(),
            intensity_of: [((1, 2), (1, 1))].into_iter().collect(),
        })
    }

    fn two_channel_cue(number: f64, dimmer: f32, pan: f32) -> Cue {
        let mut cue = Cue::new(number, format!("Cue {number}"));
        cue.values.insert((1, 1), CueValue::Literal { value: dimmer });
        cue.values.insert((1, 2), CueValue::Literal { value: pan });
        cue
    }

    #[test]
    fn delay_in_holds_the_fade_back_and_delay_out_delays_falling_intensity() {
        let mut list = Cuelist::new(1, "L".to_string(), CuelistKind::Standard, true);
        let mut c1 = two_channel_cue(1.0, 200.0, 0.0);
        c1.fade_in_ms = 100.0;
        c1.delay_in_ms = 200.0;
        let mut c2 = two_channel_cue(2.0, 0.0, 0.0);
        c2.fade_out_ms = 100.0;
        c2.delay_out_ms = 300.0;
        list.upsert_cue(c1);
        list.upsert_cue(c2);
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.info = patch_info();
        pb.go(&list, &presets, 0);
        pb.tick(150.0, &list, &presets);
        assert_eq!(pb.render_values()[&(1, 1)], 0.0); // still inside the 200ms delay
        pb.tick(100.0, &list, &presets); // 250ms: 50ms into the fade
        assert_eq!(pb.render_values()[&(1, 1)].round(), 100.0);
        pb.tick(100.0, &list, &presets);
        assert_eq!(pb.render_values()[&(1, 1)], 200.0);

        pb.go(&list, &presets, 1);
        pb.tick(250.0, &list, &presets);
        assert_eq!(pb.render_values()[&(1, 1)], 200.0); // delay out not over yet
        pb.tick(100.0, &list, &presets); // 350ms: 50ms into the 100ms fade out
        assert_eq!(pb.render_values()[&(1, 1)].round(), 100.0);
    }

    #[test]
    fn attributes_can_fade_on_their_own_timing() {
        let mut list = Cuelist::new(1, "L".to_string(), CuelistKind::Standard, true);
        let mut c1 = two_channel_cue(1.0, 200.0, 100.0);
        c1.fade_in_ms = 100.0;
        c1.attr_fade_ms = Some(1000.0);
        c1.attr_delay_ms = Some(100.0);
        list.upsert_cue(c1);
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.info = patch_info();
        pb.go(&list, &presets, 0);
        pb.tick(100.0, &list, &presets);
        let v = pb.render_values();
        assert_eq!(v[&(1, 1)], 200.0); // intensity is done
        assert_eq!(v[&(1, 2)], 0.0); // pan is only starting its delay
        pb.tick(600.0, &list, &presets); // 600ms into pan's 1000ms fade (after 100ms delay)
        assert_eq!(pb.render_values()[&(1, 2)].round(), 60.0);
    }

    #[test]
    fn follow_fires_after_the_cue_starts_and_wait_after_its_fade() {
        let mut list = Cuelist::new(1, "L".to_string(), CuelistKind::Standard, true);
        let mut c1 = cue_with(1.0, 1, 10.0, 100.0);
        c1.follow_ms = Some(300.0);
        let mut c2 = cue_with(2.0, 1, 20.0, 100.0);
        c2.wait_ms = Some(50.0);
        let c3 = cue_with(3.0, 1, 30.0, 100.0);
        list.upsert_cue(c1);
        list.upsert_cue(c2);
        list.upsert_cue(c3);
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.go(&list, &presets, 0);
        pb.tick(299.0, &list, &presets);
        assert_eq!(pb.current_index, Some(0));
        pb.tick(2.0, &list, &presets);
        assert_eq!(pb.current_index, Some(1)); // follow fired at 300ms

        pb.tick(140.0, &list, &presets); // 100ms fade + 50ms wait = 150ms
        assert_eq!(pb.current_index, Some(1));
        pb.tick(20.0, &list, &presets);
        assert_eq!(pb.current_index, Some(2));

        // The last cue has nothing to follow into: it stays put.
        pb.tick(5000.0, &list, &presets);
        assert_eq!(pb.current_index, Some(2));
    }

    #[test]
    fn link_loops_a_limited_number_of_times_then_continues() {
        let mut list = Cuelist::new(1, "L".to_string(), CuelistKind::Standard, true);
        list.upsert_cue(cue_with(1.0, 1, 10.0, 1.0));
        let mut c2 = cue_with(2.0, 1, 20.0, 1.0);
        c2.link_to = Some(1.0);
        c2.loop_count = Some(2);
        list.upsert_cue(c2);
        list.upsert_cue(cue_with(3.0, 1, 30.0, 1.0));
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        let mut visited = Vec::new();
        for _ in 0..7 {
            pb.go_next(&list, &presets);
            visited.push(pb.current_index.unwrap() + 1);
        }
        // 1, 2, then the link is taken twice (-> 1, 2, -> 1, 2), then on to 3.
        assert_eq!(visited, vec![1, 2, 1, 2, 1, 2, 3]);
    }

    #[test]
    fn link_without_a_loop_count_loops_forever_via_auto_follow() {
        let mut list = Cuelist::new(1, "L".to_string(), CuelistKind::Standard, true);
        let mut c1 = cue_with(1.0, 1, 10.0, 1.0);
        c1.wait_ms = Some(0.0);
        let mut c2 = cue_with(2.0, 1, 20.0, 1.0);
        c2.wait_ms = Some(0.0);
        c2.link_to = Some(1.0);
        list.upsert_cue(c1);
        list.upsert_cue(c2);
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.go(&list, &presets, 0);
        let mut firings = 0;
        let mut last = pb.current_index;
        for _ in 0..40 {
            pb.tick(1.0, &list, &presets);
            if pb.current_index != last {
                firings += 1;
                last = pb.current_index;
            }
        }
        assert!(firings >= 10, "keeps cycling between the two cues, got {firings}");
    }

    #[test]
    fn marked_cue_pre_positions_attributes_only_on_dark_fixtures() {
        let mut list = Cuelist::new(1, "L".to_string(), CuelistKind::Standard, true);
        let mut c1 = two_channel_cue(1.0, 0.0, 10.0); // dark
        c1.fade_in_ms = 10.0;
        let mut c2 = two_channel_cue(2.0, 255.0, 200.0);
        c2.mark = true;
        c2.fade_in_ms = 1000.0;
        list.upsert_cue(c1);
        list.upsert_cue(c2);
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.info = patch_info();
        pb.go(&list, &presets, 0);
        pb.tick(5.0, &list, &presets);
        assert_eq!(pb.render_values()[&(1, 2)], 5.0); // still fading into cue 1
        pb.tick(10.0, &list, &presets); // cue 1 done, fixture dark -> mark applied
        let v = pb.render_values();
        assert_eq!(v[&(1, 2)], 200.0, "pan moved in the dark");
        assert_eq!(v[&(1, 1)], 0.0, "intensity untouched");

        pb.go(&list, &presets, 1);
        pb.tick(1.0, &list, &presets);
        assert_eq!(pb.render_values()[&(1, 2)], 200.0, "no visible move when the marked cue fires");
    }

    #[test]
    fn mark_is_not_applied_while_the_fixture_is_lit() {
        let mut list = Cuelist::new(1, "L".to_string(), CuelistKind::Standard, true);
        let mut c1 = two_channel_cue(1.0, 255.0, 10.0); // lit
        c1.fade_in_ms = 10.0;
        let mut c2 = two_channel_cue(2.0, 255.0, 200.0);
        c2.mark = true;
        list.upsert_cue(c1);
        list.upsert_cue(c2);
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.info = patch_info();
        pb.go(&list, &presets, 0);
        pb.tick(50.0, &list, &presets);
        assert_eq!(pb.render_values()[&(1, 2)], 10.0);
    }

    #[test]
    fn rate_fader_mode_scales_the_playbacks_own_time() {
        let mut list = Cuelist::new(1, "L".to_string(), CuelistKind::Standard, true);
        list.upsert_cue(cue_with(1.0, 1, 200.0, 1000.0));
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.fader_mode = FaderMode::Rate;
        pb.fader = 1.0; // 2x
        pb.go(&list, &presets, 0);
        pb.tick(250.0, &list, &presets);
        assert_eq!(pb.render_values()[&(1, 1)].round(), 100.0); // 500ms of fade in 250ms
        assert_eq!(pb.intensity_scale(), 1.0);

        pb.fader = 0.0; // stopped
        pb.tick(1000.0, &list, &presets);
        assert_eq!(pb.render_values()[&(1, 1)].round(), 100.0);
    }

    #[test]
    fn rate_master_scales_chase_speed() {
        let mut list = Cuelist::new(
            1,
            "Chase".to_string(),
            CuelistKind::Chase { bpm: 120.0, direction: ChaseDirection::Forward }, // 500ms/beat
            true,
        );
        list.upsert_cue(cue_with(1.0, 1, 1.0, 1.0));
        list.upsert_cue(cue_with(2.0, 1, 2.0, 1.0));
        let presets = PresetStore::new();

        let mut pb = PlaybackRuntime::new(1, 1, 10);
        pb.rate_master = 2.0;
        pb.tick(1.0, &list, &presets);
        pb.tick(250.0, &list, &presets); // 250ms at 2x = one beat
        assert_eq!(pb.current_index, Some(1));
        pb.rate_master = 0.5;
        pb.tick(500.0, &list, &presets); // 250ms worth: not yet
        assert_eq!(pb.current_index, Some(1));
        pb.tick(500.0, &list, &presets);
        assert_eq!(pb.current_index, Some(0));
    }
}
