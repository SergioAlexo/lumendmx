//! The `.lumen` show-file schema.
//!
//! Only fields backed by real functionality today are modelled here
//! (universes, patch, workspaces, settings, legacy assets, since v4 the
//! engine's groups/presets/cuelists, and since v5 its tempo and pixel maps;
//! effects live inside cues and Effect presets). Timelines and MIDI/OSC
//! mappings are added to this schema — each with its own `schema_version` bump and migration step —
//! by the phase that actually implements them (see `DECISIONS.md`). Designing
//! their shape now, before any of that logic exists, would just mean
//! redesigning it later.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::engine::groups::Group;
use crate::engine::manager::CuelistDto;
use crate::engine::pixelmap::PixelMap;
use crate::engine::presets::Preset;
use crate::output::config::DriverConfig;

/// Bump this whenever `ShowFile`'s on-disk shape changes, and add an upgrade
/// step in `migrate.rs` from the previous version.
pub const CURRENT_SCHEMA_VERSION: u32 = 5;

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
pub struct Workspace {
    pub id: String,
    pub name: String,
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

/// A single step in a pre-showfile Scene/Chase/FX asset (mirrors the frontend's
/// `LightAsset`/`DmxStep` from `src/lib/types.ts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct LegacyStep {
    /// Channel (as a string, 1-512) -> value (0-255). Unlisted channels are
    /// left untouched. Stored as a string-keyed map because that's how
    /// `Record<number, number>` round-trips through JSON.
    pub channels: std::collections::BTreeMap<String, u8>,
    pub fade_time_ms: u32,
    pub hold_time_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct LegacyAsset {
    pub id: String,
    pub name: String,
    /// "scene" | "chase" | "fx"
    pub kind: String,
    pub steps: Vec<LegacyStep>,
    #[serde(rename = "loop")]
    pub loop_: bool,
    /// "ai" | "manual"
    pub source: String,
    /// Milliseconds since epoch (matches JS `Date.now()`); `f64` rather than
    /// an integer type so ts-rs maps it to a plain `number`, not `bigint`.
    pub created_at: f64,
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
    pub workspaces: Vec<Workspace>,
    pub settings: ShowSettings,
    /// Scene/Chase/FX assets carried over from the pre-showfile `localStorage`
    /// library. Superseded once Phase 5's preset families and Phase 6's
    /// cuelists exist; kept as plain data until then so nothing is lost.
    pub legacy_assets: Vec<LegacyAsset>,
    /// Engine-owned show data (v4+). Owned and edited by the Rust engine at
    /// runtime; the frontend fetches it into the file on save
    /// (`engine_export_show_data`) and pushes it back on load
    /// (`engine_load_show_data`).
    pub groups: Vec<Group>,
    pub presets: Vec<Preset>,
    pub cuelists: Vec<CuelistDto>,
    /// The global tempo (v5+).
    pub bpm: f32,
    /// Pixel maps (v5+), also engine-owned.
    pub pixel_maps: Vec<PixelMap>,
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
            workspaces: default_workspaces(),
            settings: ShowSettings::default(),
            legacy_assets: Vec::new(),
            groups: Vec::new(),
            presets: Vec::new(),
            cuelists: Vec::new(),
            bpm: crate::engine::tempo::DEFAULT_BPM,
            pixel_maps: Vec::new(),
        }
    }
}

/// The default workspace tabs from the target layout (BUILD_PLAN.md section 2).
/// Each is just a named tab for now; the dockable window grid inside each one
/// is built progressively as the window types it can hold are implemented.
fn default_workspaces() -> Vec<Workspace> {
    ["Programming", "Playback", "Live AI", "Patch/Setup"]
        .iter()
        .map(|name| Workspace {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
        })
        .collect()
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
