//! Cuelists and cues (BUILD_PLAN Phase 6). A cue's value can be a literal
//! number or a reference to a Phase 5 preset — resolved fresh every time
//! (`resolve_cue_value`/`resolve_cue_state`), the same "never copy, always
//! look up" mechanism `presets::PresetStore::apply` already uses, so updating
//! a preset really does change every cue that references it.

use std::collections::HashMap;
use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::effects::EffectInstance;
use super::presets::PresetStore;

// `Cue.values` is keyed by `(u32, u16)`, which serde_json can't serialize as
// a JSON object (object keys must be strings) — `CueValue`/`ChaseDirection`/
// `CuelistKind` don't have that problem and derive (de)serialization
// directly; `Cue`/`Cuelist` cross the Tauri boundary via the `CueDto`/
// `CuelistDto` conversion in `manager.rs` instead, which flattens `values`
// into a plain list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum CueValue {
    #[serde(rename_all = "camelCase")]
    Literal { value: f32 },
    #[serde(rename_all = "camelCase")]
    Preset { preset_id: u32, attribute: String },
}

#[derive(Debug, Clone)]
pub struct Cue {
    pub number: f64,
    pub name: String,
    pub values: HashMap<(u32, u16), CueValue>,
    pub fade_in_ms: f64,
    pub fade_out_ms: f64,
    pub delay_in_ms: f64,
    /// Move-in-black: this cue's moving-fixture attributes (pan/tilt/etc.)
    /// should snap while dark rather than move live. Stored for the UI/a
    /// future engine consumer; the merge/playback path doesn't act on it yet
    /// since there's no "blackout while moving" timing model built.
    pub mark: bool,
    /// Effects that run while this cue is live (Phase 7). Unlike `values`,
    /// effects don't track: a cue runs only its own. Every recorded cue
    /// holds a literal for every channel it captured, so a tracked effect
    /// could never be stopped by a later cue — see DECISIONS.md.
    pub effects: Vec<EffectInstance>,
}

