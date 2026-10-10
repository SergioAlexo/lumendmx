# Decisions

One line (or a short paragraph) per call made without stopping to ask, per
`BUILD_PLAN.md`'s ground rules. Newest at the bottom.

## Phase 1 — Foundations

- **Show-file schema scope:** `ShowFile` (schema.rs) only models fields backed
  by real functionality today (universes, patch, workspaces, settings, legacy
  assets). Groups/presets/cuelists/effects/pixel-maps/timelines/MIDI-OSC-mappings
  are added to the schema, with their own migration step, by the phase that
  actually implements them (5, 5, 6, 7, 7, 8, 9). Designing their on-disk shape
  now, before any of the logic that needs it exists, would just mean guessing
  wrong and redesigning it later — the migration system exists precisely so
  this can happen incrementally.
- **Rust core crate layout is populated progressively, not stubbed up front:**
  `showfile/` exists now with real content. `output/` (refactoring `dmx.rs`
  into an `OutputDriver` trait) is Phase 2's task; `engine/` (state/merge/
  playback tick) is Phase 3's; `io/` (MIDI/OSC/timecode moved off the JS side)
  is Phase 9's. Creating empty module directories now would just be decoration.
- **File dialogs via `rfd`, not a Tauri plugin:** New/Open/Save/Save As run
  entirely in Rust commands using the `rfd` crate, so nothing needed adding to
  `src-tauri/capabilities/default.json`.
- **Autosave is driven by a frontend timer, not a Rust background task:** show
  state (patch, legacy assets) currently lives in the frontend Zustand stores,
  not in Rust — that only changes once Phase 3's engine owns programming
  state. So the 60s autosave loop lives in `useShowStore`, which assembles a
  snapshot and hands it to the `showfile_autosave` command to write to disk.
- **`schemaVersion` and all show-file JSON fields are camelCase** (matching
  BUILD_PLAN.md's own wording and the rest of the frontend's JSON), via
  `#[serde(rename_all = "camelCase")]` on every schema struct.
- **Vitest added now, not deferred to Phase 14:** `npm test` is one of every
  phase's exit gates starting now, and there was no test runner at all yet.
  Phase 14 will still add Playwright smoke tests and broaden coverage.
- **`ts-rs` pinned at `9.x`** (latest major is 12) — this is just whatever
  `cargo add` resolved against the `"9"` requirement in Cargo.toml; revisit if
  a later phase's type needs a newer ts-rs feature.
- **ts-rs `export_to` points at a directory, not a single file:** pointing
  every `#[ts(export)]` struct at the same file caused the per-type export
  tests (which run in parallel) to overwrite each other, so only the
  last-finishing type's definition survived. Each type now gets its own file
  under `src/lib/showfile/generated/`, with a hand-written `index.ts` barrel.
- **Left one `npm audit` finding unresolved:** `@vitest/mocker`'s path-traversal
  advisory only has a fix via the Vitest 5 major bump, which is out of scope
  for a docs/save-file phase. It's a dev-only test-mocking codepath with no
  untrusted input anywhere in this repo's test suite.
- **Undo/redo wired into `usePatchStore`, since that's the only "programming
  edit" that exists before Phase 3's real programmer.** `src/lib/undo.ts`'s
  `UndoStack` (command pattern, 100 levels) is written to be reused by the
  Phase 3 programmer for selection/attribute/cue edits, not replaced.
- **Default workspaces are name-only stubs** (Programming/Playback/Live
  AI/Patch-Setup tabs with no window layout yet) — the dockable window grid is
  built up as the window types it can hold exist, not invented speculatively now.

## Phase 2 — Multi-universe output engine

- **Enttec Pro Mk2's second universe is explicitly rejected, not guessed at.**
  There's no physical Pro widget in this project to verify against, and the
  Mk2's dual-universe port-select framing isn't part of the base documented
  protocol I could implement with confidence. `EnttecProDriver::write_frame`
  returns a clear error for `universe_index != 0` rather than silently
  emitting unverified bytes at real hardware. See HARDWARE_CHECKS.md.
