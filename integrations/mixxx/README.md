# Mixxx → LumenDMX beat sync

A Mixxx controller mapping that sends MIDI clock following the deck you're
playing, so LumenDMX's tempo (and beat-synced effects and chases) track
Mixxx: tempo changes, jog nudges and pitch bends included. It uses macOS's
built-in IAC Driver as a virtual MIDI bus; nothing to install.

## Setup

1. **Enable the IAC bus.** Open Audio MIDI Setup → Window → Show MIDI Studio,
   double-click **IAC Driver**, tick **Device is online**.
2. **Install the mapping.** Copy `LumenDMX-Clock.midi.xml` and
   `LumenDMX-Clock.js` into Mixxx's controllers folder:
   - Mixxx from mixxx.org: `~/Library/Containers/org.mixxx.mixxx/Data/Library/Application Support/Mixxx/controllers/`
   - Windows: `%LOCALAPPDATA%\Mixxx\controllers\`; Linux: `~/.mixxx/controllers/`
3. **Load it in Mixxx.** Restart Mixxx, then Preferences → Controllers →
   **IAC Driver Bus 1** → Load Mapping → **LumenDMX MIDI Clock**, tick
   **Enabled**, Apply. Your DDJ (or other) controller keeps its own mapping.
4. **In LumenDMX**, Setup tab: Drive the BPM from **MIDI clock**, Clock /
   timecode port **IAC Driver Bus 1**. The status line shows the received
   clock BPM.

## Which deck it follows

The sync leader if a playing deck is one (Sync on, leader set), otherwise the
playing deck with the highest channel fader. When the source changes, or you
seek or scratch backwards, it waits for that deck's next beat and sends MIDI
Start there. It also sends Start every 4 beats, which snaps LumenDMX's beat
grid back onto Mixxx's beat. With nothing playing it sends Stop, and LumenDMX
keeps the last tempo.

Mixxx only runs scripts about once per audio buffer, so pulses arrive up to
one buffer late. LumenDMX fits a line through four beats of pulses, which
holds the tempo to about ±0.1 BPM at 128.
