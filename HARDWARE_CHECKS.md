# On-rig / on-desktop checks

Things a coding agent can't verify from here — either because they need the
physical rig (see `FIXTURES.md`), or because they need the actual built
desktop app rather than the browser-only dev preview this environment can
drive. Check these off by hand.

## Node-graph redesign (2026-10-20)

- [ ] **Detect controllers finds the rig.** With the FTDI widget (and/or an
      Enttec Pro, an Art-Net node) connected, Setup → Detect lists each one
      once (macOS: the `cu.` port, not `tty.`), and "Add as universe" makes
      it output.
- [ ] **A graph drives the real pars.** Group the 4 pars, then Fixture Output
      (the group) ← Color + Number 1: all four show the colour at full. Add
      Fan → LFO → intensity: a wave visibly travels across them in group
      order, and dragging members in the Groups panel changes the order.
- [ ] **RGBW/UV mixing looks right.** On the 6-colour pars, white, pastel
      and saturated colours from a Color node look right (white LED takes
      the shared part, amber/UV stay off unless driven via "More
      attributes").
- [ ] **Function channels stay in DMX mode.** Freshly patched fixtures with
      a function/mode channel don't start a built-in program with an empty
      graph.
- [ ] **No stutter while editing.** Drag nodes, scrub numbers and rewire
      while an LFO runs: the output doesn't glitch (compiles happen off the
      tick, and the tick swaps plans atomically).
- [ ] **MIDI Learn and the playback bar.** A Fader's Learn MIDI binds a knob
      on the real controller: turning it moves the playback-bar fader and
      the intensity. A flash Button learned to a pad blinks the rig while
      held, even on very quick hits; a toggle Button flips per hit. Clicking
      on the bar still works alongside.
- [ ] **An agent over MCP.** `claude mcp add --transport http lumendmx
      http://127.0.0.1:7410/mcp`, ask Claude Code for "a red/blue alternating
      chase on the beat across the pars": the nodes appear in the editor, the
      rig does it, and Cmd+Z undoes it.

## Phase 1 — Foundations

- [ ] **New/Open/Save/Save As dialogs.** These use native file dialogs (`rfd`
      crate) that only work inside a real Tauri window — the sandboxed browser
      preview used during development can't invoke them (calls fail with
      "Cannot read properties of undefined (reading 'invoke')", which is
      expected and handled gracefully). In the built app: create a show with a
      couple of patched fixtures, Save, quit, relaunch, Open it back up, and
      confirm the patch reappears.
- [ ] **Autosave.** Leave the app open with an unsaved change for a couple of
      minutes and confirm files appear under the OS app-data directory at
      `LumenDMX/autosave/<show id>/` (Windows: `%APPDATA%/com.pashchenkos.lumendmx/`),
      and that only the 10 most recent are kept.

## Phase 2 — Multi-universe output engine

- [ ] **FTDI flicker regression check.** The output thread was generalized
      from a single hardcoded engine into `output::runner::UniverseOutput` +
      `output::ftdi::FtdiDriver`. The break/MAB timing, thread priority, and
      pacing are unchanged by inspection (see docs/OUTPUT.md), but re-run the
      original zero-flicker check on the real rig (COM9, 4 pars) before
      trusting it on a show.
- [ ] **Art-Net interop.** Point a real Art-Net node (or a software one like
      QLC+ or an Art-Net-to-sACN bridge) or Wireshark at a universe configured
      as Art-Net in Setup and confirm frames arrive with the right net/subnet/
      universe and channel values. Also try ArtPoll discovery against that node.
- [ ] **sACN interop.** Point sACNView (or another sACN receiver/viewer) at a
      universe configured as sACN in Setup (try both multicast and unicast)
      and confirm the universe number, priority, and channel values are right.
- [ ] **Enttec DMX USB Pro.** No physical Pro widget was available to build
      this against — it's implemented from the documented "Send DMX Packet"
      protocol only. Needs a real Pro (or Pro Mk2) widget to confirm the frame
      format is accepted and outputs correctly. The Mk2's second universe
      (`universeIndex: 1`) is deliberately unimplemented (returns an error)
      until someone can confirm its port-select framing on real hardware.