- **DMX input receives and decodes but doesn't merge into output yet.**
  BUILD_PLAN Phase 2 asks for input "with a merge option," but Phase 3 owns
  building the real HTP/LTP merge engine. Wiring a second, ad-hoc merge into
  `runner::UniverseOutput` — the same code path the safety-critical FTDI
  driver's frame loop runs through — this late in an already-large phase was
  judged too risky for the payoff; the merge step is Phase 3's to build once,
  properly. See docs/OUTPUT.md.
- **sACN input listeners bind with `SO_REUSEADDR` via the `socket2` crate**
  (std's `UdpSocket` can't set this before bind), so more than one universe's
  multicast listener can share port 5568, which any real multi-universe sACN
  receiver needs.
- **Enum JSON field casing:** serde's `rename_all` on an enum only renames
  *variant* names, not the fields inside struct-like variants — each variant
  of `DriverConfig`/`SacnDestination` needed its own
  `#[serde(rename_all = "camelCase")]` to keep field names consistent with the
  rest of the schema. Caught by a real test failure ("missing field
  `rate_hz`"), not by inspection.
- **Show-file schema v2:** `UniverseConfig.outputPort: Option<String>` became
  `driver: DriverConfig` (tagged enum covering Null/FTDI/EnttecPro/ArtNet/
  sACN), with a migration step and tests for both the "had an FTDI port" and
  "had no port" cases.

## Phase 3 — Engine core

- **The engine is channel-based, not fixture/attribute-based.** Fixture ->
  DMX-address resolution stays in the frontend (which already owns the
  fixture library); only the resolved (channel, is-it-HTP) result and a
  fixture-number index cross into Rust. See docs/ENGINE.md. This kept the
  merge/tick/programmer genuinely simple and testable instead of duplicating
  fixture-library logic in Rust.
- **Fixture numbers for the command line are just 1-based patch order**, not
  a stored/assignable ID — real fixture ID numbering is explicitly a Phase 4
  Patch-screen task. Renumbering happens implicitly when the patch order
  changes; that's an acceptable rough edge until Phase 4 gives fixtures a
  real, stable number.
- **GROUP/RECORD CUE/UPDATE/DELETE CUE/COPY/MOVE parse but don't execute.**
  Groups (Phase 5) and cuelists (Phase 6) don't exist yet. The parser handles
  and unit-tests all these forms (satisfying "parser gets unit tests for
  every form"); the executor returns a clear "not implemented yet" error
  instead of guessing at semantics those phases haven't designed.
- **TIME sets a stored value but doesn't yet affect `@` commands** (which
  stay immediate/unfaded) — applying a fade time to a manual intensity set
  needs the same timing infrastructure Phase 6's cue recording will build;
  wiring a separate one now would be throwaway work.
- **A finished non-looping playback keeps its last values live** rather than
  reverting when its `RunningPlayback` stops ticking — matches `LightAsset`'s
  own "Scenes always apply steps[0] once" contract (a persistent look, not a
  blip). Only `stop_asset` (or re-triggering the same id) removes the layer.
  Found by writing a test for the opposite behavior first and realizing it
  was wrong, not by inspection.
- **A real bug in the legacy-playback fade math**, also found by its own
  test: a Hold-to-Fade phase transition wrote the new step's value only on
  the *next* `advance()` call, leaving a stale value on the wire for one
  tick. Fixed by looping through same-tick phase transitions with leftover-
  time carryover instead of handling one transition per call.
- **DMX input is not merged into the engine's output yet** (a decision
  carried over from Phase 2 — see its entry above and docs/OUTPUT.md).
- **Tauri command parameter naming:** a Rust parameter literally named
  `loop_` (the `_` suffix worked around `loop` being a keyword) doesn't
  reliably camelCase to a predictable JS key, so `engine_trigger_asset`'s
  parameter is named `looped` instead — avoids relying on undocumented
  behavior at a boundary that's easy to get subtly wrong.

