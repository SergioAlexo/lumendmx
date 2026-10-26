# Node reference

Generated from `src-tauri/src/graph/registry.rs` by `cargo test` (run `UPDATE_DOCS=1 cargo test nodes_doc` after changing a node). Values are 0-1 unless noted. An unconnected input takes its value from a param with the input's id, else the default shown. Every node also has an `enabled` input (trigger, default on, not listed below): while it's off, everything the node feeds behaves as if the link wasn't there.

## Input

### Number (`number`)

A fixed number.

Outputs:
- `value` Value (number)

Params:
- `value` Value: number -1000 to 1000, default 1

### Color (`color`)

A fixed colour.

Outputs:
- `color` Color (color)

Params:
- `color` Color: colour, default `#ffffff`

### Time (`time`)

The show clock. Beat and Bar run 0-1 over each beat and each 4-beat bar at the show BPM; On beat fires once per beat.

Outputs:
- `beat` Beat (number)
- `bar` Bar (number)
- `onBeat` On beat (trigger)
- `bpm` BPM (number)
- `seconds` Seconds (number)

### LFO (`lfo`)

A repeating wave from 0 to 1, at a rate in cycles per beat (follows the BPM) or per second. Phase shifts it, in cycles: feed a Fan into it to make the wave travel across fixtures. Wave Custom plays the drawn Curve: points "x:y" (x across one cycle, y the value, both 0-1), joined by Between points.

Inputs:
- `phase` Phase (number, default 0)

Outputs:
- `value` Value (number)

Params:
- `waveform` Wave: sine / square / saw / ramp / random / step / pulse / custom, default `sine`
- `rate` Rate: number 0 to 64, default 1
- `unit` Per: beats / hz, default `beats`
- `width` Width %: number 1 to 100, default 50
- `curve` Curve: points `x:y`, comma-separated, default `0:0, 0.5:1, 1:0`
- `curveMode` Between points: linear / smooth / step, default `linear`

### Audio (`audio`)

Live levels of the audio input (turn on audio analysis in Setup), each 0-1.

Outputs:
- `low` Low (number)
- `mid` Mid (number)
- `high` High (number)
- `loudness` Loudness (number)

Params:
- `gain` Gain: number 0 to 4, default 1

### MIDI CC (`midiCc`)

A MIDI controller (knob, fader) on the controller ports chosen in Setup, 0-1. Channel 0 listens on every channel.

Outputs:
- `value` Value (number)

Params:
- `channel` Channel: number 0 to 16, default 0
- `controller` CC: number 0 to 127, default 1

### MIDI Note (`midiNote`)

A MIDI note (pad, key) on the controller ports chosen in Setup: Gate is on while it's held, Velocity is how hard it was hit (0-1). Channel 0 listens on every channel.

Outputs:
- `gate` Gate (trigger)
- `velocity` Velocity (number)

Params:
- `channel` Channel: number 0 to 16, default 0
- `note` Note: number 0 to 127, default 36

### Fader (`fader`)

A fader on the playback bar, 0-1.

Outputs:
- `value` Value (number)

Params:
- `label` Label: text: Shown on the playback bar, default `Fader`

### Button (`button`)

A button on the playback bar: on while held (flash) or on/off per press (toggle).

Outputs:
- `gate` Gate (trigger)

Params:
- `label` Label: text: Shown on the playback bar, default `Button`
- `mode` Mode: flash / toggle, default `flash`

### Timecode (`timecode`)

The incoming timecode (MTC or LTC, chosen in Setup): position in seconds, and whether it's running.

Outputs:
- `seconds` Seconds (number)
- `running` Running (trigger)

### Tap Tempo (`tapTempo`)

Taps the show tempo each time Tap turns on, like the TAP button: wire a MIDI Note or Button into it. Averages the last 4 taps (a gap over 2 seconds starts again) and lines the beat up with each tap. An external tempo source (audio, MIDI clock, Mixxx) overrides it. Tap can't depend on the fixture.

