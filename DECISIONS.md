# Decisions

One line (or a short paragraph) per call made without stopping to ask.
Newest at the bottom. Decisions about features that were later removed
are gone with them (they're in git history).

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

## Show persistence

- **On launch, the app restores whichever is newer: the newest autosave or
  the last opened/saved show** (`showfile_restore_session`). An autosave is
  reattached to its show's path (if it has one) and marked dirty, so recovered
  work is never silently treated as saved.

## Tempo

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

## Phase 8 — Timing sources

- **Audio and MIDI in Rust: `cpal` (Apache-2.0) and `midir` (MIT).** Both
  free and permissive. `cpal` 0.18 needs ALSA headers on Linux builds
  (`libasound2-dev`); macOS and Windows need nothing extra.
- **No FFT crate.** A 60-line radix-2 FFT in `timing/analysis.rs` covers the
  one transform size used, so there's no `rustfft` dependency to track.
- **Beat detection tracks tempo, not phase.** Onset autocorrelation gives a
  stable BPM within the ±1 target on steady music; aligning the beat grid to
  the downbeat is unreliable without a tap or a bar-level model. TAP still
  re-aligns the grid. Half-time ambiguity above ~160 BPM is accepted (a lighting
  chase at half speed is musically fine).
- **Only locked estimates move the tempo.** Four consecutive estimates within
  1.5 BPM; until then the BPM holds its last value rather than wandering.
- **Manual and Tap are separate selector entries but behave the same**: both
  mean "nothing automatic". Tap and the BPM field work in every mode, though
  an automatic source overwrites them on its next estimate.
- **Ableton Link was skipped.** The Link SDK is GPL-2.0-or-later (a separate
  commercial licence is available from Ableton), and the Rust crates bind that
  SDK. Linking it would force the app under the GPL or need a paid licence,
  which the project rules out. Revisit if Ableton's terms change or a
  clean-room, permissively licensed implementation appears. MIDI clock (which
  Ableton Live and most DJ software can send) covers the same need.
- **Pioneer Pro DJ Link was skipped.** Its protocol is only known from
  reverse engineering and needs real CDJs to build against.

- **Timecode source, devices and audio triggers aren't saved in the show.**
  Device ids differ per machine, and triggers point at playback ids that exist
  only for a session (playbacks aren't saved either). The Timing tab is
  per-session. Persisting machine settings belongs with a settings store.
- **LTC is decoded from a mono channel with zero crossings.** It copes with
  fractional bit lengths and low levels; it does not handle reverse play or
  29.97 drop-frame labelling (30 fps is assumed).

## Node-graph redesign (2026-10-20)

- **Three tabs, no programmer.** The UI had grown one tab per engine phase
  (13 tabs plus a sidebar), which made it clunky to use. The show is now a node graph. The command line,
  cuelists, presets, the effects engine, pixel maps, the timeline, audio
  triggers, stage plot, 3D view, DMX monitor and the built-in AI chat were
  removed, front and back. The playback bar stays as faders and buttons
  that are graph inputs, and looks are switched inside the graph
  (Switch/Mix nodes) rather than by cues.
- **Rust owns the show.** Patch, groups, universes and the graph moved from
  frontend stores into `show::ShowService`, because the MCP server and the UI
  must edit the same thing with one undo history. Undo is a snapshot stack
  over patch + groups + graph (they're small). Universes aren't undoable:
  they reconfigure hardware.
- **The frontend still owns the fixture library** and pushes it to Rust:
  imports (OFL, QLC+) and the fixture editor are TypeScript, and Rust only
  needs channel offsets, attributes, defaults and capabilities.
- **Per-fixture, shader-style evaluation** instead of an effects engine with
  spread/groups/blocks/wings parameters: nodes upstream of an Output run once
  per target with that target's index, so Fan (which keeps the old
  groups/blocks/wings maths) or Fixture Index feeding an LFO's phase gives
  the same fanned effects, and composes with everything else.
  Fixture-independent nodes run once per tick.
- **One registry for node types** (`graph/registry.rs`). The editor renders
  generic nodes from it, MCP clients read it via `list_node_types`, and
  docs/NODES.md is generated from it. Nothing about a node is duplicated in
  TypeScript.
- **Only connected Output inputs are written.** An Output with just a colour
  linked leaves intensity to other outputs. That's what makes layering
  several outputs on the same fixtures useful.
- **Show format 9 starts fresh**; older development formats are refused with
  a clear message rather than migrated (backward compatibility wasn't needed).
- **MCP over streamable HTTP inside the app**, not a stdio sidecar: the
  server has to reach the live show, and HTTP lets any number of clients
  connect to the running app. rmcp's default Host allowlist (localhost) is
  the DNS-rebinding protection. It's on by default, since that's the point
  of having it built in.
- **MIDI moved to Rust** (midir): the graph's MIDI nodes read the same port
  as MIDI clock and MTC. The Web MIDI note→asset mapping went with the
  legacy assets.
- **MIDI binds to Buttons and Faders, not nodes** (2026-10-26). The MIDI CC
  and MIDI Note nodes were removed: a Button or Fader takes a MIDI binding
  (note or CC, channel, number; or Learn) and the MIDI moves its
  playback-bar value, so a pad and a click are the same press and the bar
  shows what the controller did. A quick pad hit on a flash Button is
  latched until the next tick so it's never missed. Shows with the old
  nodes don't load (no migration; nothing depends on them yet).

