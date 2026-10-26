//! The MCP tools: the same show operations the UI uses (`ShowService`,
//! `EngineManager`), so edits made by an agent are undoable, mark the show
//! unsaved, and appear in the open editor immediately.

use std::collections::BTreeMap;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::engine::manager::EngineManager;
use crate::graph::model::{Edge, Graph, NodePosition, ParamValue};
use crate::graph::registry::{node_types, NodeKind};
use crate::output::config::DriverConfig;
use crate::show::service::{AddFixtures, FixtureUpdate, ShowService};

const INSTRUCTIONS: &str = r#"LumenDMX is a DMX lighting controller. You can patch fixtures and build the node graph that drives them; the user sees every change live in the app, and can undo it.

Show structure:
- Universes: one per DMX controller (USB interface or Art-Net node). Fixtures are patched into a universe at a DMX address.
- Fixtures have a unique fixture number. Groups are ordered lists of fixture numbers; the order is the order effects spread across them.
- The node graph decides all output. Nothing lights up unless a Fixture Output node drives it.

How the graph works:
- Values are 0-1. Socket types: number, trigger (on above 0.5), color (RGB 0-1), position (pan, tilt 0-1, 0.5 = centre). Numbers feed colours (grey) and positions; a colour feeds a number (its brightness); positions only feed positions.
- A Fixture Output node (kind "output") targets a group (param "group" = group id) or a fixture list (param "fixtures" = "1-8, 12"). Only its connected inputs are sent: intensity, color, position, strobe, plus extra attributes listed in param "attributes" (e.g. "zoom, gobo"). Where outputs overlap, intensity takes the highest value; other attributes come from the output with the higher "priority".
- Every node upstream of an Output is evaluated once per target fixture. "fixtureIndex" (index, position 0-1, count) and "fan" (a per-fixture phase offset) make fixtures differ: fan.phase -> lfo.phase gives a wave travelling across a group; fixtureIndex.position -> gradient.t spreads a rainbow.
- "time" gives the show clock (beat, bar, onBeat trigger, bpm). "lfo" and "shape" run at a rate in cycles per beat (unit "beats", follows the BPM) or per second (unit "hz").
- Looks are switched live with "switch" (index input picks one of N inputs) driven by a "counter" or a "fader"; "mix" crossfades two values. "fader" and "button" nodes appear as controls on the playback bar, and are what MIDI controllers drive: set a button's params "midi" ("note" or "cc"), "midiChannel" (0 = any) and "midiNumber" to press it from a pad, or a fader's to move it from a knob (CC). An input takes one link; to drive one from several triggers (two buttons), connect them to an "any" node's inputs "in0", "in1", ... (it always has one free input after the last connected one) and its "gate" to the input.
- Every node has an "enabled" trigger input (default on). While it's off, everything the node feeds behaves as if the link were unplugged (inputs fall back to their param or default, and an Output doesn't send that attribute); a disabled Output sends nothing. Feed it a "button" to bring parts of a look in and out.
- "frame" nodes are labelled boxes (params label, color, width, height, collapsed) that only group nodes visually; they have no sockets of their own and links always go to the real nodes. A frame's position is its top-left corner; the nodes inside its width x height are its contents. Collapsed, it hides them and shows the sockets they expose: a node inside sets its "exposed" param to JSON from "in:<input id>" / "out:<output id>" to the name shown on the frame ("" for the socket's own), e.g. {"in:rate": "Speed"}. Expose a socket before linking to it across a frame.
- An unconnected input takes its value from a param with the input's socket id (e.g. params {"t": 0.5} on a mix node), else its default.

Workflow: call list_node_types once for the exact node kinds, sockets and params; get_show and list_fixtures to see the rig; get_graph before editing; list_node_presets for the user's saved building blocks. Check get_graph for per-node issues and get_output to confirm what fixtures are actually doing.