Inputs:
- `tap` Tap (trigger, default 0)

## Fixtures

### Fixture Index (`fixtureIndex`)

Which fixture is being lit: its place in the Output's target list (Index from 0, Position 0-1 from first to last), and how many there are. Makes every fixture behave differently.

Outputs:
- `index` Index (number)
- `position` Position (number)
- `count` Count (number)

### Fan (`fan`)

A per-fixture phase offset: Spread 1 spaces the fixtures evenly over one whole cycle. Feed it into an LFO or Shape's Phase. Groups/Blocks/Wings split the fan like a console's effect engine.

Inputs:
- `spread` Spread (number, default 1)

Outputs:
- `phase` Phase (number)

Params:
- `groups` Groups: number 0 to 64, default 0
- `blocks` Blocks: number 0 to 64, default 0
- `wings` Wings: number 0 to 16, default 0

## Math

### Math (`math`)

Combines two numbers. Invert is 1 - A; Abs ignores B.

Inputs:
- `a` A (number, default 0)
- `b` B (number, default 1)

Outputs:
- `value` Value (number)

Params:
- `op` Operation: add / subtract / multiply / divide / min / max / power / modulo / invert / abs, default `multiply`
- `clamp` Clamp 0-1: on/off, default false

### Map Range (`mapRange`)

Rescales a value from one range to another.

Inputs:
- `value` Value (number, default 0)

Outputs:
- `value` Value (number)

Params:
- `fromMin` From min: number -1000 to 1000, default 0
- `fromMax` From max: number -1000 to 1000, default 1
- `toMin` To min: number -1000 to 1000, default 0
- `toMax` To max: number -1000 to 1000, default 1
- `clamp` Clamp: on/off, default true

### Compare (`compare`)

On when A is greater than / less than / equal to B.

Inputs:
- `a` A (number, default 0)
- `b` B (number, default 0.5)

Outputs:
- `value` Result (trigger)

Params:
- `op` Test: greater / less / equal, default `greater`

### Random (`random`)

A random value 0-1, different for every fixture, picking a new one each time Trigger turns on.

Inputs:
- `trigger` Trigger (trigger, default 0)

Outputs:
- `value` Value (number)

Params:
- `seed` Seed: number 0 to 9999, default 0

### Smooth (`smooth`)

Follows its input, taking Attack ms to rise and Release ms to fall.

Inputs:
- `value` Value (number, default 0)

Outputs:
- `value` Value (number)

Params:
- `attack` Attack ms: number 0 to 10000, default 50
- `release` Release ms: number 0 to 10000, default 300

### Envelope (`envelope`)

When Trigger turns on, rises to 1 over Attack ms and falls back to 0 over Decay ms. A flash on every beat or hit.

Inputs:
- `trigger` Trigger (trigger, default 0)

Outputs:
- `value` Value (number)

Params:
- `attack` Attack ms: number 0 to 10000, default 10
- `decay` Decay ms: number 0 to 10000, default 250

### Toggle (`toggle`)

Flips on/off each time Trigger turns on.

Inputs:
- `trigger` Trigger (trigger, default 0)

Outputs:
- `value` State (trigger)

### Any (`any`)

On while any of its inputs is on: a Button and a MIDI pad driving the same thing. There's always a free input at the bottom; connect to it and another appears.

Inputs:
- `in0` 1 (trigger)
- `in1` 2 (trigger)

Outputs:
- `gate` Gate (trigger)

### Counter (`counter`)

Counts 0, 1, 2 ... Steps-1 and round again, one step each time Trigger turns on; Reset goes back to 0. Feed Index into a Switch to step through looks.

Inputs:
- `trigger` Trigger (trigger, default 0)
- `reset` Reset (trigger, default 0)

Outputs:
- `index` Index (number)
- `position` Position (number)

Params:
- `steps` Steps: number 1 to 64, default 4

## Colour

### RGB (`rgb`)

Builds a colour from red, green and blue (0-1).

Inputs:
- `r` Red (number, default 1)
- `g` Green (number, default 1)
- `b` Blue (number, default 1)

