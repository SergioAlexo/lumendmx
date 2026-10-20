# The lighting engine

What goes out on the wire is decided by the show's **node graph**, evaluated
by the Rust engine at a fixed 44Hz, independent of the UI. This document
covers how the show, the graph and the engine fit together. The node types
themselves are listed in [NODES.md](NODES.md); the MCP server in
[MCP.md](MCP.md).

## Rust owns the show

`src-tauri/src/show/` holds the open show (`ShowService`): universes, the
patch, groups, the graph, undo/redo, and file open/save/autosave. The UI and
MCP clients both edit it through the same operations, so they share one
undo history and one "unsaved changes" state. After every change the
service:

1. re-resolves the patch against the fixture library (`show/resolve.rs`):
   every fixture's attributes → DMX channels, which channels are HTP
   (dimmers), and each channel's rest value (`defaultValue`, e.g. a function
   channel's "obey DMX" value);
2. recompiles the graph (`graph/compile.rs`) against that patch and the
   groups;
3. hands both to the engine, and emits `show://changed` so the UI refetches.

The fixture library (bundled JSON plus user imports) stays in the frontend,
which pushes the whole set to Rust whenever it changes
(`show_set_library`). Rust only reads the parts resolution needs.

Side effects outside the show (engine, output drivers, events) go through
the `ShowSink` trait, so the service is unit-tested without Tauri.

## Universes and controllers

One universe is one controller output (`UniverseConfig.driver`: FTDI Open
DMX, Enttec DMX USB Pro, Art-Net, sACN, or none). Fixtures are patched into
a universe at an address, so fixtures on different controllers live in
different universes. Setup's "Detect controllers"
(`output_detect_controllers`) lists FTDI-chip USB widgets and Art-Net nodes
answering a poll, each ready to add as a universe.

## The graph

A graph is nodes (kind, position, params) and edges (output socket → input
socket). Sockets are typed: number, trigger, colour, position. Conversions
are limited to ones that make sense (`SocketType::feeds`). Every node type,
with its sockets and params, is defined once in `graph/registry.rs`. The
editor UI and MCP clients both render and describe nodes from it.

### Compiling

`compile` turns a graph into a `GraphPlan`:

- links are checked (sockets exist, types are compatible), and nodes on a
  loop are reported and cut out, so one bad link doesn't stop the rest;
- params are parsed into typed configs once, not every tick;
- each node is marked **varying** if it depends on which fixture is being
  lit (Fixture Index, Fan, Random, or anything fed by them);
- each Fixture Output's targets (a group, or a list like `1-8, 12`) are
  resolved to `TargetItem`s: dimmer channels, colour channel sets (one per
  cell when "each cell separately" is on), pan/tilt/strobe/extra
  attributes with their fine channels and capabilities.

Problems are reported per node (`GraphIssue`) and shown on the node.

### Evaluating

Each tick (`graph/eval.rs`):

1. non-varying nodes are evaluated once, in dependency order;
2. for each Output, for each target in order, its varying dependencies are
   evaluated with that target's context (index, count, fixture identity).
   That's like a shader running per fixture: chases and fans come from
   Fixture Index / Fan feeding phases, not from a separate effects engine;
3. the Output's connected inputs are written to its targets
   (`graph/write.rs`): colour mixing for RGB(W)/CMY/colour wheels, a
   virtual dimmer for fixtures without one, 16-bit pan/tilt with
   invert/swap, strobe through the channel's capability ranges.

Stateful nodes (Smooth, Envelope, Toggle, Counter, Random) keep state by
node id and fixture, so recompiling after an edit doesn't reset them.

### What the graph reads

The tick passes the graph the show clock (`engine/tempo.rs`: a BPM and an
integrated beat position, so BPM changes don't jump phases), audio band
levels, MIDI controller/note state from the MIDI port chosen in Setup,
timecode, and the playback bar's control values (one per Fader/Button
node, set live and saved with the show, not undoable).

## Merge

Each Output node writes its own layer; the patch's rest values sit in a
lowest-priority layer. `engine/merge.rs` merges them per universe:
intensity (dimmer) channels are HTP (highest value wins). Everything else is
LTP by priority (the Output's "priority" param). The grand master then
scales intensity channels, and the merged frame goes to the universe's
output driver. Every 50ms the engine also emits each universe's frame
(`engine://universe-frame`, for the Fixtures tab's Values and Raw DMX views)
and each node's current output values (`graph://values`, for the editor's
live previews).
