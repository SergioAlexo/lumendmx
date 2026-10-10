# Pioneer Pro DJ Link: feasibility and licensing

Status: research only. Nothing here is implemented, and the notes below come
from prior knowledge of the public reverse-engineering work, not from testing
against hardware. Verify every protocol detail against a real CDJ/XDJ setup
before building on it.

## What it is

Pro DJ Link is the Ethernet protocol Pioneer DJ (now AlphaTheta) players and
mixers use to share state: each player announces itself, broadcasts beats with
the track's BPM and pitch, and reports status (playing, master, track, beat
within the bar). Rekordbox on a laptop can join the same network.

## What we would want

- The **master player's BPM and beat position**, to drive `Tempo::set_bpm`
  and align the beat grid to the actual downbeat. This is better than audio
  beat detection (exact BPM, bar phase, instant response to pitch changes).
- Optionally the on-air player and track metadata, for the AI context in
  Phase 11.

## How it works (as publicly documented)

- UDP on the local network: device announcements on port 50000, beat packets
  on 50001 (broadcast, sent by every playing deck: BPM, pitch, beat 1-4 in the
  bar, next-beat timings), and status packets on 50002 (unicast to devices that
  announced themselves).
- Beat packets are plain broadcast, so a passive listener can read them
  without joining. Status packets, master/sync information and track
  metadata require the listener to announce itself as a device (a "virtual
  CDJ" with a free player number), which takes up one of the (usually four to
  six) device slots on the network.
- The most complete open documentation is Deep Symmetry's "dysentery" protocol
  analysis, and its reference implementation is the `beat-link` Java library.
  Both are community reverse-engineered; Pioneer has not published a spec.
- The player must be on the same wired network as the computer (Ethernet
  through the mixer or a switch). On Windows/macOS that means dealing with
  network interface selection and the DJ gear's link-local addressing.

## Feasibility

Technically feasible in Rust with `std::net::UdpSocket` and `socket2` (already
a dependency): a passive beat listener is a small amount of code (parse the
port-50001 beat packet). A full virtual-CDJ participant is a few hundred lines
more (announce packets, keep-alives, status parsing) but is the part most likely
to be fragile across firmware versions.

Risks:

- No official spec: newer firmware or hardware generations (CDJ-3000, newer
  XDJ models, Rekordbox's own link modes) change packet details. Needs access to
  real devices for each model to be confident.
- Joining as a virtual device can interfere with a DJ's setup if it takes a
  player slot or misbehaves; a passive-only mode avoids this.
- Wired network setup is awkward for a lighting laptop in a booth.

## Licensing

- There is no Pioneer SDK or licence for third-party Pro DJ Link clients. The
  protocol is implemented by the community through interoperability reverse
  engineering. Implementing it clean-room from our own packet captures and the
  public protocol notes is the normal route; we should not copy code from
  projects with incompatible licences.
- `beat-link` (Deep Symmetry) is distributed under the Eclipse Public License
  1.0: fine as a reference for behaviour, but it is Java and not usable as a
  dependency of a Rust/Tauri app. Check the licence of any Rust crate found
  before depending on it (none was verified here).
- Trademarks: "Pioneer DJ", "Pro DJ Link", "CDJ" and "Rekordbox" are
  trademarks; the UI should say "Pro DJ Link compatible" at most, with no
  logos.
- Legal risk is generally considered low for interoperability of this kind,
  but this is not legal advice; a short review before shipping is sensible.

## Recommendation

Do not implement yet. If DJ-booth use makes this a priority:

1. Start with a **passive beat listener** (port 50001 only): BPM, pitch, beat
   in bar, master flag where present. No announcing, no slot usage, no
   metadata. Feed it into `Tempo` as another source next to Audio and MIDI
   clock (`TempoSource::ProDjLink`).
2. Capture traffic from the user's own gear to confirm packet layouts before
   writing the parser; keep the capture files as test fixtures.
3. Only then consider announcing as a virtual device for track metadata.

Until then, the practical routes for DJ sync are audio beat detection from the
mixer's booth/record output and MIDI clock (Traktor, Serato, Rekordbox and
Ableton can all send it).
