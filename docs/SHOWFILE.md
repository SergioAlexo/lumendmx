# The `.lumen` show file

A show file is a single JSON document (extension `.lumen`) holding everything
needed to reopen a show: universes (each with its controller), the patch,
groups, the node graph, the playback-bar control values, the tempo and
settings.

## Where it lives

- **Schema:** `src-tauri/src/showfile/schema.rs` (Rust structs, `serde` for
  JSON, `ts-rs` for TypeScript). The graph's types are in
  `src-tauri/src/graph/model.rs`.
- **Generated TypeScript types:** `src/lib/showfile/generated/*.ts`, imported
  through the barrel `src/lib/showfile/generated/index.ts`. Regenerate by
  running `cargo test` in `src-tauri/`: each `#[ts(export)]` type has a test
  that (re)writes its file. **Never hand-edit files under `generated/`.**
- **Reading and version checks:** `src-tauri/src/showfile/migrate.rs`.
- **Disk:** `src-tauri/src/showfile/store.rs` (read/write, recent list,
  autosaves, restoring the last session).
- **The open show:** `src-tauri/src/show/service.rs` (`ShowService`) holds it
  in memory and does New/Open/Save. The frontend never assembles a show file:
  it calls `show_save`, `show_open`, `show_new` (`src/lib/show.ts`).

Open/Save dialogs use the `rfd` crate from the Rust commands, so no dialog
or filesystem plugin capability is needed.

## Contents (format 9)

- `universes[]`: `id`, `name`, `driver` (the controller: `null`, `ftdi`,
  `enttecPro`, `artNet`, `sacn` with their settings).
- `patch[]`: `fixtureId` and `modeIndex` (library type and mode), `universe`,
  `address`, `fixtureNumber` (unique; groups and the graph refer to it),
  `name`, `invertPan`/`invertTilt`/`swapPanTilt`.
- `groups[]`: `id`, `name`, `fixtureNumbers` in fan order.
- `graph`: `nodes[]` (`id`, `kind`, `position`, `params`) and `edges[]`
  (`fromNode`/`fromSocket` → `toNode`/`toSocket`). See docs/NODES.md for
  each kind's sockets and params.
- `controls`: each Fader/Button node's value as last left, by node id.
- `bpm`, `settings`. `settings.beatOffsetMs` (±500) shifts every
  beat-synced effect earlier (negative: later) to make up for output delay;
  it lives in the show because it belongs to the rig the show is patched for.

Machine settings (tempo source, audio/MIDI devices, timecode source, the MCP
server) aren't in the show: device ids differ between machines.

## Versioning

`schemaVersion` is `9`, the first node-graph format. Files from the
pre-graph development builds (1-8) are refused with a message, not migrated.
A newer version than the app supports is refused with a request to update.
To change the shape: bump `CURRENT_SCHEMA_VERSION`, add an
`upgrade_N_to_N+1(serde_json::Value) -> serde_json::Value` step in
`migrate.rs`, and test it with a file of the old shape. Fields that can be
missing (`graph`, `controls`) default when absent.

## Restoring the last session

On launch, `ShowService::new` reopens whichever was touched last: the
newest autosave (unsaved work, reattached to its show file's path if that
show was saved before, and marked unsaved) or the most recently opened or
saved show file. With neither, it starts a new show.

## Autosave

Every 60 seconds, if the show has unsaved changes, Rust writes a snapshot to
`<app data dir>/autosave/<show id>/autosave-<timestamp>.lumen` and keeps the
10 most recent. It never touches the user's own file.