## Phase 4 — Fixture library and patch

- **Did not bundle "thousands of OFL fixtures."** Actually fetching,
  validating and vetting thousands of real fixture files isn't practical to
  do one-by-one here. What's real instead: a genuine OFL JSON importer and a
  genuine QLC+ `.qxf` importer, each built against real fixture data and unit
  tested, so a user can import any OFL/QLC+ fixture file directly (or drop
  OFL JSON files into `src/fixtures/library/` the same way a custom fixture
  works today). Only the small bundled set stays checked into the repo.
- **`FixtureChannel.capabilities` is additive, not a replacement for
  `type`.** A channel whose DMX range does one thing across its whole range
  (most of our own hand-authored fixtures) just keeps using `type`; only
  channels that actually subdivide (wheels, strobe, function-select) carry a
  `capabilities` array. Full row-for-row capability modeling everywhere would
  have meant rewriting every place that currently reads `ch.type` (color
  swatches, the engine's HTP classification, sliders) for no real gain on
  fixtures that don't need it.
- **Matrix/pixel fixtures are rejected, not partially supported.** Both
  importers refuse OFL `matrix`/`templateChannels` fixtures and note that
  full per-pixel addressing is Phase 7's Pixel Map — a `cell` field exists on
  `FixtureChannel` for later, but nothing resolves it yet.
- **QLC+ color/gobo specifics are inferred from the channel's own name**
  when the group is generically "Colour"/"Gobo" — QLC+'s format doesn't
  carry OFL's structured color-name/wheel-slot data, so this is an honest
  limitation of the source format, not a shortcut in the importer.
- **Fixture numbers are now real, user-assignable, and persisted**
  (`PatchedFixture.fixtureNumber`, schema v3), replacing Phase 3's "just
  patch order" placeholder. Existing v2 shows migrate by assigning numbers
  in patch order, so nothing renumbers on upgrade.
- **Per-fixture universe assignment** (`PatchedFixture.universe`, schema v3)
  replaces the implicit "everything is universe 1" from Phases 1-3.
- **The "patch by drag onto a universe grid" view wasn't built.** Address
  collision detection, auto-addressing, re-patch and universe assignment (the
  parts that actually prevent mistakes) were prioritized over a visual drag
  interaction, which is additive UI polish on top of the same underlying
  operations.
- **Live channel display for patched fixtures now reads per-universe engine
  frames** (`useUniverseChannelsStore`), not just the primary universe's
  buffer — needed once a fixture could be patched to any universe, not only
  the first one.

## Phase 5 — Groups, presets, palettes

- **"Group master" is the same operation as `GROUP N @ value` on the command
  line, not a continuously-multiplying scaling layer.** A true multiplicative
  master (everything under it gets scaled, live, regardless of what sets it)
  would need restructuring `engine::merge`'s pipeline so every layer's value
  passes through any group masters affecting its fixtures before HTP/LTP
  resolution — a real, more invasive change. What's here instead: moving the
  slider re-applies the group's intensity directly, same as typing the
  command. Good enough for "a fader that dims a group," not for "a fader that
  rides on top of whatever a chase is doing to that group."
- **Presets are attribute-keyed (e.g. "red", "pan"), not raw-channel-keyed.**
  Recording captures each attribute's *currently rendered* (post-merge)
  value; applying resolves attribute -> channel fresh per **target** fixture
  via `PatchIndexEntry.attribute_channels`. This is what makes "global/per-
  fixture-type/per-fixture" presets in BUILD_PLAN's wording actually mean
  something — the same "Deep Blue" preset can apply to any fixture with
  red/green/blue channels, not just the one it was recorded from.
- **`apply_preset` always re-reads the preset by id — never caches or copies
  its values anywhere.** This is the actual mechanism behind "updating a
  preset changes every cue that references it"; proven by a unit test
  (`updating_a_preset_changes_what_every_future_lookup_sees`) rather than by
  a real cue, since cuelists are Phase 6. Phase 6 reuses `apply_preset`/the
  lookup-by-id pattern as-is rather than inventing a second mechanism.
- **The Palettes panel has its own fixture picker, not a shared "current
  selection".** There's no cross-tab/global selection state yet (the patch
  panel's selection is local component state); building that just so Groups/
  Presets could "use the current selection" would be more new infrastructure
  than this phase needs — a self-contained picker does the same job.
- **Color picker uses HSV internally, not true HSI**, with a "Brightness"
  slider standing in for HSI's intensity axis — same wheel interaction (hue =
  angle, saturation = radius) BUILD_PLAN's "HSI wheel" asks for, well-known
  conversion math (unit-tested), without chasing a less-common color model
  for no visible difference in this UI. CMY faders weren't added: redundant
  with RGB for this app's channel-based model.
