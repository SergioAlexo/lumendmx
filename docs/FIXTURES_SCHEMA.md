# Fixture schema, importers, and patch

See `src/fixtures/library/README.md` for the schema itself (fields, channel
types, the optional 16-bit/capabilities/multi-cell/physical additions). This
doc covers the importers and the patch model that sit around it.

## Importers

- `src/lib/fixtures/importers/ofl.ts` — [Open Fixture Library](https://open-fixture-library.org/)
  JSON. Built against OFL's documented fixture format and a real fixture
  (Cameo Auro Spot 300) fetched from the OFL GitHub repo. Handles fine
  (16-bit) channels via `fineChannelAliases`, wheel slot colors/images via the
  `wheels` object, and the full capability-type vocabulary (Pan/Tilt,
  ColorIntensity, ShutterStrobe, WheelSlot, Prism, Focus/Zoom/Iris/Frost,
  Speed, Effect, Maintenance, NoFunction). Matrix/pixel fixtures
  (`matrix`/`templateChannels`) are rejected with a clear error; they
  aren't supported.
- `src/lib/fixtures/importers/qlcplus.ts` — QLC+ `.qxf` (XML, via
  `DOMParser`). QLC+'s `Group` is a broad category (Colour/Beam/Gobo/Pan/
  Tilt/Shutter/Speed/Effect/Maintenance/Nothing) with no structured color
  name or wheel-slot data, so a specific color (red/green/blue/...) is
  inferred from the channel's own name when the group is generically
  "Colour" — an honest limitation of the source format. A `Capability`'s
  `Preset` attribute (e.g. `ShutterOpen`, `PanMSB`) refines the type further
  when present. Two consecutive `<Channel>` entries in a `<Mode>` sharing a
  `Group` name with `Byte="0"` then `Byte="1"` are recognized as a 16-bit
  coarse/fine pair, by QLC+ convention.

Both are unit-tested against realistic (not toy) fixture data — see their
`.test.ts` files. Neither ships "the full OFL/QLC+ catalog" bundled; see
DECISIONS.md for why.

## Patch model

`PatchedFixture` (`src-tauri/src/showfile/schema.rs`; the patch is owned by
Rust's `ShowService`, see docs/ENGINE.md) carries:

- `fixtureId` / `modeIndex`: the library type and mode.
- `universe` and `address`: one universe per controller, so this also says
  which controller the fixture is on.
- `fixtureNumber`: unique across the show. Groups and the graph's Output
  nodes (`"fixtures": "1-8, 12"`) refer to fixtures by it; renumbering a
  fixture updates the groups it's in.
- `invertPan`/`invertTilt`/`swapPanTilt`: applied when an Output node writes
  a position.

Rust resolves each fixture's attributes to channels from the library the
frontend pushes (`show/resolve.rs`): `type` → attribute, `fineOffset` → the
16-bit LSB, `cell` → per-cell channel sets (an Output can light cells
separately), `defaultValue` → the channel's rest value, `capabilities` →
strobe ranges and colour-wheel slots.

The Fixtures tab's Patch view adds fixtures (several at once, with
auto-picked addresses and numbers), edits them inline, and refuses address
overlaps naming the conflicting fixture.
