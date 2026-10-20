//! Reads an on-disk `.lumen` JSON document into the current `ShowFile` shape.
//!
//! To add a new version: bump `CURRENT_SCHEMA_VERSION` in `schema.rs`, add an
//! `upgrade_N_to_N+1` step taking and returning a `serde_json::Value` (so it
//! can add/rename/drop fields without the old Rust struct still existing),
//! and call it from `migrate`. Formats older than `MIN_SCHEMA_VERSION` came
//! from the pre-graph development builds and are rejected rather than
//! converted.

use serde_json::Value;

use super::schema::{ShowFile, CURRENT_SCHEMA_VERSION, MIN_SCHEMA_VERSION};

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
    if version < MIN_SCHEMA_VERSION {
        return Err(format!(
            "This show file was made by an older development build of LumenDMX (schema {version}) and can't be opened by this version."
        ));
    }

    serde_json::from_value(raw).map_err(|e| format!("Invalid show file: {e}"))
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
    fn pre_graph_files_are_rejected_with_a_clear_message() {
        let raw = serde_json::json!({ "schemaVersion": 8, "name": "Old Show" });
        let err = migrate(raw).unwrap_err();
        assert!(err.contains("older development build"), "{err}");
    }

    #[test]
    fn newer_files_ask_for_an_update() {
        let raw = serde_json::json!({ "schemaVersion": CURRENT_SCHEMA_VERSION + 1 });
        assert!(migrate(raw).unwrap_err().contains("newer version"));
    }
}