- **No preset thumbnails beyond a plain color swatch.** Colour presets show
  their recorded color; other families show a plain tile with the preset's
  name. Real thumbnails (e.g. a gobo image, a position indicator) need
  per-family rendering that isn't worth building before Phase 6 gives
  presets an actual consumer.

## Phase 6 — Cuelists, playback

- **Cue playbacks share `EngineState.playbacks` with legacy Scene/Chase/FX
  assets**, keyed by a `"cue-playback-{id}"` prefix instead of a second merge
  path — both go through the exact same HTP/LTP logic in `merge.rs`. Adding
  a parallel "cue layer" concept would have meant either duplicating the
  merge rules or making `merge_universe` aware of two different layer
  sources; reusing the one abstraction it already has was simpler and
  already proven correct by Phase 3's tests.
- **Override cuelists get a priority tier (`OVERRIDE_PRIORITY_BASE =
  1_000_000_000`), not a special merge case.** This falls out of the same
  "priority determines LTP" rule everything else uses — an override cuelist
  just starts its priority counter a billion higher than standard playbacks,
  still strictly below the programmer's `i64::MAX`.
- **The command line's `RECORD CUE`/`UPDATE`/`DELETE CUE`/`NEXT`/`PREV` now
  execute for real** (they only parsed through Phase 3-5). They operate
  against a "current cuelist" (`engine_set_current_cuelist`) the Cuelists tab
  sets when you open one — there's still no cross-tab selection state (see
  Phase 5's decision above), so this is the smallest thing that makes the
  command line's cue syntax actually do something, without inventing a
  bigger selection-sharing mechanism this phase doesn't need elsewhere.
- **`UPDATE` re-captures whichever cue `RECORD CUE` last touched.** The
  grammar (designed in Phase 3, before cuelists existed) gives `UPDATE` no
  number of its own — `exact(1, Command::UpdateCue)`. Rather than change a
  grammar an earlier phase already committed to and unit-tested, `UPDATE`
  tracks the last-recorded cue number per `ProgrammerContext`.
- **`COPY`/`MOVE` still don't execute.** Same root cause as `UPDATE`: the
  existing grammar parses them with zero arguments, so there's no target cue
  number to copy/move to. Inventing new syntax for this (e.g. `COPY 1 TO 2`)
  wasn't specified by anything this project has already committed to, and
  guessing wrong would mean redoing both the parser and its existing passing
  tests. Left as a clear "not implemented" error rather than a silent no-op
  or a guessed syntax.
- **`RECORD CUE`'s fade time comes from the last `TIME n` command**, applied
  to both fade-in and fade-out — the grammar has no separate in/out syntax
  (`TIME` takes one number). This is also what finally gives `TIME` an
  effect; Phase 3 introduced it as a stored-but-inert value specifically
  pending this.
- **A playback's `current_cue_number` is polled (`PlaybackBar` on a 250ms
  interval), not pushed as an event.** Every other piece of the UI that
  needs the engine's live output (universe frames, universe/channel status)
  has a `listen`-based push event; playback status doesn't get one this
  phase. A 250ms-polled fader/cue readout is unnecessary for a phase whose
  actual complexity was the engine model itself; adding a new
  `engine://...` event class is a small, low-risk follow-up if playback UI
  responsiveness turns out to matter later.
