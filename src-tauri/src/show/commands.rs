//! Tauri commands over `ShowService`, and the sink wiring it to the engine,
//! the output drivers and the UI.
//!
//! Open/Save dialogs run natively via `rfd` in these (synchronous, so
//! main-thread) commands.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, State};

use super::graph_ops::{NodeMove, ParamChanges};
use super::library::FixtureTypeSummary;
use super::resolve::ResolvedPatch;
use super::service::{AddFixtures, FixtureUpdate, ShowService, ShowSink, ShowSnapshot};
use crate::engine::manager::EngineManager;
use crate::graph::compile::GraphPlan;
use crate::graph::model::{Edge, Graph, Node, NodePosition};
use crate::graph::registry::{node_types, NodeKind, NodeTypeInfo};
use crate::output::config::DriverConfig;
use crate::output::manager::OutputManager;
use crate::showfile::schema::{Group, PatchedFixture, RecentShowEntry, UniverseConfig};
use crate::showfile::store::sanitize_filename;

/// Emitted with the new revision after every change to the show.
pub const CHANGED_EVENT: &str = "show://changed";

pub struct AppSink {
    pub app: AppHandle,
    pub engine: Arc<EngineManager>,
}

impl ShowSink for AppSink {
    fn patch_resolved(&self, patch: &ResolvedPatch) {
        self.engine.set_patch(patch);
    }

    fn graph_compiled(&self, plan: Arc<GraphPlan>) {
        self.engine.set_graph(plan);
    }

    fn controls(&self) -> HashMap<String, f32> {
        self.engine.controls()
    }

    fn set_controls(&self, controls: HashMap<String, f32>) {
        self.engine.set_controls(controls);
    }

    fn set_control(&self, id: &str, value: f32) {
        self.engine.set_control(id, value);
    }

    fn configure_universe(&self, id: u32, driver: &DriverConfig) -> Result<(), String> {
        self.app.state::<OutputManager>().configure(id, driver.clone())
    }

    fn remove_universe(&self, id: u32) {
        self.app.state::<OutputManager>().remove(id);
    }

    fn bpm(&self) -> f32 {
        self.engine.bpm()
    }

    fn set_bpm(&self, bpm: f32) {
        self.engine.set_bpm(bpm);
    }

    fn changed(&self, revision: u64) {
        let _ = self.app.emit(CHANGED_EVENT, revision);
    }
}

#[tauri::command]
pub fn show_get(show: State<Arc<ShowService>>) -> ShowSnapshot {
    show.snapshot()
}

#[tauri::command]
pub fn show_fixture_types(show: State<Arc<ShowService>>) -> Vec<FixtureTypeSummary> {
    show.fixture_types()
}

/// Replaces the fixture library; returns the definitions that couldn't be read.
#[tauri::command]
pub fn show_set_library(show: State<Arc<ShowService>>, definitions: Vec<serde_json::Value>) -> Vec<String> {
    show.set_library(definitions)
}

#[tauri::command]
pub fn show_add_fixtures(show: State<Arc<ShowService>>, request: AddFixtures) -> Result<Vec<PatchedFixture>, String> {
    show.add_fixtures(request)
}

#[tauri::command]
pub fn show_update_fixture(show: State<Arc<ShowService>>, number: u32, update: FixtureUpdate) -> Result<PatchedFixture, String> {
    show.update_fixture(number, update)
}

#[tauri::command]
pub fn show_remove_fixtures(show: State<Arc<ShowService>>, numbers: Vec<u32>) -> Result<(), String> {
    show.remove_fixtures(&numbers)
}

#[tauri::command]
pub fn show_create_group(show: State<Arc<ShowService>>, name: String, fixture_numbers: Vec<u32>) -> Result<Group, String> {
    show.create_group(name, fixture_numbers)
}

#[tauri::command]
pub fn show_update_group(
    show: State<Arc<ShowService>>,
    id: u32,
    name: Option<String>,
    fixture_numbers: Option<Vec<u32>>,
) -> Result<Group, String> {
    show.update_group(id, name, fixture_numbers)
}

#[tauri::command]
pub fn show_delete_group(show: State<Arc<ShowService>>, id: u32) -> Result<(), String> {
    show.delete_group(id)
}

#[tauri::command]
pub fn show_undo(show: State<Arc<ShowService>>) -> Result<String, String> {
    show.undo()
}

#[tauri::command]
pub fn show_redo(show: State<Arc<ShowService>>) -> Result<String, String> {
    show.redo()
}

