# The lighting engine

`src-tauri/src/engine/` computes what actually goes out on the wire: it merges
the programmer and every running playback into a final per-universe buffer at
a fixed 44Hz, independent of the UI's frame rate.

## Why channel-based, not fixture/attribute-based

The engine's `Layer` type (`state.rs`) only knows about `(universe, channel)
-> value`. Resolving "fixture 3's red channel" into an actual DMX address is
static configuration data (the fixture library + patch), not timing-critical,
so that resolution stays in the frontend, which already has the fixture
library (`src/lib/fixtures/`) — duplicating it in Rust would just be a second
copy to keep in sync. Only the *resolved* result crosses into Rust:

- `engine_set_htp_channels(universe, channels)` — which channels merge via HTP
  (a fixture's dimmer channel) vs LTP (everything else). Recomputed and pushed
  whenever the patch or fixture library changes (`useShowStore.ts`'s
  `syncEnginePatch`).
- `engine_set_patch_index(entries)` — fixture number -> (universe, intensity
  channel), what the command line's `1 THRU 8 @ 50` resolves against. Fixture
  numbers are just 1-based patch order for now; real fixture ID assignment is
  Phase 4's Patch screen.

## Merge model

`merge::merge_universe` (see its module docs and tests): intensity channels
are HTP (max across every layer, regardless of priority); everything else is
LTP (the highest-priority layer with an entry for that channel wins outright).
The programmer's priority is `i64::MAX`, so "programmer > playbacks" is just a
consequence of the general rule, not a special case.

## Playback layers

Every triggered Scene/Chase/FX (`src/store/useLibraryStore.ts`'s `trigger`,
or the Live AI console) becomes one playback layer
(`legacy_playback::RunningPlayback`), advanced every tick instead of on its
own `setTimeout` — this is what BUILD_PLAN Phase 3 means by "retire
chaseEngine.ts; replace it with a Rust playback that's frame-accurate". Two
playbacks that touch the same channel now go through the same HTP/LTP merge
as everything else, instead of racing to call `applyPatch` last the way the
old runner did.

A non-looping playback (a "Scene") keeps its last values live after it
finishes ticking — matching `LightAsset`'s own "Scenes always apply steps[0]
once" contract (a static look that persists) rather than reverting once its
`RunningPlayback` stops. `stop_asset`/re-triggering the same id is what
actually removes a layer.

## Groups and presets (Phase 5)

- `engine::groups::GroupStore`: a named, persistent set of fixture numbers.
  The command line's `GROUP N` selection term resolves against these (see
  `SelectionExpr::resolve`'s `groups` parameter).
- `engine::presets::PresetStore`: a named set of *attribute* values (e.g.
  "red" -> 200.0), one store shared across all seven families
  (Intensity/Colour/Position/Beam/BeamFx/Framing/Effect), filterable by
  family. Recording reads each target fixture's currently-rendered value via
  `merge_universe`; applying resolves attribute -> channel fresh per target
  fixture through `PatchIndexEntry.attribute_channels`, so the same preset
  can apply to any fixture with matching attributes — not just the one it
  was recorded from, and never a snapshot copied into whatever uses it. See
  DECISIONS.md for what a preset apply and a "group master" fader are (and
  aren't).

## Cuelists and playback (Phase 6)

- `engine::cues::{Cuelist, Cue, CueValue}`: a cuelist is a `Vec<Cue>` kept
  sorted by (possibly decimal) cue number. A cue's value per channel is
  either `Literal(f32)` or `Preset { preset_id, attribute }` — the latter
  reuses `apply_preset`'s "resolve by id, never copy" pattern, so updating a
  preset changes every cue that references it, immediately, with no
  propagation step. `resolve_cue_state` computes a cue's effective state
  fresh on every call: with `tracking` on, it layers cues `0..=index`
  (a later cue's value for the same channel wins); with tracking off, just
  the cue's own values. This is deliberately *not* cached — editing an
  earlier cue immediately changes what every later cue tracks in.
- `engine::playback::PlaybackRuntime`: a running instance of a cuelist —
  GO/Back/Release, a time-based crossfade (`FaderMode::IntensityMaster`,
  the fader scales only HTP/intensity channels) or a manual crossfade
  fader (`FaderMode::Crossfade`), and for a `CuelistKind::Chase`, BPM-driven
  auto-advance (Forward/Bounce/Random). Multiple playbacks can run the same
  or different cuelists concurrently — `EngineManager::create_playback`
  hands out a fresh id and priority per slot.
- Cue playbacks render into `EngineState.playbacks` — the **same** map
  legacy Scene/Chase/FX assets use — keyed by `"cue-playback-{id}"` instead
  of the legacy asset's own id, so both go through identical HTP/LTP merge
  logic with no separate code path. A `CuelistKind::Override` playback gets
  a priority at or above `manager::OVERRIDE_PRIORITY_BASE`
  (1,000,000,000), below the programmer's `i64::MAX` but above every
  standard/chase playback and legacy asset, so it always wins LTP
  regardless of trigger order.
- `CuelistKind::Submaster`: no GO/cue-stepping; `create_playback` puts it on
  cue 0 immediately and its fader directly scales that (tracked) state.
  `CuelistKind::Timecode` behaves like `Standard` (manual GO only) — nothing
  drives it from an external clock yet; LTC/MTC input is Phase 8.
- The command line's `RECORD CUE`/`UPDATE`/`DELETE CUE`/`NEXT`/`PREV` are
  now live: they operate against whichever cuelist
  `engine_set_current_cuelist` last selected (the Cuelists tab calls this
  when you open a cuelist) and, for `RECORD CUE`, the command line's own
  selection + `TIME` value (`RECORD CUE`'s fade-in/out both come from the
  last `TIME n` — there's no separate in/out syntax). `UPDATE` re-captures
  whichever cue number `RECORD CUE` last touched (the grammar gives it no
  number of its own). `NEXT`/`PREV` step the first playback slot attached
  to the current cuelist; if none exists yet, they return a clear error
  rather than silently doing nothing. `COPY`/`MOVE` still parse (and are
  unit-tested) but stay unimplemented — the existing grammar gives them no
  target-number argument, and inventing one isn't this phase's call to
  make; see DECISIONS.md.
- `CueDto`/`CuelistDto`/`CueValueEntry` flatten `Cue.values`'s
  `HashMap<(u32,u16), CueValue>` into a plain list crossing the Tauri
  boundary, the same reason `PatchIndexEntry` exists — JSON object keys
  must be strings, and a `(universe, channel)` tuple isn't one.

## Tempo, effects and pixel maps (Phase 7)

- `engine::tempo::Tempo`: one global BPM (typed in or tapped in the top
  bar) and a beat position that the tick *integrates* (`advance`). Because
  it isn't recomputed as wall-clock × BPM, changing the BPM changes the
  speed from that moment on without making synced effects jump. A tap also
  rounds the beat position to a whole beat, so tapping along puts effects
  on the downbeat. Phase 8's audio/Link/MIDI-clock sources will just call
  `set_bpm`.
- `engine::effects`: pure maths. An `EffectDef` is a waveform on one
  attribute (sine, square, saw, ramp, random, step, pulse) or a pan/tilt
  shape (circle, figure-8, ballyhoo, pan saw, tilt saw), with size, rate
  (`Hz`, or `Beats` per cycle at the global BPM), fan spread, phase, width,
  direction, and groups/blocks/wings. `Relative` swings around the
  underlying value; `Absolute` around a fixed centre. An `EffectInstance`
  applies a def to an *ordered* fixture list; the order is the fan order.
- **Effects run inside layers.** `Layer` has `effects` (with a crossfade
  weight each) and `fx_values`, which the tick fills by rendering them and
  which win over the layer's own `values` for the same channel. So an effect
  merges HTP/LTP exactly like the layer it lives in, with no separate path.
  A relative effect's "underlying value" is its own layer's value for that
  channel, else the effect-free merge of the whole show
  (`merge_universe_static`).
- **Programmer effects** (`start_effect` and friends): starting an effect on
  an attribute takes those fixtures away from any programmer effect already
  running on the same attribute (it replaces, it doesn't stack). CLEAR's
  second press and RELEASE stop them along with the values.
- **Effects in cues:** `RECORD CUE` captures the programmer's effects for
  the selected fixtures into `Cue.effects` (values are captured from the
  effect-free merge, so a running effect is recorded as an effect, not as
  whatever value it happened to have that instant). A playback crossfades
  the outgoing cue's effects out and the incoming cue's in; an effect both
  cues share keeps running at full weight. Effects don't track (see
  DECISIONS.md).
- **Effect presets** are `Preset`s with `family: Effect` and an `effect`
  def. Applying one starts the effect linked by `preset_id`, and a linked
  instance renders the preset's *current* def every tick, so updating the
  preset changes every cue that uses it.
- **Fader and effects:** a playback's intensity-master fader is now the
  layer's `intensity_scale`, applied to HTP channels at merge time (after
  effects), so a dimmer effect fades with its fader.
- `engine::pixelmap`: a `PixelMap` is a W×H grid. `PixelMapRuntime::
  render_frame` produces one RGB per position from a generator (colour
  chase, rainbow, wave, plasma, text scroll with a built-in 5×7 font, fire,
  noise, or imported image/GIF frames), independent of what's mapped, so an
  unmapped "virtual matrix" still renders and previews.
  `frame_to_channels` writes mapped cells' colours into their fixture or
  fixture cell (`PatchIndexEntry.cells`): RGB, CMY as the inverse, or
  brightness for a dimmer-only cell. A fixture with a dimmer is driven to
  full, and the map's master scales it through the layer's
  `intensity_scale`. A running map is a playback layer
  (`"pixelmap-{id}"`). Imported media is decoded in Rust (`image` crate),
  scaled to the map at import time, and stored base64 in the show file.
- **The tick** now lives in `TickShared::advance` (tempo → legacy assets →
  cue playbacks → pixel maps → effects), which the 44Hz thread calls before
  merging and sending. Tests call it directly through `EngineManager::step`.
  Lock order is patch index → state → … everywhere, so the tick can't
  deadlock against `apply_intensity`/`record_cue`.

## Timing sources (Phase 8)

Everything below lives in `src-tauri/src/timing/` (inputs and DSP) and
`engine/{timecode,audio_reactive}.rs` (the state the tick reads). Inputs
write into shared `Arc<Mutex<..>>` state obtained from
`EngineManager::timing_handles()`; nothing here touches the output threads.

- **Tempo sources** (`TimingSettings.tempo_source`): Manual and Tap (nothing
  automatic), Audio, MIDI clock. All end in `Tempo::set_bpm`. Chases keep their
  own BPM unless `chases_follow_tempo` is on, in which case the tick hands
  every chase playback the show BPM (`PlaybackRuntime::tempo_bpm`).
- **Audio** (`timing::audio_input`, `timing::analysis`): `cpal` captures one
  channel of one input device on its own thread. `Analyzer` runs a 1024-point
  FFT every 256 samples and produces (a) low/mid/high band levels
  (30-250 Hz, 250-2.5 kHz, 2.5-12 kHz, normalized against a slowly decaying
  peak, 0-1) and (b) a spectral-flux onset signal. Every 0.5 s the last 8 s of
  onsets are autocorrelated to a beat period in 70-180 BPM (octave ties lean
  towards 120), refined on the 2nd-4th multiples of the period for
  sub-BPM resolution, and median-filtered. The estimate is "locked" when the
  last four agree within 1.5 BPM; only a locked estimate drives the tempo.
  Tested against synthesized click tracks (90-150 BPM within 1 BPM, with
  off-beat hats and a bass hum). It tracks the tempo, not the beat phase: the
  beat grid isn't aligned to the audio's downbeat. Very fast tracks (above
  ~160) can lock to half time.
- **MIDI** (`io::midi`, `timing::mtc`): `io::midi::{list_input_ports,
  open_input}` is the reusable port layer (Phase 9 builds on it). MIDI clock
  (24 PPQN) is averaged over two beats; Start snaps the beat grid. MTC
  quarter frames (plus full-frame SysEx) decode to a position, advanced a
  quarter frame per message and corrected by +2 frames on each complete
  sequence.
- **LTC** (`timing::ltc`): a biphase-mark decoder over zero crossings of one
  audio channel (adaptive bit period, sync word 0xBFFC); 24/25/30 fps
  detected from the bit rate. A decoded frame is positioned one frame after its
  label (it has just finished).
- **Timecode clock** (`engine::timecode::TimecodeClock`): one position with a
  selectable source (Internal / MTC / LTC). Internal is the Timeline
  transport (play/pause/stop/seek). External sources count as running only
  while positions keep arriving (250 ms timeout). Each tick, while the clock
  runs (and once after any seek), every Timecode-cuelist playback jumps to
  `target_index`: the cue with the latest `timecode_ms` at or before the
  position. That is chase-style, so scrubbing backwards or locating re-fires
  the right cue. A manual GO still works. **Learn mode** stamps the cue a GO
  lands on with the current position.
- **Audio-reactive triggers** (`engine::audio_reactive`): a trigger maps a
  band to a playback flash (`PlaybackRuntime::audio_flash` lifts the
  intensity scale, like holding Flash) or an effect's size (multiplies its
  crossfade weight). Level = band × sensitivity × 2, cut below the gate, fast
  attack, linear release. Triggers reference runtime playback / effect ids and
  aren't saved with the show.
- **Show timeline** (`TimelinePanel.tsx`): the UI draws the audio waveform
  (decoded in the webview), cue blocks at their `timecode_ms` and a playhead on
  the engine clock; dragging a block sets its time, clicking the ruler or
  waveform seeks. With the internal clock the webview plays the audio file in
  step with the engine's position (re-seeking on drift). The audio isn't
  sample-locked to DMX; it is a monitor, and MTC/LTC are the sample-accurate
  routes.

## What isn't here yet

- **An attribute encoder bar / full manual programmer UI**: `CommandLine.tsx`
  is the only manual programmer surface today. The Onyx-style attribute bar
  (BUILD_PLAN section 2) is progressive UI work built up as later phases add
  the window types it needs.
- **DMX input merged into output**: `output::input` receives and decodes
  Art-Net/sACN but doesn't feed it into this engine's merge — see
  docs/OUTPUT.md's scope note.
