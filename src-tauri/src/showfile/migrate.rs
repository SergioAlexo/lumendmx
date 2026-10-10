//! Upgrades an on-disk `.lumen` JSON document (any past `schemaVersion`) to
//! the current `ShowFile` shape.
//!
//! To add a new version: bump `CURRENT_SCHEMA_VERSION` in `schema.rs`, add an
//! `upgrade_N_to_N+1` step below, and call it from the match arm for version
//! `N`. Each step takes and returns a `serde_json::Value` so it can add/rename/
//! drop fields without needing the old Rust struct to still exist.

use serde_json::Value;

use super::schema::{ShowFile, CURRENT_SCHEMA_VERSION};

pub fn migrate(raw: Value) -> Result<ShowFile, String> {
    let version = raw
        .as_object()
        .ok_or_else(|| "Show file is not a JSON object".to_string())?
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;

    if version > CURRENT_SCHEMA_VERSION {
        return Err(format!(
            "This show file is from a newer version of LumenDMX (schema {}, this app supports up to {}). Update the app first.",
            version, CURRENT_SCHEMA_VERSION
        ));
    }

    let mut current_version = version;
    let mut value = raw;

    if current_version == 0 {
        value = upgrade_0_to_1(value);
        current_version = 1;
    }
    if current_version == 1 {
        value = upgrade_1_to_2(value);
        current_version = 2;
    }
    if current_version == 2 {
        value = upgrade_2_to_3(value);
        current_version = 3;
    }
    if current_version == 3 {
        value = upgrade_3_to_4(value);
        current_version = 4;
    }

    debug_assert_eq!(current_version, CURRENT_SCHEMA_VERSION);

    serde_json::from_value(value).map_err(|e| format!("Failed to parse show file: {e}"))
}

/// Version 0 is "no `schemaVersion` field at all" — either a hand-written
/// test fixture or, in the future, genuinely pre-showfile data. Fills in every
/// field `ShowFile` requires with sensible defaults, keeping whatever the
/// input already had.
fn upgrade_0_to_1(value: Value) -> Value {
    let mut obj = match value {
        Value::Object(o) => o,
        _ => serde_json::Map::new(),
    };

    let default = ShowFile::new_default("Untitled Show");
    let default_value = serde_json::to_value(&default).expect("ShowFile always serializes");
    let default_obj = default_value.as_object().expect("object").clone();

    for (key, default_val) in default_obj {
        obj.entry(key).or_insert(default_val);
    }
    obj.insert("schemaVersion".to_string(), Value::from(1));

    Value::Object(obj)
}

/// v1 -> v2: each `UniverseConfig` traded its FTDI-only `outputPort:
/// Option<String>` for a driver-tagged `driver: DriverConfig` (Phase 2, adding
/// Enttec Pro/Art-Net/sACN). An FTDI port becomes `{kind:"ftdi", port,
/// rateHz:30}`; no port becomes `{kind:"null"}`.
fn upgrade_1_to_2(value: Value) -> Value {
    let mut obj = match value {
        Value::Object(o) => o,
        _ => serde_json::Map::new(),
    };

    if let Some(Value::Array(universes)) = obj.get_mut("universes") {
        for universe in universes.iter_mut() {
            if let Value::Object(u) = universe {
                if u.contains_key("driver") {
                    continue;
                }
                let driver = match u.remove("outputPort") {
                    Some(Value::String(port)) => serde_json::json!({
                        "kind": "ftdi",
                        "port": port,
                        "rateHz": 30,
                    }),
                    _ => serde_json::json!({ "kind": "null" }),
                };
                u.insert("driver".to_string(), driver);
            }
        }
    }

    obj.insert("schemaVersion".to_string(), Value::from(2));
    Value::Object(obj)
}

