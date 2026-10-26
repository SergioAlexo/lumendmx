# Multi-universe output

## Architecture

`src-tauri/src/output/`:

- **`OutputDriver` trait** (`mod.rs`): one implementation per way of getting a
  universe onto the wire/network. Just `write_frame(&mut self, &[u8;512]) ->
  Result<(), String>`, plus `needs_realtime_priority()`.
- **`runner::UniverseOutput`**: one dedicated OS thread per active universe,
  looping at that universe's configured rate. Generalized from the original
  single-FTDI-universe thread — see "Preserving the FTDI path" below.
- **`manager::OutputManager`**: owns every active universe (`HashMap<u32,
  ActiveUniverse>`); `configure()` creates or replaces a universe's driver,
  which stops its old thread first (via `UniverseOutput`'s `Drop`).
- **Drivers**: `ftdi`, `enttec_pro`, `artnet`, `sacn`, `null`.
- **`input`**: Art-Net/sACN receivers — see its own scope note below.
- **`config::DriverConfig`**: the tagged-union config for all of the above,
  shared between the show-file schema (`showfile::schema::UniverseConfig.driver`)
  and the manager's runtime API — the same JSON shape lives on disk and in memory.

## Preserving the FTDI path

Never regress the real hardware break, the time-critical thread priority,
or the frame pacing on the FTDI Open DMX driver. Concretely, across this refactor:

- The break/MAB timing and the `set_break`/`clear_break` calls in
  `output::ftdi::write_dmx_frame` are byte-for-byte what `dmx.rs` had.
- The thread priority bump moved from "always applied once at the top of the
  one hardcoded thread" to "applied per-universe-thread when
  `driver.needs_realtime_priority()` is true" — still unconditionally true for
  `FtdiDriver`, so no behavior change there; it's just now correctly `false`
  for network drivers, which don't need it.
- The old thread's reconnect backoff (retry opening the port at most every
  500ms) moved from the thread loop into `FtdiDriver::write_frame` itself,
  since drivers now own their own connection lifecycle. Same 500ms constant,
  same behavior.
- Frame pacing moved from a hardcoded 33333us constant into
  `runner::UniverseOutput`'s generic `frame_interval` (computed from
  `DriverConfig::rate_hz()`), which for the FTDI driver still defaults to 30Hz.

## Driver notes

- **FTDI Open DMX**: no onboard framing chip — the host generates the real
  DMX break. This is the hardware this project was built and tuned against
  (see FIXTURES.md); don't touch its timing without the on-rig flicker check
  in HARDWARE_CHECKS.md.
- **Enttec DMX USB Pro**: has an onboard MCU that generates DMX timing from a
  framed serial message (`output::enttec_pro`), so no host-side break is
  needed. There's no physical Pro widget to test against, so this was built
  from the documented protocol only, and the Mk2's second universe is
  explicitly rejected (see the comment in `enttec_pro.rs`) rather than
  guessing at unverified port-select framing.
- **Art-Net 4** (`output::artnet`): ArtDMX encode/decode and ArtPoll/
  ArtPollReply discovery, unit-tested byte-for-byte against the spec plus a
  same-host UDP loopback round trip.
- **sACN / E1.31** (`output::sacn`): the root/framing/DMP layered packet,
  unit-tested against the standard 638-byte full-universe packet size and each
  layer's field offsets, plus a loopback round trip.

Real interop for Art-Net/sACN (an actual node, a viewer like sACNView, or
Wireshark) and for the Enttec Pro (actual hardware) still needs on-network/
on-rig verification — see HARDWARE_CHECKS.md.

## DMX input — scope note

`output::input::DmxInputManager` receives and decodes Art-Net and sACN input
into per-universe frames, but does **not** merge that input into any
universe's live output. Merging it belongs in the engine's merge, not as a
second merge bolted onto `runner::UniverseOutput`, the code path the
safety-critical FTDI driver runs through (see DECISIONS.md). The
receive/decode side is real, tested, and reusable once that's wired up.
