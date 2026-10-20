# MCP server

LumenDMX has a built-in [MCP](https://modelcontextprotocol.io) server, so an
AI agent can patch fixtures and build node graphs. It replaces the old
built-in AI chat. The agent works through the same show operations as the
UI (`src-tauri/src/show/`):
- changes appear live in the open app;
- they mark the show unsaved;
- each is one undo step.

## Connecting

The server is on by default at `http://127.0.0.1:7410/mcp` (streamable
HTTP). Setup → "AI agents (MCP)" shows the status, lets you turn it off or
change the port, and has a ready-to-copy command for Claude Code:

```bash
claude mcp add --transport http lumendmx http://127.0.0.1:7410/mcp
```

Settings are per machine (`mcp.json` in the app data folder), not per show.

## Security

It listens on 127.0.0.1 only, and refuses requests whose `Host` header isn't
localhost. That protects against DNS rebinding: a web page can't drive your
lights through your browser. There is no authentication beyond that: any
program on this machine can connect while the server is on.

## Tools

| Tool | Does |
|------|------|
| `get_show` | Name, universes and controllers, counts, groups, graph issues, BPM |
| `list_fixture_types` | Library types, modes, channel attributes |
| `list_fixtures` / `add_fixtures` / `update_fixture` / `remove_fixtures` | The patch (auto addresses and numbers when omitted) |
| `add_universe` | A universe, optionally with its controller |
| `create_group` / `update_group` / `delete_group` | Ordered fixture groups |
| `list_node_types` | Every node: description, sockets, params (the registry) |
| `get_graph` | Nodes with their sockets and issues, and edges |
| `add_node` / `remove_nodes` / `connect` / `disconnect` / `set_params` | Graph edits |
| `replace_graph` | Write a whole graph in one undo step |
| `get_output` | Each fixture's live DMX values per attribute |
| `get_tempo` / `set_bpm` | Show tempo |
| `set_control` | Move a playback-bar Fader or press a Button |
| `undo` / `redo` | Show history |

The server's instructions (`INSTRUCTIONS` in `src-tauri/src/mcp/tools.rs`)
explain the graph model to the agent: socket types, how Output targets and
priorities work, per-fixture evaluation, and the usual patterns (Fan → LFO,
Fixture Index → Gradient, Counter → Switch).

`src-tauri/src/mcp/mod.rs`'s test drives a real server over HTTP: it
patches, groups, wires a graph and checks the DMX output.