Outputs:
- `color` Color (color)

### HSV (`hsv`)

Builds a colour from hue (0-1 around the colour wheel), saturation and value.

Inputs:
- `h` Hue (number, default 0)
- `s` Saturation (number, default 1)
- `v` Value (number, default 1)

Outputs:
- `color` Color (color)

### Mix Color (`mixColor`)

Blends from colour A (Amount 0) to B (Amount 1).

Inputs:
- `a` A (color, default `#ff0000`)
- `b` B (color, default `#0000ff`)
- `t` Amount (number, default 0.5)

Outputs:
- `color` Color (color)

### Gradient (`gradient`)

Picks a colour along evenly spaced stops; Position 0 is the first stop, 1 the last.

Inputs:
- `t` Position (number, default 0)

Outputs:
- `color` Color (color)

Params:
- `stops` Stops: text: Colours, comma-separated: #ff0000, #00ff00, default `#ff0000, #ffff00, #00ff00, #00ffff, #0000ff, #ff00ff`
- `wrap` Wrap around: on/off, default true

## Position

### Shape (`shape`)

Moves pan/tilt around a shape at a rate per beat or per second, centred on Center with the given Size. Phase shifts it in cycles, like an LFO.

Inputs:
- `phase` Phase (number, default 0)
- `size` Size (number, default 0.25)
- `center` Center (position, default 0.5)

Outputs:
- `position` Position (position)

Params:
- `shape` Shape: circle / figure8 / ballyhoo / panSaw / tiltSaw, default `circle`
- `rate` Rate: number 0 to 64, default 1
- `unit` Per: beats / hz, default `beats`

### Pan/Tilt (`panTilt`)

Builds a position from pan and tilt (0-1, 0.5 is centre).

Inputs:
- `pan` Pan (number, default 0.5)
- `tilt` Tilt (number, default 0.5)

Outputs:
- `position` Position (position)

## Switching

### Switch (`switch`)

Passes one of its inputs through: Index 0 picks the first, 1 the second ... Drive Index from a Counter, MIDI or a Fader to change looks live.

Inputs:
- `index` Index (number, default 0)
- `in0` 0 (color, default `#000000`)
- `in1` 1 (color, default `#000000`)

Outputs:
- `value` Value (color)

Params:
- `type` Type: number / color / position, default `color`
- `count` Inputs: number 2 to 8, default 2

### Mix (`mix`)

Crossfades from A (Amount 0) to B (Amount 1).

Inputs:
- `a` A (number, default 0)
- `b` B (number, default 0)
- `t` Amount (number, default 0.5)

Outputs:
- `value` Value (number)

Params:
- `type` Type: number / color / position, default `number`

## Output

### Fixture Output (`output`)

Sends values to fixtures: a group, or fixture numbers like "1-8, 12". Only connected inputs are sent. Inputs are evaluated once per fixture, so Fixture Index and Fan vary them across the targets. Where two outputs drive the same fixture, intensity takes the highest and everything else the higher Priority.

Inputs:
- `intensity` Intensity (number)
- `color` Color (color)
- `position` Position (position)
- `strobe` Strobe (number)

Params:
- `group` Group: group id (0 = none), default 0
- `fixtures` Fixtures: text: Fixture numbers when no group: 1-8, 12, default (empty)
- `cells` Each cell separately: on/off, default false
- `priority` Priority: number 0 to 100, default 0
- `attributes` More attributes: text: Extra inputs, comma-separated: zoom, gobo, white, default (empty)

## Layout

### Frame (`frame`)

A labelled box to group nodes visually; it does nothing to the output. Dragging it moves the nodes inside it. Select nodes and press Cmd/Ctrl+J to frame them.

Params:
- `label` Label: text: Shown at the top, default `Frame`
- `color` Color: gray / red / orange / yellow / green / blue / purple, default `gray`
- `width` Width: number 100 to 10000, default 400
- `height` Height: number 80 to 10000, default 300