impl Cue {
    pub fn new(number: f64, name: String) -> Self {
        Self {
            number,
            name,
            values: HashMap::new(),
            fade_in_ms: 0.0,
            fade_out_ms: 0.0,
            delay_in_ms: 0.0,
            mark: false,
            effects: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum ChaseDirection {
    Forward,
    Bounce,
    Random,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum CuelistKind {
    Standard,
    #[serde(rename_all = "camelCase")]
    Chase { bpm: f32, direction: ChaseDirection },
    /// Runs at a priority tier above standard playbacks (below the
    /// programmer) via `manager::OVERRIDE_PRIORITY_BASE` — an override
    /// cuelist's cues win LTP over any standard/chase playback regardless of
    /// trigger order.
    Override,
    /// A fader directly *is* the cuelist's intensity, with no GO/cue
    /// stepping — always renders cue 0's (tracked) state, scaled by the
    /// playback's fader.
    Submaster,
    /// Per-cue fire times exist (`Cue`'s eventual timecode field would live
    /// here), but nothing drives playback from an external clock yet — LTC/
    /// MTC input is Phase 8. A Timecode cuelist behaves like Standard
    /// (manual GO) until then.
    Timecode,
}

#[derive(Debug, Clone)]
pub struct Cuelist {
    pub id: u32,
    pub name: String,
    pub kind: CuelistKind,
    /// On: each cue's effective state includes every earlier cue's values
    /// for channels this cue doesn't itself touch. Off: a cue is absolute —
    /// only the channels it explicitly sets.
    pub tracking: bool,
    /// Kept sorted by `number`.
    pub cues: Vec<Cue>,
}

impl Cuelist {
    pub fn new(id: u32, name: String, kind: CuelistKind, tracking: bool) -> Self {
        Self { id, name, kind, tracking, cues: Vec::new() }
    }

    /// Inserts a cue in number order, replacing any existing cue with the
    /// same number (UPDATE semantics).
    pub fn upsert_cue(&mut self, cue: Cue) {
        match self.cues.binary_search_by(|c| c.number.partial_cmp(&cue.number).unwrap()) {
            Ok(idx) => self.cues[idx] = cue,
            Err(idx) => self.cues.insert(idx, cue),
        }
    }

    pub fn remove_cue(&mut self, number: f64) {
        self.cues.retain(|c| (c.number - number).abs() > f64::EPSILON);
    }

    pub fn index_of(&self, number: f64) -> Option<usize> {
        self.cues.iter().position(|c| (c.number - number).abs() < f64::EPSILON)
    }
}

pub fn resolve_cue_value(value: &CueValue, presets: &PresetStore) -> f32 {
    match value {
        CueValue::Literal { value } => *value,
        CueValue::Preset { preset_id, attribute } => {
            presets.get(*preset_id).and_then(|p| p.values.get(attribute)).copied().unwrap_or(0.0)
        }
    }
}

/// Cue `index`'s effective (universe,channel)->value state. With tracking on,
/// this layers cue `index`'s own values over every earlier cue's (a later
/// cue's value for the same channel wins) — computed fresh every call, so
/// editing an earlier cue immediately changes what every later cue tracks
/// in, exactly like a real console.
pub fn resolve_cue_state(cuelist: &Cuelist, index: usize, presets: &PresetStore) -> HashMap<(u32, u16), f32> {
    let mut state = HashMap::new();
    if index >= cuelist.cues.len() {
        return state;
    }
    let range: RangeInclusive<usize> = if cuelist.tracking { 0..=index } else { index..=index };
    for cue in &cuelist.cues[range] {
        for (key, value) in &cue.values {
            state.insert(*key, resolve_cue_value(value, presets));
        }
    }
    state
}

#[derive(Default)]
pub struct CueStore {
    cuelists: HashMap<u32, Cuelist>,
    next_id: u32,
}

impl CueStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&mut self, name: String, kind: CuelistKind, tracking: bool) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        self.cuelists.insert(id, Cuelist::new(id, name, kind, tracking));
        id
    }

    pub fn get(&self, id: u32) -> Option<&Cuelist> {
        self.cuelists.get(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Cuelist> {
        self.cuelists.get_mut(&id)
    }

    pub fn list(&self) -> Vec<Cuelist> {
        let mut lists: Vec<Cuelist> = self.cuelists.values().cloned().collect();
        lists.sort_by_key(|c| c.id);
        lists
    }

    pub fn delete(&mut self, id: u32) {
        self.cuelists.remove(&id);
    }

    /// Replaces every cuelist (loading a show). New ids continue after the
    /// highest loaded one.
    pub fn replace_all(&mut self, cuelists: Vec<Cuelist>) {
        self.next_id = cuelists.iter().map(|c| c.id).max().unwrap_or(0);
        self.cuelists = cuelists.into_iter().map(|c| (c.id, c)).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::presets::PresetFamily;

    fn literal_cue(number: f64, entries: &[((u32, u16), f32)]) -> Cue {
        let mut cue = Cue::new(number, format!("Cue {number}"));
        for &(key, value) in entries {
            cue.values.insert(key, CueValue::Literal { value });
        }
        cue
    }

    #[test]
    fn tracking_carries_forward_untouched_channels() {
        let mut list = Cuelist::new(1, "List".to_string(), CuelistKind::Standard, true);
        list.upsert_cue(literal_cue(1.0, &[((1, 1), 255.0), ((1, 2), 0.0)]));
        list.upsert_cue(literal_cue(2.0, &[((1, 2), 128.0)])); // only touches channel 2
        list.upsert_cue(literal_cue(3.0, &[((1, 3), 64.0)])); // touches a new channel

        let presets = PresetStore::new();
        let state_at_3 = resolve_cue_state(&list, 2, &presets);
        // Channel 1 tracks forward from cue 1 (255), channel 2 from cue 2
        // (128, overriding cue 1's 0), channel 3 is cue 3's own value.
        assert_eq!(state_at_3.get(&(1, 1)), Some(&255.0));
        assert_eq!(state_at_3.get(&(1, 2)), Some(&128.0));
        assert_eq!(state_at_3.get(&(1, 3)), Some(&64.0));
    }

    #[test]
    fn tracking_off_means_only_the_cues_own_values() {
        let mut list = Cuelist::new(1, "List".to_string(), CuelistKind::Standard, false);
        list.upsert_cue(literal_cue(1.0, &[((1, 1), 255.0)]));
        list.upsert_cue(literal_cue(2.0, &[((1, 2), 128.0)]));

        let presets = PresetStore::new();
        let state_at_2 = resolve_cue_state(&list, 1, &presets);
        assert_eq!(state_at_2.get(&(1, 1)), None); // did NOT track forward
        assert_eq!(state_at_2.get(&(1, 2)), Some(&128.0));
    }

    #[test]
    fn editing_an_earlier_cue_immediately_changes_later_tracking() {
        let mut list = Cuelist::new(1, "List".to_string(), CuelistKind::Standard, true);
        list.upsert_cue(literal_cue(1.0, &[((1, 1), 100.0)]));
        list.upsert_cue(literal_cue(2.0, &[((1, 2), 50.0)]));
        let presets = PresetStore::new();

        assert_eq!(resolve_cue_state(&list, 1, &presets).get(&(1, 1)), Some(&100.0));

        // UPDATE cue 1 (same number = replace).
        list.upsert_cue(literal_cue(1.0, &[((1, 1), 200.0)]));
        assert_eq!(resolve_cue_state(&list, 1, &presets).get(&(1, 1)), Some(&200.0));
    }

    #[test]
    fn a_cue_referencing_a_preset_reflects_updates_to_it() {
        let mut presets = PresetStore::new();
        let preset_id = presets.record(PresetFamily::Colour, "Wash".to_string(), [("red".to_string(), 100.0)].into(), None);

        let mut list = Cuelist::new(1, "List".to_string(), CuelistKind::Standard, true);
        let mut cue = Cue::new(1.0, "Cue 1".to_string());
        cue.values.insert((1, 1), CueValue::Preset { preset_id, attribute: "red".to_string() });
        list.upsert_cue(cue);

        assert_eq!(resolve_cue_state(&list, 0, &presets).get(&(1, 1)), Some(&100.0));

        presets.update(preset_id, [("red".to_string(), 200.0)].into(), None).unwrap();
        assert_eq!(resolve_cue_state(&list, 0, &presets).get(&(1, 1)), Some(&200.0));
    }

    #[test]
    fn upsert_keeps_cues_sorted_by_number_regardless_of_insertion_order() {
        let mut list = Cuelist::new(1, "List".to_string(), CuelistKind::Standard, true);
        list.upsert_cue(Cue::new(3.0, "C".to_string()));
        list.upsert_cue(Cue::new(1.0, "A".to_string()));
        list.upsert_cue(Cue::new(2.0, "B".to_string()));
        assert_eq!(list.cues.iter().map(|c| c.number).collect::<Vec<_>>(), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn decimal_cue_numbers_sort_correctly() {
        let mut list = Cuelist::new(1, "List".to_string(), CuelistKind::Standard, true);
        list.upsert_cue(Cue::new(1.5, "1.5".to_string()));
        list.upsert_cue(Cue::new(1.0, "1".to_string()));
        list.upsert_cue(Cue::new(1.2, "1.2".to_string()));
        assert_eq!(list.cues.iter().map(|c| c.number).collect::<Vec<_>>(), vec![1.0, 1.2, 1.5]);
    }

    #[test]
    fn remove_and_index_of() {
        let mut list = Cuelist::new(1, "List".to_string(), CuelistKind::Standard, true);
        list.upsert_cue(Cue::new(1.0, "A".to_string()));
        list.upsert_cue(Cue::new(2.0, "B".to_string()));
        assert_eq!(list.index_of(2.0), Some(1));
        list.remove_cue(1.0);
        assert_eq!(list.cues.len(), 1);
        assert_eq!(list.index_of(1.0), None);
    }

    #[test]
    fn resolving_past_the_end_returns_empty_state() {
        let list = Cuelist::new(1, "List".to_string(), CuelistKind::Standard, true);
        let presets = PresetStore::new();
        assert!(resolve_cue_state(&list, 0, &presets).is_empty());
    }
}