Building:
- Add to the user's graph with add_node, connect and set_params. Use replace_graph only on an empty graph or when the user asks to start over: it replaces everything they built.
- Before building something from scratch, check whether a preset does it; spawn_node_preset places a copy (one undo step) and returns the new nodes to wire up. Save a preset (save_node_preset) only when the user asks.
- Group each piece of work in a labelled frame named for what it does ("Rainbow chase", "Strobe on pad"). Place it in free space: from get_graph, find the lowest point of the existing nodes and frames (a frame covers position to position + width/height; count ~300 px for a node) and start ~100 px below it, at x ~ 0. Inside, lay nodes out inputs on the left, outputs on the right (~250 px apart horizontally, ~150 px vertically). Then add the frame: position 30 px left of and 60 px above the top-left node, width and height covering the nodes plus ~30 px padding (~300 px for the last column, ~250 px for the last row).
- To make a piece reusable like a node of its own, expose its controls (set "exposed" on the nodes inside) and set the frame's "collapsed" to true."#;

type ToolResult = Result<String, String>;

fn json(value: impl Serialize) -> ToolResult {
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FixtureNumbers {
    /// Fixture numbers.
    pub numbers: Vec<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateFixtureArgs {
    /// The fixture's current number.
    pub number: u32,
    pub update: FixtureUpdate,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateGroupArgs {
    pub name: String,
    /// Fixture numbers in fan order.
    pub fixture_numbers: Vec<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateGroupArgs {
    pub id: u32,
    #[serde(default)]
    pub name: Option<String>,
    /// Replaces the members, in fan order.
    #[serde(default)]
    pub fixture_numbers: Option<Vec<u32>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IdArg {
    pub id: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddUniverseArgs {
    #[serde(default)]
    pub name: Option<String>,
    /// The controller; omit for none (fixtures can still be patched).
    #[serde(default)]
    pub driver: Option<DriverConfig>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AddNodeArgs {
    pub kind: NodeKind,
    /// Canvas position in pixels.
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    /// Param values by id (see list_node_types), including values for
    /// unconnected inputs under the input's socket id.
    #[serde(default)]
    pub params: BTreeMap<String, ParamValue>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NodeIds {
    pub ids: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SpawnPresetArgs {
    /// A preset id from list_node_presets.
    pub id: String,
    /// Where its top-left node goes, in canvas pixels.
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SavePresetArgs {
    pub name: String,
    /// The nodes to save; a frame among them brings the nodes inside it.
    pub ids: Vec<String>,
}

/// A preset as list_node_presets shows it: what's in it, not every param.
#[derive(Serialize)]
struct PresetSummary {
    id: String,
    name: String,
    /// Kind and label of each node, e.g. "lfo", "button: Strobe".
    nodes: Vec<String>,
    links: usize,
}

fn preset_summary(p: &crate::show::presets::NodePreset) -> PresetSummary {
    let nodes = p
        .nodes
        .iter()
        .map(|n| {
            let kind = serde_json::to_value(n.kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
            match n.params.get("label").and_then(ParamValue::as_text) {
                Some(label) => format!("{kind}: {label}"),
                None => kind,
            }
        })
        .collect();
    PresetSummary { id: p.id.clone(), name: p.name.clone(), nodes, links: p.edges.len() }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DisconnectArgs {
    pub to_node: String,
    pub to_socket: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetParamsArgs {
    pub id: String,
    /// Param values by id; null resets a param to its default.
    pub params: BTreeMap<String, Option<ParamValue>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReplaceGraphArgs {
    pub graph: Graph,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BpmArg {
    pub bpm: f32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ControlArgs {
    /// A Fader or Button node id.
    pub id: String,
    /// 0-1 (a Button is on above 0.5).
    pub value: f32,
}

#[derive(Clone)]
pub struct LumenMcp {
    show: Arc<ShowService>,
    engine: Arc<EngineManager>,
    tool_router: ToolRouter<Self>,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for LumenMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("LumenDMX", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

#[tool_router(router = tool_router)]
impl LumenMcp {
    pub fn new(show: Arc<ShowService>, engine: Arc<EngineManager>) -> Self {
        Self { show, engine, tool_router: Self::tool_router() }
    }

    /// Show overview: name, universes and their controllers, fixture and node counts, groups, graph issues, tempo.
    #[tool]
    async fn get_show(&self) -> ToolResult {
        let s = self.show.snapshot();
        json(serde_json::json!({
            "name": s.name,
            "unsavedChanges": s.dirty,
            "universes": s.universes,
            "fixtureCount": s.patch.len(),
            "unknownFixtureTypes": s.unresolved,
            "groups": s.groups,
            "nodeCount": s.graph.nodes.len(),
            "graphIssues": self.show.graph_issues(),
            "bpm": self.engine.bpm(),
        }))
    }

    /// Fixture types in the library, with each mode's channel count and the attribute of each channel.
    #[tool]
    async fn list_fixture_types(&self) -> ToolResult {
        json(self.show.fixture_types())
    }

    /// Patched fixtures: number, name, type, mode, universe, address, orientation flags.
    #[tool]
    async fn list_fixtures(&self) -> ToolResult {
        json(self.show.snapshot().patch)
    }

    /// Patch one or more fixtures of one type into a universe. Addresses and fixture numbers are picked automatically when omitted; several fixtures are patched back to back.
    #[tool]
    async fn add_fixtures(&self, Parameters(args): Parameters<AddFixtures>) -> ToolResult {
        json(self.show.add_fixtures(args)?)
    }

    /// Change a fixture's name, number, universe, address or pan/tilt invert/swap. Omitted fields stay as they are.
    #[tool]
    async fn update_fixture(&self, Parameters(args): Parameters<UpdateFixtureArgs>) -> ToolResult {
        json(self.show.update_fixture(args.number, args.update)?)
    }

    /// Unpatch fixtures (also removes them from groups).
    #[tool]
    async fn remove_fixtures(&self, Parameters(args): Parameters<FixtureNumbers>) -> ToolResult {
        self.show.remove_fixtures(&args.numbers)?;
        Ok("Removed".into())
    }

    /// Add a universe, optionally with its DMX controller (driver).
    #[tool]
    async fn add_universe(&self, Parameters(args): Parameters<AddUniverseArgs>) -> ToolResult {
        json(self.show.add_universe(args.name, args.driver.unwrap_or_default())?)
    }

    /// Create a group from fixture numbers (in fan order). Returns it with its id.
    #[tool]
    async fn create_group(&self, Parameters(args): Parameters<CreateGroupArgs>) -> ToolResult {
        json(self.show.create_group(args.name, args.fixture_numbers)?)
    }

    /// Rename a group and/or replace its members.
    #[tool]
    async fn update_group(&self, Parameters(args): Parameters<UpdateGroupArgs>) -> ToolResult {
        json(self.show.update_group(args.id, args.name, args.fixture_numbers)?)
    }

    /// Delete a group.
    #[tool]
    async fn delete_group(&self, Parameters(args): Parameters<IdArg>) -> ToolResult {
        self.show.delete_group(args.id)?;
        Ok("Deleted".into())
    }

    /// Every node type: kind, description, input/output sockets (id, type, default) and params (id, kind, range/options, default). Switch, Mix, Any and Output change their sockets with their params.
    #[tool]
    async fn list_node_types(&self) -> ToolResult {
        json(node_types())
    }

    /// The node graph: nodes (id, kind, position, params, actual sockets, issues) and edges.
    #[tool]
    async fn get_graph(&self) -> ToolResult {
        json(self.show.snapshot().graph)
    }

    /// Add a node. Returns it with its id.
    #[tool]
    async fn add_node(&self, Parameters(args): Parameters<AddNodeArgs>) -> ToolResult {
        let params = args.params.into_iter().map(|(k, v)| (k, Some(v))).collect();
        json(self.show.add_node(args.kind, NodePosition { x: args.x, y: args.y }, params)?)
    }

    /// Delete nodes and their links.
    #[tool]
    async fn remove_nodes(&self, Parameters(args): Parameters<NodeIds>) -> ToolResult {
        self.show.remove_nodes(&args.ids)?;
        Ok("Removed".into())
    }

    /// Link an output socket to an input socket (replacing whatever fed that input). Socket ids come from get_graph / list_node_types.
    #[tool]
    async fn connect(&self, Parameters(edge): Parameters<Edge>) -> ToolResult {
        self.show.connect(edge)?;
        Ok("Connected".into())
    }

    /// Remove the link into an input socket.
    #[tool]
    async fn disconnect(&self, Parameters(args): Parameters<DisconnectArgs>) -> ToolResult {
        self.show.disconnect(&args.to_node, &args.to_socket)?;
        Ok("Disconnected".into())
    }

    /// Set a node's params (null resets one). Values for unconnected inputs go under the input's socket id.
    #[tool]
    async fn set_params(&self, Parameters(args): Parameters<SetParamsArgs>) -> ToolResult {
        json(self.show.set_params(&args.id, args.params)?)
    }

    /// The user's saved node presets (app-wide building blocks): id, name, the nodes in each.
    #[tool]
    async fn list_node_presets(&self) -> ToolResult {
        json(self.show.node_presets().iter().map(preset_summary).collect::<Vec<_>>())
    }

    /// Add a copy of a node preset with its top-left node at x, y (one undo step). Returns the new nodes.
    #[tool]
    async fn spawn_node_preset(&self, Parameters(args): Parameters<SpawnPresetArgs>) -> ToolResult {
        let ids = self.show.spawn_node_preset(&args.id, NodePosition { x: args.x, y: args.y })?;
        let graph = self.show.snapshot().graph;
        json(graph.nodes.iter().filter(|n| ids.contains(&n.node.id)).map(|n| &n.node).collect::<Vec<_>>())
    }

    /// Save nodes and the links between them as a named preset (app-wide, outside the show). Only when the user asks.
    #[tool]
    async fn save_node_preset(&self, Parameters(args): Parameters<SavePresetArgs>) -> ToolResult {
        json(preset_summary(&self.show.save_node_preset(&args.name, &args.ids)?))
    }

    /// Replace the entire graph in one step (one undo). Node ids must be unique; links to missing sockets are dropped.
    #[tool]
    async fn replace_graph(&self, Parameters(args): Parameters<ReplaceGraphArgs>) -> ToolResult {
        self.show.replace_graph(args.graph)?;
        json(self.show.graph_issues())
    }

    /// What every fixture is outputting right now: each attribute's DMX value (0-255), read from the live output.
    #[tool]
    async fn get_output(&self) -> ToolResult {
        let snapshot = self.show.snapshot();
        let resolved = self.show.resolved();
        let mut frames = BTreeMap::new();
        let fixtures: Vec<serde_json::Value> = resolved
            .fixtures
            .iter()
            .map(|f| {
                let frame = *frames.entry(f.universe).or_insert_with(|| self.engine.get_universe_data(f.universe));
                let read = |ch: u16| frame.get(ch as usize - 1).copied().unwrap_or(0);
                let values: BTreeMap<String, u8> = f.attributes.iter().map(|(a, c)| (a.clone(), read(c.coarse))).collect();
                let cells: Vec<BTreeMap<String, u8>> =
                    f.cells.iter().map(|cell| cell.iter().map(|(a, c)| (a.clone(), read(c.coarse))).collect()).collect();
                let name = snapshot.patch.iter().find(|p| p.fixture_number == f.fixture_number).map(|p| p.name.clone());
                serde_json::json!({ "number": f.fixture_number, "name": name, "universe": f.universe, "values": values, "cells": cells })
            })
            .collect();
        json(fixtures)
    }

    /// The show tempo: BPM and the running beat count.
    #[tool]
    async fn get_tempo(&self) -> ToolResult {
        json(self.engine.tempo_status())
    }

    /// Set the show BPM (20-300). Ignored while an external tempo source (audio, MIDI clock, Mixxx) drives it.
    #[tool]
    async fn set_bpm(&self, Parameters(args): Parameters<BpmArg>) -> ToolResult {
        self.engine.set_bpm(args.bpm);
        Ok(format!("BPM {}", self.engine.bpm()))
    }

    /// Move a playback-bar Fader or press/release a Button (0-1).
    #[tool]
    async fn set_control(&self, Parameters(args): Parameters<ControlArgs>) -> ToolResult {
        self.show.set_control(&args.id, args.value)?;
        Ok("Set".into())
    }

    /// Undo the last show edit (patch, groups or graph), whoever made it.
    #[tool]
    async fn undo(&self) -> ToolResult {
        Ok(format!("Undid: {}", self.show.undo()?))
    }

    /// Redo the last undone edit.
    #[tool]
    async fn redo(&self) -> ToolResult {
        Ok(format!("Redid: {}", self.show.redo()?))
    }
}
