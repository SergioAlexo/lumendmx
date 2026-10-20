# Rig configuration

## DMX interface

- Port: **COM9**
- Genuine FTDI USB-to-DMX widget (Device Manager: Driver Provider "FTDI",
  `ftser2k.sys`), dedicated DMX/XLR output -- not a bare TTL adapter.
- Requires the real-hardware-break output path in `src-tauri/src/output/ftdi.rs`
  (`set_break`/`clear_break`, 110us break / 16us MAB, 30Hz pacing, time-critical
  thread priority). Do not switch this back to a baud-rate-faked break -- that was
  tried and caused intermittent flicker on this exact adapter.

## Fixtures

4x generic "6-color LED Lamp" par, run in **10-channel mode**.

**Important:** this fixture stores two separate DMX addresses -- `d0xx` for its
6-channel mode and `A0xx` for its 10-channel mode. Since these run in 10-channel
mode, the **A** address is the one that matters. Set it per fixture via the unit's
own display: MENU -> UP/DOWN to the `A0xx` screen -> UP/DOWN to set the value ->
ENTER to confirm.

| Fixture | A address |
|---|---|
| 1 | 001 |
| 2 | 011 |
| 3 | 021 |
| 4 | 031 |

### 10-channel mode map (confirmed from the fixture's printed manual)

| Channel | Function |
|---|---|
| 1 | Total dimmer (R,G,B,W,A,U master dim) |
| 2 | Red |
| 3 | Green |
| 4 | Blue |
| 5 | White |
| 6 | Amber |
| 7 | UV (labeled "U"/"Purple" on the fixture) |
| 8 | Total strobe/flash speed (0 = off) |
| 9 | Function Select -- **keep at 000-050** to stay in plain "CH1-CH8 control" (manual DMX) mode. Higher values switch the fixture into built-in auto-chase/color-jump/sound-reactive programs and it will stop obeying CH1-8. |
| 10 | Auto-program speed (irrelevant while CH9 is in the 000-050 manual range) |

## App fixture library / patching

The channel map above is captured as a reusable fixture definition at
[src/fixtures/library/generic-6color-par.json](src/fixtures/library/generic-6color-par.json)
(id: `generic-6color-par`, "10-Channel" mode). See
[src/fixtures/library/README.md](src/fixtures/library/README.md) for the JSON
schema if you want to add another fixture model -- one file per fixture, no code
changes needed, and community-contributed fixtures can either live in that folder
(PR) or be loaded at runtime via the "Import fixture JSON" button.

Your actual rig (which physical fixture is patched to which address) is *not*
stored in this repo -- it's data, not code. It lives in the Fixture Patch panel's
"patch" list (persisted in the app's local storage). To recreate this rig after a
fresh install/profile reset, add 4 instances of `generic-6color-par` / "10-Channel"
mode at addresses 1, 11, 21, 31, matching the A-address table above.
