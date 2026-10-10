//! Presets: a named set of attribute values, recordable per family
//! (BUILD_PLAN Phase 5). Applying a preset looks up its *current* values by
//! id every time — nothing ever copies a preset's values into whatever uses
//! it, so updating a preset changes every future apply, including (once
//! Phase 6 exists) a recorded cue that references it. See `apply_updates_...`
//! below for the property that actually matters here.
//!
//! Attribute names are plain strings (matching the frontend's `ChannelType`
//! union values, e.g. "red"/"pan"/"gobo") rather than a mirrored Rust enum —
//! the fixture library that defines what attributes exist lives in
//! TypeScript (see docs/ENGINE.md's channel-based design note); Rust just
//! stores and looks up by whatever attribute names the frontend sends.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::effects::EffectDef;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum PresetFamily {
    Intensity,
    Colour,
    Position,
    Beam,
    BeamFx,
    Framing,
    Effect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct Preset {
    pub id: u32,
    pub family: PresetFamily,
    pub name: String,
    /// Attribute name -> value (0.0-255.0, matching the engine's other
    /// channel value ranges).
    pub values: HashMap<String, f32>,
    /// For a Colour preset's grid swatch; not meaningful for other families.
    pub color: Option<String>,
    /// An Effect preset's effect (Phase 7). `values` is empty for those.
    #[serde(default)]
    pub effect: Option<EffectDef>,
}

#[derive(Default)]
pub struct PresetStore {
    presets: HashMap<u32, Preset>,
    next_id: u32,
}

impl PresetStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, family: PresetFamily, name: String, values: HashMap<String, f32>, color: Option<String>) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        self.presets.insert(id, Preset { id, family, name, values, color, effect: None });
        id
    }

    /// Records an Effect preset holding `effect`.
    pub fn record_effect(&mut self, name: String, effect: EffectDef) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        let preset = Preset { id, family: PresetFamily::Effect, name, values: HashMap::new(), color: None, effect: Some(effect) };
        self.presets.insert(id, preset);
        id
    }

    /// Replaces an Effect preset's effect in place (same id, so every cue
    /// playing it picks up the change on its next tick).
    pub fn update_effect(&mut self, id: u32, effect: EffectDef) -> Result<(), String> {
        let preset = self.presets.get_mut(&id).ok_or_else(|| format!("No preset {id}"))?;
        if preset.family != PresetFamily::Effect {
            return Err(format!("Preset {id} isn't an Effect preset"));
        }
        preset.effect = Some(effect);
        Ok(())
    }

    /// Re-records an existing preset's values in place (same id, so every
    /// consumer that looks it up by id immediately sees the update).
    pub fn update(&mut self, id: u32, values: HashMap<String, f32>, color: Option<String>) -> Result<(), String> {
        let preset = self.presets.get_mut(&id).ok_or_else(|| format!("No preset {id}"))?;
        preset.values = values;
        if color.is_some() {
            preset.color = color;
        }
        Ok(())
    }

    pub fn rename(&mut self, id: u32, name: String) -> Result<(), String> {
        let preset = self.presets.get_mut(&id).ok_or_else(|| format!("No preset {id}"))?;
        preset.name = name;
        Ok(())
    }

    pub fn delete(&mut self, id: u32) {
        self.presets.remove(&id);
    }

    pub fn get(&self, id: u32) -> Option<&Preset> {
        self.presets.get(&id)
    }

    /// Replaces every preset (loading a show). New ids continue after the
    /// highest loaded one, so cue links (which reference presets by id)
    /// keep pointing at the right preset.
    pub fn replace_all(&mut self, presets: Vec<Preset>) {
        self.next_id = presets.iter().map(|p| p.id).max().unwrap_or(0);
        self.presets = presets.into_iter().map(|p| (p.id, p)).collect();
    }

    pub fn list(&self, family: Option<PresetFamily>) -> Vec<Preset> {
        let mut presets: Vec<Preset> = self
            .presets
            .values()
            .filter(|p| family.is_none_or(|f| p.family == f))
            .cloned()
            .collect();
        presets.sort_by_key(|p| p.id);
        presets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, f32)]) -> HashMap<String, f32> {
        pairs.iter().map(|&(k, v)| (k.to_string(), v)).collect()
    }

    #[test]
    fn records_and_looks_up_by_id() {
        let mut store = PresetStore::new();
        let id = store.record(
            PresetFamily::Colour,
            "Deep Blue".to_string(),
            values(&[("red", 0.0), ("blue", 255.0)]),
            Some("#0000ff".to_string()),
        );
        let preset = store.get(id).unwrap();
        assert_eq!(preset.name, "Deep Blue");
        assert_eq!(preset.values.get("blue"), Some(&255.0));
    }

    /// The property BUILD_PLAN Phase 5's "done when" actually cares about:
    /// nothing that "applies" a preset should see stale values after the
    /// preset is updated, because nothing ever copies the values out — every
    /// apply is a fresh `get(id)` lookup.
    #[test]
    fn updating_a_preset_changes_what_every_future_lookup_sees() {
        let mut store = PresetStore::new();
        let id = store.record(PresetFamily::Colour, "Wash".to_string(), values(&[("red", 100.0)]), None);

        let first_lookup = store.get(id).unwrap().values.get("red").copied();
        assert_eq!(first_lookup, Some(100.0));

        store.update(id, values(&[("red", 200.0)]), None).unwrap();

        let second_lookup = store.get(id).unwrap().values.get("red").copied();
        assert_eq!(second_lookup, Some(200.0));
    }

    #[test]
    fn update_preserves_color_when_none_given() {
        let mut store = PresetStore::new();
        let id = store.record(PresetFamily::Colour, "C".to_string(), values(&[]), Some("#ff0000".to_string()));
        store.update(id, values(&[("red", 1.0)]), None).unwrap();
        assert_eq!(store.get(id).unwrap().color.as_deref(), Some("#ff0000"));
    }

    #[test]
    fn list_filters_by_family() {
        let mut store = PresetStore::new();
        store.record(PresetFamily::Colour, "C".to_string(), values(&[]), None);
        store.record(PresetFamily::Position, "P".to_string(), values(&[]), None);

        assert_eq!(store.list(Some(PresetFamily::Colour)).len(), 1);
        assert_eq!(store.list(Some(PresetFamily::Position)).len(), 1);
        assert_eq!(store.list(None).len(), 2);
    }

    #[test]
    fn rename_and_delete_unknown_preset_errors_or_is_a_noop() {
        let mut store = PresetStore::new();
        assert!(store.rename(999, "X".to_string()).is_err());
        store.delete(999); // no panic
    }
}