- **Playback bar covers fader modes and Flash; pages/rate-master/main-
  playback-GO-BACK-PAUSE-RELEASE-ALL do not exist.** `FaderMode` (Intensity
  Master / Crossfade, `engine_playback_set_fader_mode`) and a per-slot Flash
  button (bump to full while held, restore on release — a frontend-only
  fader manipulation, no new engine state) are real, working per-slot
  controls, since BUILD_PLAN names both explicitly. A "rate" fader mode
  (playback speed, distinct from a crossfade position) has no defined
  meaning yet outside a Chase's BPM, so it wasn't invented. Paged banks (20+
  pages) and a distinct "main playback" fader with a global RELEASE ALL are
  real UI infrastructure BUILD_PLAN also asks for that a single scrollable
  row of slots doesn't provide — deferred as genuinely unbuilt, not silently
  dropped; see `PlaybackBar`'s own doc comment.
- **Submaster cuelists have no GO/cue-stepping UI**; `create_playback` just
  puts them on cue 0 (tracked) immediately and the fader scales that.
  Building a distinct "submaster" playback slot UI (vs. the standard
  GO/Back/Release row `PlaybackBar` shows for every kind) is deferred —
  the standard controls simply don't do much for a kind that has only one
  effective state, which is an acceptable rough edge, not a broken feature.
- **Timecode cuelists behave exactly like Standard (manual GO only).**
  Nothing drives playback from an external clock — LTC/MTC input is
  explicitly Phase 8's scope, not guessed at here.
- **`Cue.delay_in_ms` and `Cue.mark` are modeled but not acted on.** Both
  fields exist on `Cue` and cross the Tauri boundary via `CueDto`, but
  nothing currently sets `delay_in_ms` to a non-zero value (no UI field for
  it) or reads it in `PlaybackRuntime::go` (a cue's fade always starts
  immediately), and `mark` (move-in-black) has no UI checkbox and no engine
  effect — `cues.rs`'s own doc comment already flagged this. Per-attribute
  timing (different fade times for different channels within one cue) and
  follow/wait/auto-follow/links/loops between cues aren't modeled at all.
  All of these are genuine, real-timing features, not stubs that fake
  correctness — they're left as explicit gaps rather than a half-working
  approximation, since BUILD_PLAN's own "done when" for this phase (a
  20-cue tracking cuelist plays back correctly, a chase follows tapped BPM)
  doesn't require them and they're each substantial enough to deserve their
  own real design pass rather than a rushed addition here.
- **The "Triggers" tab is the renamed Phase 3/4 `TriggerGrid`** (legacy
  Scene/Chase/FX one-shot triggers), kept as its own tab distinct from the
  new "Cuelists" tab rather than merged into it — they're different data
  models (`LightAsset` vs. `Cuelist`/`Cue`) with different lifecycles, and
  conflating them in one screen would have been more confusing than two
  clearly-named tabs.

## Phase 6 follow-up — Show persistence

- **Groups, presets and cuelists are saved in the show file (schema v4)
  through the engine, not a frontend store.** The engine owns this data at
  runtime, so Save calls `engine_export_show_data` and Open/New call
  `engine_load_show_data`. A second copy in a Zustand store would just be a
  second source of truth to keep in sync. Ids are kept on load so cue→preset
  references stay valid, and new ids continue after the highest loaded one.
- **Cuelists are stored in the flattened `CuelistDto` shape**, the same one
  the UI already uses, because `Cue.values` is keyed by `(universe, channel)`
  tuples, which can't be JSON object keys.
- **Dirty tracking for engine data uses a polled revision counter**
  (`engine_show_data_revision`), not change events. Every engine edit bumps
  it; the frontend compares it to the revision at the last save/load. It's
  one cheap integer read and doesn't need a new event channel.
- **Playbacks aren't saved.** They're runtime state (which cue is live,
  fader positions), and loading a show drops running cue playbacks since
  they point at the outgoing cuelists.