/// v2 -> v3: `PatchedFixture` gains `universe` (every fixture was implicitly
/// universe 1 before Setup could manage more than one), `fixtureNumber` (a
/// stable command-line-visible number — defaults to 1-based patch order,
/// matching Phase 3's in-memory placeholder exactly so nothing renumbers on
/// upgrade), and `invertPan`/`invertTilt`/`swapPanTilt` (default off).
fn upgrade_2_to_3(value: Value) -> Value {
    let mut obj = match value {
        Value::Object(o) => o,
        _ => serde_json::Map::new(),
    };

    if let Some(Value::Array(patch)) = obj.get_mut("patch") {
        for (index, fixture) in patch.iter_mut().enumerate() {
            if let Value::Object(f) = fixture {
                f.entry("universe").or_insert(Value::from(1));
                f.entry("fixtureNumber").or_insert(Value::from(index as u64 + 1));
                f.entry("invertPan").or_insert(Value::from(false));
                f.entry("invertTilt").or_insert(Value::from(false));
                f.entry("swapPanTilt").or_insert(Value::from(false));
            }
        }
    }

    obj.insert("schemaVersion".to_string(), Value::from(3));
    Value::Object(obj)
}

/// v3 -> v4: groups, presets and cuelists (Phases 5/6) are saved in the
/// show file. Before v4 they only lived in the engine's memory, so an older
/// file simply has none.
fn upgrade_3_to_4(value: Value) -> Value {
    let mut obj = match value {
        Value::Object(o) => o,
        _ => serde_json::Map::new(),
    };
    for key in ["groups", "presets", "cuelists"] {
        obj.entry(key).or_insert(Value::Array(Vec::new()));
    }
    obj.insert("schemaVersion".to_string(), Value::from(4));
    Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_version_round_trips_exactly() {
        let show = ShowFile::new_default("My Show");
        let json = serde_json::to_value(&show).unwrap();
        let migrated = migrate(json).unwrap();
        assert_eq!(migrated, show);
    }

    #[test]
    fn missing_schema_version_fills_in_defaults() {
        let raw = serde_json::json!({ "name": "Old Show" });
        let migrated = migrate(raw).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(migrated.name, "Old Show");
        assert_eq!(migrated.universes.len(), 1);
        assert_eq!(migrated.workspaces.len(), 4);
    }

    #[test]
    fn preserves_existing_patch_and_settings_while_filling_gaps() {
        let raw = serde_json::json!({
            "name": "Partial",
            "patch": [
                { "id": "f1", "fixtureId": "generic-6color-par", "modeIndex": 0, "address": 1, "name": "Par 1" }
            ],
        });
        let migrated = migrate(raw).unwrap();
        assert_eq!(migrated.patch.len(), 1);
        assert_eq!(migrated.patch[0].address, 1);
        assert_eq!(migrated.settings.output_rate_hz, 30);
    }

    /// A realistic v1 file (as Phase 1's `showfile_save` would have written
    /// one): every `ShowFile` field present, `universes[].outputPort` in the
    /// old shape.
    fn v1_fixture(output_port: Value) -> Value {
        serde_json::json!({
            "schemaVersion": 1,
            "id": "show-1",
            "name": "Old Show",
            "createdAt": "2026-01-01T00:00:00Z",
            "modifiedAt": "2026-01-01T00:00:00Z",
            "universes": [
                { "id": 1, "name": "Universe 1", "outputPort": output_port }
            ],
            "patch": [],
            "workspaces": [],
            "settings": { "outputRateHz": 30, "defaultFadeMs": 0 },
            "legacyAssets": [],
        })
    }

    #[test]
    fn v1_ftdi_output_port_becomes_a_driver_config() {
        let migrated = migrate(v1_fixture(Value::String("COM9".to_string()))).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        match &migrated.universes[0].driver {
            crate::output::config::DriverConfig::Ftdi { port, rate_hz } => {
                assert_eq!(port.as_deref(), Some("COM9"));
                assert_eq!(*rate_hz, 30);
            }
            other => panic!("expected Ftdi driver, got {other:?}"),
        }
    }

    #[test]
    fn v1_unset_output_port_becomes_null_driver() {
        let migrated = migrate(v1_fixture(Value::Null)).unwrap();
        assert_eq!(migrated.universes[0].driver, crate::output::config::DriverConfig::Null);
    }

    /// A realistic v2 file: every `ShowFile` field present, `patch[]` fixtures
    /// in the pre-Phase-4 shape (no universe/fixtureNumber/invert*/swap*).
    fn v2_fixture(patch: Value) -> Value {
        serde_json::json!({
            "schemaVersion": 2,
            "id": "show-1",
            "name": "Old Show",
            "createdAt": "2026-01-01T00:00:00Z",
            "modifiedAt": "2026-01-01T00:00:00Z",
            "universes": [
                { "id": 1, "name": "Universe 1", "driver": { "kind": "null" } }
            ],
            "patch": patch,
            "workspaces": [],
            "settings": { "outputRateHz": 30, "defaultFadeMs": 0 },
            "legacyAssets": [],
        })
    }

    #[test]
    fn v2_patch_gains_universe_and_fixture_number() {
        let patch = serde_json::json!([
            { "id": "f1", "fixtureId": "par", "modeIndex": 0, "address": 1, "name": "Par 1" },
            { "id": "f2", "fixtureId": "par", "modeIndex": 0, "address": 11, "name": "Par 2" },
        ]);
        let migrated = migrate(v2_fixture(patch)).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(migrated.patch[0].universe, 1);
        assert_eq!(migrated.patch[0].fixture_number, 1);
        assert!(!migrated.patch[0].invert_pan);
        assert!(!migrated.patch[0].invert_tilt);
        assert!(!migrated.patch[0].swap_pan_tilt);
        assert_eq!(migrated.patch[1].fixture_number, 2);
    }

    #[test]
    fn v3_gains_empty_groups_presets_and_cuelists() {
        let mut v3 = serde_json::to_value(ShowFile::new_default("Old Show")).unwrap();
        let obj = v3.as_object_mut().unwrap();
        for key in ["groups", "presets", "cuelists"] {
            obj.remove(key);
        }
        obj.insert("schemaVersion".to_string(), Value::from(3));

        let migrated = migrate(v3).unwrap();
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(migrated.groups.is_empty());
        assert!(migrated.presets.is_empty());
        assert!(migrated.cuelists.is_empty());
    }

    #[test]
    fn engine_data_survives_a_save_and_load() {
        use crate::engine::cues::CuelistKind;
        use crate::engine::manager::{EngineManager, PatchIndexEntry};
        use crate::engine::presets::PresetFamily;
        use std::collections::HashSet;

        let engine = EngineManager::new();
        engine.set_patch_index(vec![PatchIndexEntry {
            fixture_number: 1,
            universe: 1,
            intensity_channel: Some(1),
            attribute_channels: [("dimmer".to_string(), 1)].into_iter().collect(),
        }]);
        let fixtures: HashSet<u32> = [1].into();
        engine.record_group("Front".to_string(), vec![1]);
        engine.execute_command("1 @ 50");
        let preset_id = engine.record_preset(PresetFamily::Intensity, "Half".to_string(), &fixtures, None);
        let cuelist_id = engine.create_cuelist("Main".to_string(), CuelistKind::Standard, true);
        engine.record_cue(cuelist_id, 1.0, "Look".to_string(), &fixtures, 500.0, 500.0).unwrap();
        engine.set_cue_preset_reference(cuelist_id, 1.0, 1, "dimmer".to_string(), preset_id).unwrap();
        let saved = engine.export_show_data();

        // Through the file format and back, as Save then Open would.
        let mut show = ShowFile::new_default("Show");
        show.groups = saved.groups.clone();
        show.presets = saved.presets.clone();
        show.cuelists = saved.cuelists.clone();
        let reopened = migrate(serde_json::to_value(&show).unwrap()).unwrap();

        let fresh = EngineManager::new();
        fresh.load_show_data(crate::engine::manager::ShowEngineData {
            groups: reopened.groups,
            presets: reopened.presets,
            cuelists: reopened.cuelists,
        });
        assert_eq!(fresh.export_show_data(), saved);

        // New ids continue after the loaded ones instead of colliding.
        assert!(fresh.record_group("Back".to_string(), vec![1]) > saved.groups[0].id);
    }

    #[test]
    fn future_schema_version_is_rejected() {
        let raw = serde_json::json!({ "schemaVersion": CURRENT_SCHEMA_VERSION + 1 });
        let err = migrate(raw).unwrap_err();
        assert!(err.contains("newer version"));
    }

    #[test]
    fn non_object_input_is_rejected() {
        let err = migrate(serde_json::json!([1, 2, 3])).unwrap_err();
        assert!(err.contains("not a JSON object"));
    }
}
