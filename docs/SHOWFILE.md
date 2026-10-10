# The `.lumen` show file

A show file is a single JSON document (extension `.lumen`) holding everything
needed to reopen a show: universes, patch, workspaces, settings, groups,
presets, cuelists (with their cues' effects), the global tempo, pixel maps,
and (for now) the legacy Scene/Chase/FX asset library. Timelines and
MIDI/OSC mappings will be added to this document by the phases that
implement them (see `DECISIONS.md` for why they
aren't in the schema yet).

## Where it lives

- **Source of truth for the schema:** `src-tauri/src/showfile/schema.rs`
  (Rust structs, `serde` for JSON, `ts-rs` for TypeScript).
- **Generated TypeScript types:** `src/lib/showfile/generated/*.ts`, imported
  through the barrel `src/lib/showfile/generated/index.ts`. Regenerate after
  changing `schema.rs` by running `cargo test` in `src-tauri/` — each
  `#[ts(export)]`'d struct has an auto-generated test that (re)writes its file.
  **Never hand-edit files under `generated/`.**
- **Migration:** `src-tauri/src/showfile/migrate.rs`.
- **Disk I/O and dialogs:** `src-tauri/src/showfile/commands.rs` (Tauri
  commands `showfile_new`, `showfile_open`, `showfile_save`,
  `showfile_autosave`, `showfile_recent`, `showfile_restore_session`).
- **Frontend:** `src/lib/showfile/api.ts` (thin `invoke()` wrappers),
  `src/store/useShowStore.ts` (New/Open/Save/Save As, autosave timer, dirty
  tracking).

## Why file dialogs aren't a Tauri plugin

Open/Save use the `rfd` crate directly from Rust commands instead of the
`@tauri-apps/plugin-dialog`/`plugin-fs` JS plugins. The dialog only ever runs
inside a Rust command, so no extra entries were needed in
`src-tauri/capabilities/default.json`.

## Where "show state" actually lives

The patch and the legacy asset library are **not** duplicated inside
`useShowStore`. `usePatchStore` and `useLibraryStore` remain the single source
of truth for that data (as they were before show files existed); `useShowStore`
only holds show *metadata* (id, name, timestamps, universes, workspaces,
settings) and assembles/applies the other two stores' data when saving or
opening. This avoids a second copy of the patch that could drift out of sync.
Once Phase 3 moves the programmer and playback state into the Rust engine,
that state becomes the new source of truth for its part of the show file the
same way.

## Versioning and migration

`schemaVersion` (currently `6`) is bumped whenever `ShowFile`'s on-disk shape
changes. `migrate::migrate()` takes the raw parsed JSON, reads `schemaVersion`
(treating a missing field as version `0`), and applies upgrade steps one
version at a time until it reaches the current version, then deserializes into
`ShowFile`. To add a new version:

1. Bump `CURRENT_SCHEMA_VERSION` in `schema.rs`.
2. Add an `upgrade_N_to_N+1(value: serde_json::Value) -> serde_json::Value`
   function in `migrate.rs` and call it from the match arm for version `N`.
3. Add a unit test with a fixture of the old shape asserting the new shape.

A `schemaVersion` newer than what the running app supports is rejected with an
error asking the user to update the app, rather than silently mis-reading it.

## Legacy assets

Before show files existed, Scene/Chase/FX assets lived in `localStorage` under
`lumendmx.library.v1` (`useLibraryStore`). Rather than lose that data, the show
file schema carries them as `legacyAssets: LegacyAsset[]` — a straight mirror
of the old `LightAsset`/`DmxStep` shape (`src/lib/showfile/legacyAssets.ts` has
the two-way mapping, with unit tests). They stay under that name until Phase
5's preset families and Phase 6's cuelists give this data a real home; nothing
in today's app deletes it.

## Groups, presets and cuelists (v4+)

These are owned by the Rust engine at runtime, not by a frontend store, so
the file's `groups`/`presets`/`cuelists` fields travel through the engine:

- **Save/autosave:** `useShowStore` calls `engine_export_show_data` and puts
  the result into the `ShowFile` it sends to `showfile_save`/`showfile_autosave`.
  Cuelists use the same flattened `CuelistDto` shape as the UI (cue values
  keyed by `(universe, channel)` can't be JSON object keys).
- **Open/New/restore:** `useShowStore` passes them to `engine_load_show_data`,
  which replaces the engine's stores (ids are kept, so cue→preset links stay
  valid, and new ids continue after the highest loaded one) and drops running
  cue playbacks, which point at the outgoing cuelists.
- **Dirty tracking:** every engine edit to this data bumps a revision
  (`engine_show_data_revision`); the frontend polls it and marks the show
  unsaved when it differs from the revision at the last save/load.

Playbacks are runtime state and aren't saved.

## Tempo, effects and pixel maps (v5+)

- `bpm`: the global tempo. Engine-owned like the above; it travels through
  `engine_export_show_data`/`engine_load_show_data` too.
- `pixelMaps`: each map's grid size, cell → fixture/cell mapping, generator
  and its settings. Imported image/GIF frames are stored already scaled to
  the map, as base64 RGB (`media.frames[].rgb`).
- Effects aren't a top-level list: a cue's effects are in `cues[].effects`,
  an Effect preset's definition in `presets[].effect`. Both are optional
  fields, so v4 files load without them. Effects running in the programmer
  aren't saved, like the rest of the programmer.
- Running pixel maps and programmer effects are runtime state and aren't
  saved.

## Timecode cues (v6+)

- `cuelists[].cues[].timecodeMs`: when a Timecode cuelist fires the cue, in
  milliseconds on the timecode clock. Optional (`null`/absent = never fires
  from timecode); v5 files load without it.
- The timing source settings (tempo source, audio/MIDI devices, timecode
  source) and audio-reactive triggers are runtime/machine state and aren't
  saved: a show moves between machines, device ids and runtime playback ids
  don't.

## Restoring the last session

On launch, `showfile_restore_session` picks whichever was touched last: the
newest autosave (unsaved work — reattached to its show file's path if that
show was saved before, and marked dirty) or the most recently opened/saved
show file. With neither, the app starts a new show.

## Autosave

Every 60 seconds, if the show has unsaved changes, the frontend assembles a
full `ShowFile` snapshot and calls `showfile_autosave`, which writes it to
`<app data dir>/autosave/<show id>/autosave-<timestamp>.lumen` and prunes
anything past the 10 most recent. This never touches the user's own save file,
so a bad autosave can't corrupt work that was already saved on purpose.
