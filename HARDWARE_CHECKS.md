# On-rig / on-desktop checks

Things a coding agent can't verify from here — either because they need the
physical rig (see `FIXTURES.md`), or because they need the actual built
desktop app rather than the browser-only dev preview this environment can
drive. Check these off by hand; each entry says which phase added it.

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
- [ ] **DMX input.** `output::input::DmxInputManager` receives and decodes
      Art-Net/sACN input (unit-tested for the decode logic) but has no UI yet
      and isn't merged into any output — there's nothing to check on the rig
      for this one until Phase 3 (merge) and Phase 12 (DMX monitor UI) exist.

## Phase 3 — Engine core

- [ ] **No stutter under UI load.** Per the phase's own "done when": freeze
      the UI thread for ~2s (e.g. a heavy re-render or a breakpoint) and
      confirm the FTDI rig's output doesn't glitch — the 44Hz engine tick
      runs on its own thread, independent of the UI, but this needs an actual
      running app + rig to confirm, not just code inspection.
- [ ] **Command line against the real patch.** Patch the 4 pars, then try
      `1 THRU 4 @ FULL`, `1 THRU 4 @ 0`, and confirm all four actually go to
      full/zero on the rig, not just that the command reports success.
- [ ] **Two overlapping triggered Scenes on the real rig.** Trigger two
      Scenes that both touch the same par's dimmer and a color channel;
      confirm dimmer shows the brighter one (HTP) and color shows whichever
      was triggered more recently (LTP), matching engine::merge's tests.

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

## Phase 5 — Groups, presets, palettes

- [ ] **Record and apply a real Colour preset on the 4 pars.** Pick a color
      with the wheel, record it, then apply it to a different par than the
      one it was recorded from, and confirm the color actually matches (the
      attribute->channel resolution per target fixture is the part that
      can't be fully proven without real hardware to look at).
- [ ] **Group master fader** on a real group of the 4 pars — confirm moving
      the slider dims them together, and that it behaves like `GROUP N @
      value` (an immediate set, not a live-riding scale) as documented in
      DECISIONS.md, not something more than that.

## Phase 6 — Cuelists, playback

- [ ] **Record and step through a real Standard cuelist on the 4 pars.**
      Select fixtures via the command line or the Cuelists tab's picker, set
      a look, `RECORD CUE 1`, change the look, `RECORD CUE 2`, add a playback
      from the Cuelists tab, and confirm `NEXT`/GO actually crossfades the
      real par output over the recorded fade time — the crossfade math is
      unit-tested, but real DMX output on the wire over that fade is not.
- [ ] **Two playbacks on the same fixtures, one an Override cuelist.**
      Confirm the Override playback's LTP channels always win over the
      Standard one regardless of which was triggered more recently, matching
      `OVERRIDE_PRIORITY_BASE`'s intent — this is unit-tested against the
      merge logic in isolation but not against two real concurrent
      `PlaybackRuntime`s ticking on the live engine thread.
- [ ] **Chase auto-advance and tap-tempo on the real rig.** Create a Chase
      cuelist across the 4 pars, add a playback, and confirm the BPM-driven
      auto-advance actually paces visibly correct on real hardware, and that
      `PlaybackBar`'s tap-tempo button converges to a sensible BPM after a
      few taps (its averaging window and click-timestamp math haven't been
      exercised outside this browser-preview environment's `performance.now`).
- [ ] **Submaster fader behavior on a real fixture.** Confirm a Submaster
      playback's fader directly scales its recorded (tracked) look on real
      output, with no GO-stepping, as intended.

## Phase 6 follow-up — Show persistence

- [ ] **Groups/presets/cuelists survive Save → quit → Open.** In the built
      app, record a group, a Colour preset, and a cuelist with a cue that
      references that preset. Save, quit, relaunch, Open the file, and confirm
      all three are back, and that updating the preset still changes the cue
      (the link survived, it wasn't copied).
- [ ] **Session restore on launch.** Make an unsaved change, wait for an
      autosave (~60s), quit without saving, relaunch: the app should reopen
      that work marked unsaved. Then Save, quit, relaunch: it should reopen
      the saved file, not an older autosave.