- **On launch, the app restores whichever is newer: the newest autosave or
  the last opened/saved show** (`showfile_restore_session`). An autosave is
  reattached to its show's path (if it has one) and marked dirty, so recovered
  work is never silently treated as saved.

## Phase 7 — Effects engine and pixel mapping

- **A minimal global tempo was built here, not in Phase 8.** Phase 7's
  done-when needs "BPM-synced", and the only BPM so far was per chase
  cuelist. `engine::tempo` has a manual BPM, tap, and a beat clock. Phase 8
  adds audio detection, Ableton Link and MIDI clock as more ways to set the
  same BPM. Chase cuelists keep their own BPM for now; locking them to the
  global tempo is a Phase 8 decision.
- **The beat clock is integrated per tick, not computed from wall time.**
  Changing the BPM then changes speed without making every synced effect
  jump to a new phase. Hz-rate effects use the engine's elapsed seconds
  directly, so changing an effect's Hz *does* jump its phase. That's
  acceptable for a parameter you set while programming, not while running.
- **Effects run inside the owning layer** (`Layer.fx_values` beat
  `Layer.values`), not as a separate post-merge pass. That way an effect in
  an Override cuelist wins like an Override, and a dimmer effect is HTP
  like any dimmer, with no new merge rules to reason about.
- **Effects don't track.** `RECORD CUE` captures a literal for every
  channel of the selected fixtures, so a tracked effect could never be
  stopped by a later cue. A cue runs only its own effects. Re-record an
  effect into each cue that should have it; when it's the same programmer
  effect (same id), GO between those cues doesn't interrupt it.
- **A new effect on an attribute replaces the old one for those fixtures**
  instead of stacking two effects on the same channel. That matches what a
  programmer expects from re-applying an effect. Stacking still happens
  across layers (a cue's effect plus a programmer effect).
- **Editing a preset-linked running effect unlinks it.** It's no longer
  playing the preset's definition, so following later preset edits would
  overwrite the live edit. Use ↻ on the preset to push the edit into the
  preset instead.
- **Effect presets hold a whole effect definition** (`Preset.effect`)
  rather than attribute values. The Palettes tab's Effect family no longer
  offers "record from selection" (it would have recorded plain values);
  recording happens in the Effects tab.
- **Pan/tilt effects drive the coarse channel only.** The patch index
  carries one channel per attribute, and fine channels aren't modelled
  there yet. Fine-channel output is part of the 16-bit work for moving
  heads.
- **"Step" is four equal levels and "pulse" a smooth bump over `width`%.**
  The plan names both without defining them; these are the common
  meanings.
- **The intensity-master fader moved from `PlaybackRuntime::render` to the
  merge** (`Layer.intensity_scale`). Otherwise a dimmer effect on a cue
  would ignore its playback's fader.
- **Pixel-map media is decoded in Rust and scaled at import.** The `image`
  crate (MIT/Apache) handles PNG/JPEG/GIF. Frames are stored at the map's
  size as base64 RGB in the show file, so a show stays self-contained and
  small, and a later map resize resamples (nearest-neighbour). Imports are
  capped at 8 MB and 300 frames.
- **Video onto the grid was skipped.** It needs ffmpeg or a platform
  decoder, which is a large native dependency for a feature a GIF covers
  for most DJ-set use. Recorded here per the "skip and note" rule.
- **The scriptable generator (sandboxed JS) was skipped.** The plan marks
  it optional, and doing it properly means embedding a JS engine (e.g.
  `boa`) with time and memory limits in the engine thread. The eight
  built-in generators cover the plan's list.
- **Pixel maps aren't recordable into cues yet.** They run from the Pixel
  maps tab as their own playback layers, with a master. Triggering a map
  from a cue or a playback-bar slot belongs with the playback-bar pages
  work (still open from Phase 6) or the Virtual Console (Phase 10).
- **The fire generator steps at `height` steps per cycle** and catches up
  at most four steps per tick, so after a long pause it doesn't spend a
  tick simulating seconds of flames.