- [ ] **4 simultaneous universes.** Per the phase's own "done when": run 4
      universes at once (e.g. FTDI + Art-Net + sACN + a second Art-Net
      universe) and confirm all four keep outputting correctly under load.

## Phase 3 — Engine core

- [ ] **No stutter under UI load.** Per the phase's own "done when": freeze
      the UI thread for ~2s (e.g. a heavy re-render or a breakpoint) and
      confirm the FTDI rig's output doesn't glitch — the 44Hz engine tick
      runs on its own thread, independent of the UI, but this needs an actual
      running app + rig to confirm, not just code inspection.

## Phase 4 — Fixture library and patch

- [ ] **Patch a real OFL moving head.** Fetch a real fixture's OFL JSON
      (there's no moving head on this rig — see FIXTURES.md), import it via
      "Import OFL", patch it, and confirm dimmer/pan/tilt/color-wheel/gobo
      channels actually move the fixture correctly — the importer's own
      correctness is unit-tested, but real hardware interpreting the
      resulting channel values is not.
- [ ] **Re-verify the 4 pars after the patch-panel rewrite.** The live
      channel display now reads per-universe engine frames instead of the
      old single-universe buffer read; confirm the pars' sliders/color
      presets still drive real DMX output correctly on the rig.
- [ ] **Try a QLC+ `.qxf` import** against a real fixture file from QLC+'s
      own fixture repository (not just this project's test fixture) to
      catch any real-world `.qxf` quirk the unit tests didn't cover.

## Show persistence

- [ ] **Session restore on launch.** Make an unsaved change, wait for an
      autosave (~60s), quit without saving, relaunch: the app should reopen
      that work marked unsaved. Then Save, quit, relaunch: it should reopen
      the saved file, not an older autosave.

## Phase 8 — Timing sources

These need real audio, a MIDI device and the built desktop app (the unit
tests use synthesized signals).

- [ ] **Audio beat lock.** Setup tab: pick the line-in / loopback device,
      set Tempo source to Audio, play a steady 4-on-the-floor track. After
      ~6 s the badge should read Locked and the top bar BPM should match the
      track within ±1. Try a track with a long breakdown (the BPM should hold,
      not wander) and a very quiet input.
- [ ] **Device permissions.** On macOS the first capture asks for microphone
      access; confirm the prompt appears and that denying it shows an error in
      the Setup tab instead of a crash. `src-tauri/Info.plist` supplies
      `NSMicrophoneUsageDescription`; check it ends up in the built bundle's
      Info.plist (the prompt never appearing means it didn't).
- [ ] **Spectrum meters and the Audio node.** With Analysis on, the
      Low/Mid/High bars should follow kick / vocals / hats, and an Audio
      node's Low wired to an Output's intensity should pulse the rig on kicks.
- [ ] **MIDI clock.** Send clock from a DAW or DJ app to a virtual MIDI port
      (IAC / loopMIDI). Pick it as the Clock / timecode port and Tempo source
      MIDI clock: the BPM should follow within 0.1 and Start should snap the
      beat light.
- [ ] **Clock and controllers together.** Keep the clock running and tick a
      MIDI keyboard or controller under Controller ports (plus a second one,
      if you have it). Turn knobs and hit pads: they show in the MIDI monitor
      with their port, and bound Buttons / Faders follow them, while the BPM
      keeps following the clock. Notes/CCs sent on the clock port alone must
      not reach the nodes, and the keyboard must not move the BPM.
- [ ] **MTC.** Send MTC from a DAW. Set Timecode source to MTC, select the
      port: the Setup tab's clock and a Timecode node should track it,
      including after relocating backwards.
- [ ] **LTC.** Play an LTC file/generator into a line input, set the
      source to LTC and the right device/channel: the clock should lock within
      a second. Check 25 and 30 fps, and a quiet signal.
- [ ] **TAP latency.** Tap along to a track at a known BPM and confirm the
      displayed BPM lands within about ±1 and the beat light sits on the
      beat.
