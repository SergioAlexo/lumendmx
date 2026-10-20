//! The `.lumen` show-file schema.
//!
//! Format 9 is the node-graph rewrite: universes, the patch, groups, the
//! graph and the tempo. Older development formats (1-8, built around cuelists and a
//! programmer) aren't read at all — see `migrate.rs`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::graph::model::Graph;
use crate::output::config::DriverConfig;

/// Bump this whenever `ShowFile`'s on-disk shape changes, and add an upgrade
/// step in `migrate.rs` from the previous version.
pub const CURRENT_SCHEMA_VERSION: u32 = 9;
/// The oldest format this app reads: the first node-graph format.
pub const MIN_SCHEMA_VERSION: u32 = 9;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct UniverseConfig {
    pub id: u32,
    pub name: String,
    /// The controller this universe is sent through.
    pub driver: DriverConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct PatchedFixture {
    pub id: String,
    pub fixture_id: String,
    pub mode_index: u32,
    /// DMX start address, 1-512.
    pub address: u16,
    pub name: String,
    /// Which universe (and so which controller) this fixture is patched into.
    pub universe: u32,
    /// A stable, user-assignable number, unique across the show. Groups and
    /// the graph refer to fixtures by it.
    pub fixture_number: u32,
    pub invert_pan: bool,
    pub invert_tilt: bool,
    pub swap_pan_tilt: bool,
}

/// A named, ordered set of fixtures. The order is the fan order: effects
/// that spread across a group run from its first fixture to its last.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct Group {
    pub id: u32,
    pub name: String,
    pub fixture_numbers: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct ShowSettings {
    pub output_rate_hz: u32,
    pub default_fade_ms: u32,
    /// Shifts every beat-synced effect this much earlier (negative: later),
    /// to make up for delay in the DMX interface, the lights or the sound.
    #[serde(default)]
    pub beat_offset_ms: f32,
}

impl Default for ShowSettings {
    fn default() -> Self {
        Self {
            output_rate_hz: 30,
            default_fade_ms: 0,
            beat_offset_ms: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct ShowFile {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    /// RFC 3339 timestamps.
    pub created_at: String,
    pub modified_at: String,
    pub universes: Vec<UniverseConfig>,
    pub patch: Vec<PatchedFixture>,
    pub settings: ShowSettings,
    pub groups: Vec<Group>,
    /// What the show plays.
    #[serde(default)]
    pub graph: Graph,
    /// Playback-bar Fader/Button values by node id, as last left.
    #[serde(default)]
    pub controls: BTreeMap<String, f32>,
    /// The global tempo.
    pub bpm: f32,
}

impl ShowFile {
    pub fn new_default(name: &str) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            created_at: now.clone(),
            modified_at: now,
            universes: vec![UniverseConfig {
                id: 1,
                name: "Universe 1".to_string(),
                driver: DriverConfig::default(),
            }],
            patch: Vec::new(),
            settings: ShowSettings::default(),
            groups: Vec::new(),
            graph: Graph::default(),
            controls: BTreeMap::new(),
            bpm: crate::engine::tempo::DEFAULT_BPM,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct RecentShowEntry {
    pub path: String,
    pub name: String,
    pub opened_at: String,
}
