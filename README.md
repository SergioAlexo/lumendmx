# LumenDMX

DMX512 lighting controller for live electronic music. You build the show as
a **node graph**, in the style of Blender or Unity: data sources (the beat,
audio, MIDI, playback-bar faders) flow through effect, colour and switching
nodes into fixtures. An AI agent can do the same through the built-in MCP
server. Hardware output runs in its own loop and never glitches, whatever
the UI is doing.

## The app

Three tabs, plus the playback bar at the bottom:

- **Setup**: universes and the DMX controller each one outputs through
  (with detection of USB interfaces and Art-Net nodes); the tempo source,
  audio and MIDI inputs; the MCP server; app updates.
- **Fixtures**: the patch, listed per universe/controller. The **Patch** view
  edits fixtures, **Values** shows what each fixture is outputting, **Raw
  DMX** shows each universe's 512 channels. Groups (ordered fixture sets)
  sit on the right.
- **Nodes**: the graph editor. Shift+A or right-click adds nodes. Drag
  between typed sockets to link them, or drag a link onto empty canvas to
  add a connected node. Every node shows its live values.
- **Playback bar**: one fader or flash/toggle button per Fader/Button node.

See [docs/ENGINE.md](docs/ENGINE.md) for how it works, [docs/NODES.md](docs/NODES.md)
for every node type, and [docs/MCP.md](docs/MCP.md) for connecting an AI agent.

## Stack

- **Frontend:** React 19 + Vite + TypeScript + Tailwind CSS v4, Zustand for
  state, React Flow (`@xyflow/react`) for the node editor.
- **Desktop shell:** Tauri v2 (Rust). Rust owns the show (patch, groups,
  graph, undo, files) and the engine. The UI is a view over it.
- **Output:** FTDI Open DMX (host-generated break), Enttec DMX USB Pro,
  Art-Net, sACN, one driver per universe, each on its own thread
  (`src-tauri/src/output/`).
- **Engine:** compiles the node graph and evaluates it at 44Hz, per fixture
  where needed, merging outputs HTP/LTP (`src-tauri/src/graph/`,
  `src-tauri/src/engine/`).
- **Fixtures:** JSON definitions in `src/fixtures/library/` plus imports
  (Open Fixture Library, QLC+ `.qxf`, or the built-in editor).
- **Timing:** tap/manual BPM, audio beat detection, MIDI clock, Mixxx
  (`docs/mixxx/`), MTC and LTC timecode (`src-tauri/src/timing/`).
- **MCP:** the official Rust SDK (`rmcp`), streamable HTTP on 127.0.0.1
  (`src-tauri/src/mcp/`).
- **Design system:** ported from
  [sergioalexo/design-system](https://github.com/sergioalexo/design-system):
  HSL token pairs mapped through Tailwind v4 `@theme inline` in
  `src/index.css`, CVA-based primitives in `src/components/ui/`. Dark only.

## Running it

```bash
npm install
npm run tauri dev
```

Requires the Rust toolchain (`rustc`/`cargo`). `npm run dev` alone runs just
the Vite frontend in a browser; native calls fail gracefully outside Tauri.

Tests: `npm test` (frontend) and `cargo test` in `src-tauri/` (engine, graph,
show, MCP end to end). After changing a node type, regenerate the node
reference with `UPDATE_DOCS=1 cargo test nodes_doc`.

## Known limitations

- Show files from the pre-graph development builds (format 8 and older)
  can't be opened.
- Custom fixture types live in the app's local storage, not in the show
  file, so a show using them needs them imported on another machine too.
- The MCP server has no authentication beyond being localhost-only.
- Fader/Button nodes can't be bound to MIDI directly. Use a MIDI CC/Note node
  (with Learn) instead.