#[tauri::command]
pub fn show_add_universe(show: State<Arc<ShowService>>, name: Option<String>, driver: DriverConfig) -> Result<UniverseConfig, String> {
    show.add_universe(name, driver)
}

#[tauri::command]
pub fn show_update_universe(
    show: State<Arc<ShowService>>,
    id: u32,
    name: Option<String>,
    driver: Option<DriverConfig>,
) -> Result<UniverseConfig, String> {
    show.update_universe(id, name, driver)
}

#[tauri::command]
pub fn show_remove_universe(show: State<Arc<ShowService>>, id: u32) -> Result<(), String> {
    show.remove_universe(id)
}

#[tauri::command]
pub fn show_rename(show: State<Arc<ShowService>>, name: String) {
    show.rename_show(name);
}

#[tauri::command]
pub fn show_new(show: State<Arc<ShowService>>) {
    show.new_show();
}

/// Opens `path`, or asks for a file when omitted. `Ok(false)`: cancelled.
#[tauri::command]
pub fn show_open(show: State<Arc<ShowService>>, path: Option<String>) -> Result<bool, String> {
    let chosen = match path {
        Some(p) => Some(PathBuf::from(p)),
        None => rfd::FileDialog::new().add_filter("LumenDMX show", &["lumen"]).pick_file(),
    };
    let Some(chosen) = chosen else { return Ok(false) };
    show.open(&chosen)?;
    Ok(true)
}

/// Saves to the show's file, or asks where (first save, or `save_as`).
/// `Ok(false)`: cancelled.
#[tauri::command]
pub fn show_save(show: State<Arc<ShowService>>, save_as: bool) -> Result<bool, String> {
    let existing = if save_as { None } else { show.path().map(PathBuf::from) };
    let chosen = existing.or_else(|| {
        rfd::FileDialog::new()
            .add_filter("LumenDMX show", &["lumen"])
            .set_file_name(format!("{}.lumen", sanitize_filename(&show.name())))
            .save_file()
    });
    let Some(chosen) = chosen else { return Ok(false) };
    show.save(&chosen)?;
    Ok(true)
}

#[tauri::command]
pub fn show_recent(show: State<Arc<ShowService>>) -> Vec<RecentShowEntry> {
    show.recent()
}

// --- Graph ---

#[tauri::command]
pub fn graph_node_types() -> Vec<NodeTypeInfo> {
    node_types()
}

#[tauri::command]
pub fn graph_add_node(show: State<Arc<ShowService>>, kind: NodeKind, position: NodePosition, params: Option<ParamChanges>) -> Result<Node, String> {
    show.add_node(kind, position, params.unwrap_or_default())
}

#[tauri::command]
pub fn graph_remove_nodes(show: State<Arc<ShowService>>, ids: Vec<String>) -> Result<(), String> {
    show.remove_nodes(&ids)
}

#[tauri::command]
pub fn graph_connect(show: State<Arc<ShowService>>, edge: Edge) -> Result<(), String> {
    show.connect(edge)
}

#[tauri::command]
pub fn graph_disconnect(show: State<Arc<ShowService>>, to_node: String, to_socket: String) -> Result<(), String> {
    show.disconnect(&to_node, &to_socket)
}

#[tauri::command]
pub fn graph_set_params(show: State<Arc<ShowService>>, id: String, params: ParamChanges) -> Result<Node, String> {
    show.set_params(&id, params)
}

#[tauri::command]
pub fn graph_move_nodes(show: State<Arc<ShowService>>, moves: Vec<NodeMove>) -> Result<(), String> {
    show.move_nodes(&moves)
}

#[tauri::command]
pub fn graph_replace(show: State<Arc<ShowService>>, graph: Graph) -> Result<(), String> {
    show.replace_graph(graph)
}

#[tauri::command]
pub fn graph_set_control(show: State<Arc<ShowService>>, id: String, value: f32) -> Result<(), String> {
    show.set_control(&id, value)
}

#[tauri::command]
pub fn graph_controls(engine: State<Arc<EngineManager>>) -> HashMap<String, f32> {
    engine.controls()
}

/// The last MIDI controller/note received, for MIDI learn:
/// `[isNote, channel 1-16, number]`.
#[tauri::command]
pub fn graph_last_midi(engine: State<Arc<EngineManager>>) -> Option<(bool, u8, u8)> {
    engine.last_midi()
}
