//! Audio-reactive triggers (BUILD_PLAN Phase 8): a spectrum band's level,
//! scaled by a sensitivity and cut by a gate, flashes a playback or scales
//! an effect's size. The audio thread writes `levels`; the engine tick calls
//! `compute` once per tick and applies the result.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum Band {
    Low,
    Mid,
    High,
    /// Absolute input loudness, not normalized to recent peaks, so the gate
    /// is a real "this loud" threshold (a clap or shout, not the room tone).
    Loudness,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum TriggerTarget {
    /// Bumps the playback's intensity to the band level (a flash that
    /// follows the music), like holding its Flash button.
    #[serde(rename_all = "camelCase")]
    Flash { playback_id: u32 },
    /// Scales a running effect between nothing and its full size.
    #[serde(rename_all = "camelCase")]
    EffectSize { effect_id: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct AudioTrigger {
    /// 0 when creating; assigned by the engine.
    pub id: u32,
    pub band: Band,
    /// 0.0-1.0; 0.5 is unity gain, 1.0 is twice as sensitive.
    pub sensitivity: f32,
    /// 0.0-1.0: levels below this count as silence.
    pub gate: f32,
    /// How long a peak takes to fade away.
    pub release_ms: f32,
    pub target: TriggerTarget,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct BandLevels {
    pub low: f32,
    pub mid: f32,
    pub high: f32,
    /// Overall loudness, 0.0-1.0.
    pub rms: f32,
    /// Absolute loudness: the input's dBFS mapped -60 dB → 0.0, 0 dB → 1.0.
    pub loudness: f32,
}

impl BandLevels {
    pub fn get(&self, band: Band) -> f32 {
        match band {
            Band::Low => self.low,
            Band::Mid => self.mid,
            Band::High => self.high,
            Band::Loudness => self.loudness,
        }
    }
}

/// What the triggers want this tick.
#[derive(Debug, Default)]
pub struct AudioMods {
    pub flash: HashMap<u32, f32>,
    pub effect_size: HashMap<u32, f32>,
}

#[derive(Debug, Default)]
pub struct AudioReactive {
    pub levels: BandLevels,
    triggers: Vec<AudioTrigger>,
    /// Per-trigger smoothed output (fast attack, `release_ms` decay).
    smoothed: HashMap<u32, f32>,
    next_id: u32,
}

impl AudioReactive {
    pub fn new() -> Self {
        Self { next_id: 1, ..Default::default() }
    }

    pub fn triggers(&self) -> &[AudioTrigger] {
        &self.triggers
    }

    /// Adds (id 0) or replaces a trigger; returns its id.
    pub fn upsert(&mut self, mut trigger: AudioTrigger) -> u32 {
        trigger.sensitivity = trigger.sensitivity.clamp(0.0, 1.0);
        trigger.gate = trigger.gate.clamp(0.0, 1.0);
        trigger.release_ms = trigger.release_ms.max(1.0);
        if trigger.id == 0 {
            trigger.id = self.next_id.max(1);
        }
        let id = trigger.id;
        self.next_id = self.next_id.max(id + 1);
        match self.triggers.iter_mut().find(|t| t.id == id) {
            Some(slot) => *slot = trigger,
            None => self.triggers.push(trigger),
        }
        id
    }

    pub fn delete(&mut self, id: u32) {
        self.triggers.retain(|t| t.id != id);
        self.smoothed.remove(&id);
    }

    pub fn compute(&mut self, dt_ms: f64) -> AudioMods {
        let mut mods = AudioMods::default();
        for t in &self.triggers {
            let prev = self.smoothed.get(&t.id).copied().unwrap_or(0.0);
            let level = if t.enabled {
                let x = (self.levels.get(t.band) * t.sensitivity * 2.0).clamp(0.0, 1.0);
                if x < t.gate {
                    0.0
                } else {
                    x
                }
            } else {
                0.0
            };
            let out = level.max(prev - (dt_ms as f32 / t.release_ms)).max(0.0);
            self.smoothed.insert(t.id, out);
            let entry = match t.target {
                TriggerTarget::Flash { playback_id } => mods.flash.entry(playback_id),
                TriggerTarget::EffectSize { effect_id } => mods.effect_size.entry(effect_id),
            }
            .or_insert(0.0);
            *entry = entry.max(out);
        }
        mods
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flash_trigger(gate: f32) -> AudioTrigger {
        AudioTrigger {
            id: 0,
            band: Band::Low,
            sensitivity: 0.5,
            gate,
            release_ms: 100.0,
            target: TriggerTarget::Flash { playback_id: 7 },
            enabled: true,
        }
    }

    #[test]
    fn gate_cuts_quiet_levels_and_release_fades_peaks() {
        let mut ar = AudioReactive::new();
        ar.upsert(flash_trigger(0.3));

        ar.levels.low = 0.2; // under the gate
        assert_eq!(ar.compute(10.0).flash[&7], 0.0);

        ar.levels.low = 0.8;
        assert_eq!(ar.compute(10.0).flash[&7], 0.8);

        ar.levels.low = 0.0; // 50ms into a 100ms release: half gone
        let faded = ar.compute(50.0).flash[&7];
        assert!((faded - 0.3).abs() < 1e-5, "{faded}");
        assert_eq!(ar.compute(500.0).flash[&7], 0.0);
    }

    #[test]
    fn sensitivity_scales_the_level_and_disabled_triggers_stay_silent() {
        let mut ar = AudioReactive::new();
        let mut t = flash_trigger(0.0);
        t.target = TriggerTarget::EffectSize { effect_id: 3 };
        t.sensitivity = 1.0;
        let id = ar.upsert(t.clone());
        ar.levels.low = 0.2;
        assert!((ar.compute(1.0).effect_size[&3] - 0.4).abs() < 1e-5);

        t.id = id;
        t.enabled = false;
        ar.upsert(t);
        assert_eq!(ar.compute(1000.0).effect_size[&3], 0.0);
    }
}
