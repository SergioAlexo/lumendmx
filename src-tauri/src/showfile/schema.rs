//! The `.lumen` show-file schema.
//!
//! Format 9 is the node-graph rewrite: universes, the patch, groups and the
//! tempo. Older development formats (1-8, built around cuelists and a
//! programmer) aren't read at all — see `migrate.rs`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::engine::groups::Group;
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
    /// v2: replaced the single FTDI-only `outputPort: Option<String>` with a
    /// driver-tagged config supporting FTDI/Enttec Pro/Art-Net/sACN/none
    /// (see `migrate::upgrade_1_to_2`).
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
    /// v3: which universe this fixture is patched into. Every fixture was
    /// implicitly universe 1 before Setup could manage more than one (see
    /// `migrate::upgrade_2_to_3`).
    pub universe: u32,
    /// v3: a stable, user-assignable number for the command line's `1 THRU 8`
    /// etc. to reference — replaces Phase 3's "just patch order" placeholder.
    pub fixture_number: u32,
    pub invert_pan: bool,
    pub invert_tilt: bool,
    pub swap_pan_tilt: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct ShowSettings {
    pub output_rate_hz: u32,
    pub default_fade_ms: u32,
}

impl Default for ShowSettings {
    fn default() -> Self {
        Self {
            output_rate_hz: 30,
            default_fade_ms: 0,
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
    /// Engine-owned show data: owned and edited by the Rust engine at
    /// runtime; the frontend fetches it into the file on save
    /// (`engine_export_show_data`) and pushes it back on load
    /// (`engine_load_show_data`).
    pub groups: Vec<Group>,
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

/// What `showfile_restore_session` brings back on launch.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct RestoredSession {
    pub show: ShowFile,
    /// The show's file on disk, if it has one (a recovered autosave of a
    /// never-saved show has none).
    pub path: Option<String>,
    /// True when `show` came from an autosave holding changes newer than the
    /// last open/save, i.e. it has unsaved work.
    pub recovered: bool,
}
