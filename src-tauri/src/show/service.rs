//! The open show and every edit to it. The UI (Tauri commands) and MCP
//! clients both go through `ShowService`, so they share one undo history and
//! one notion of "unsaved changes", and each edit re-resolves the patch for
//! the engine and tells the UI to refetch.
//!
//! Side effects outside the show itself (engine, output drivers, events)
//! go through `ShowSink`, so the service runs in tests without Tauri.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::library::{FixtureLibrary, FixtureTypeSummary};
use super::resolve::{resolve, ResolvedPatch};
use crate::graph::compile::{compile, GraphIssue, GraphPlan};
use crate::graph::model::{Edge, Graph, Node};
use crate::graph::registry::{node_sockets, SocketInfo};
use crate::output::config::DriverConfig;
use crate::output::UNIVERSE_SIZE;
use crate::showfile::schema::{Group, PatchedFixture, RecentShowEntry, ShowFile, UniverseConfig, CURRENT_SCHEMA_VERSION};
use crate::showfile::store::{read_show_file, write_show_file, ShowStore};

const UNDO_LIMIT: usize = 100;
/// The largest beat offset either way: past this it's a different beat, not latency.
pub const MAX_BEAT_OFFSET_MS: f32 = 500.0;

/// Everything the show does to the world outside itself.
pub trait ShowSink: Send + Sync {
    fn patch_resolved(&self, patch: &ResolvedPatch);
    fn graph_compiled(&self, plan: Arc<GraphPlan>);
    /// Playback-bar control values by node id.
    fn controls(&self) -> HashMap<String, f32>;
    fn set_controls(&self, controls: HashMap<String, f32>);
    fn set_control(&self, id: &str, value: f32);
    fn configure_universe(&self, id: u32, driver: &DriverConfig) -> Result<(), String>;
    fn remove_universe(&self, id: u32);
    fn bpm(&self) -> f32;
    fn set_bpm(&self, bpm: f32);
    fn set_beat_offset(&self, ms: f32);
    /// The show changed; `revision` is its new revision.
    fn changed(&self, revision: u64);
}

/// The show as the UI renders it.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct ShowSnapshot {
    /// Bumped by every change.
    #[ts(type = "number")]
    pub revision: u64,
    pub id: String,
    pub name: String,
    /// The show's file on disk, once saved.
    pub path: Option<String>,
    /// Changed since the last open/save.
    pub dirty: bool,
    pub universes: Vec<UniverseConfig>,
    pub patch: Vec<PatchedFixture>,
    pub groups: Vec<Group>,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
    /// Patched fixtures whose type or mode isn't in the fixture library.
    pub unresolved: Vec<u32>,
    pub graph: GraphView,
    pub beat_offset_ms: f32,
}

/// A graph node as the editor shows it: the node, its actual sockets, and
/// what's wrong with it, if anything.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct NodeView {
    #[serde(flatten)]
    pub node: Node,
    pub inputs: Vec<SocketInfo>,
    pub outputs: Vec<SocketInfo>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct GraphView {
    pub nodes: Vec<NodeView>,
    pub edges: Vec<Edge>,
}

/// Patch one or more fixtures of one type.
#[derive(Debug, Clone, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct AddFixtures {
    pub fixture_type: String,
    #[serde(default)]
    pub mode_index: u32,
    pub universe: u32,
    /// Start address of the first fixture; the rest follow back to back.
    /// Omitted: the first free addresses in the universe.
    #[serde(default)]
    #[ts(optional)]
    pub address: Option<u16>,
    /// How many fixtures (default 1).
    #[serde(default)]
    #[ts(optional)]
    pub count: Option<u32>,
    /// Fixture number of the first fixture; the rest count up. Omitted:
    /// after the highest number in use.
    #[serde(default)]
    #[ts(optional)]
    pub first_number: Option<u32>,
    /// Name of the fixtures (numbered when more than one). Omitted: the
    /// fixture type's model name.
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
}

/// Changes to one fixture; omitted fields stay as they are.
#[derive(Debug, Clone, Default, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct FixtureUpdate {
    #[ts(optional)]
    pub name: Option<String>,
    #[ts(optional)]
    pub fixture_number: Option<u32>,
    #[ts(optional)]
    pub universe: Option<u32>,
    #[ts(optional)]
    pub address: Option<u16>,
    #[ts(optional)]
    pub invert_pan: Option<bool>,
    #[ts(optional)]
    pub invert_tilt: Option<bool>,
    #[ts(optional)]
    pub swap_pan_tilt: Option<bool>,
}

/// The part of the show undo/redo restores.
#[derive(Clone)]
struct Snapshot {
    label: String,
    patch: Vec<PatchedFixture>,
    groups: Vec<Group>,
    graph: Graph,
}

pub(super) struct Inner {
    pub(super) show: ShowFile,
    path: Option<String>,
    library: FixtureLibrary,
    resolved: Arc<ResolvedPatch>,
    issues: Vec<GraphIssue>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    revision: u64,
    saved_revision: u64,
}

impl Inner {
    fn snapshot(&self, label: &str) -> Snapshot {
        Snapshot {
            label: label.to_string(),
            patch: self.show.patch.clone(),
            groups: self.show.groups.clone(),
            graph: self.show.graph.clone(),
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.show.patch = snapshot.patch;
        self.show.groups = snapshot.groups;
        self.show.graph = snapshot.graph;
    }

    fn graph_view(&self) -> GraphView {
        let mut issues: HashMap<&str, Vec<String>> = HashMap::new();
        for issue in &self.issues {
            issues.entry(issue.node.as_str()).or_default().push(issue.message.clone());
        }
        GraphView {
            nodes: self
                .show
                .graph
                .nodes
                .iter()
                .map(|node| {
                    let (inputs, outputs) = node_sockets(node.kind, &node.params);
                    NodeView { node: node.clone(), inputs, outputs, issues: issues.remove(node.id.as_str()).unwrap_or_default() }
                })
                .collect(),
            edges: self.show.graph.edges.clone(),
        }
    }

    fn channel_count(&self, fixture: &PatchedFixture) -> u16 {
        self.library.mode(&fixture.fixture_id, fixture.mode_index).map_or(1, |m| m.channel_count.max(1))
    }

    /// Fixtures in `universe` overlapping `address..address+count`, other
    /// than those numbered in `ignore`.
    fn collisions(&self, universe: u32, address: u16, count: u16, ignore: &HashSet<u32>) -> Vec<&PatchedFixture> {
        let end = address + count - 1;
        self.show
            .patch
            .iter()
            .filter(|f| f.universe == universe && !ignore.contains(&f.fixture_number))
            .filter(|f| {
                let f_end = f.address + self.channel_count(f) - 1;
                f.address <= end && address <= f_end
            })
            .collect()
    }

    /// The first address in `universe` with `count` free channels.
    fn next_free_address(&self, universe: u32, count: u16) -> Option<u16> {
        let mut spans: Vec<(u16, u16)> = self
            .show
            .patch
            .iter()
            .filter(|f| f.universe == universe)
            .map(|f| (f.address, f.address + self.channel_count(f) - 1))
            .collect();
        spans.sort_unstable();
        let mut candidate = 1u16;
        for (start, end) in spans {
            if candidate + count - 1 < start {
                break;
            }
            candidate = candidate.max(end + 1);
        }
        (candidate as usize + count as usize - 1 <= UNIVERSE_SIZE).then_some(candidate)
    }

    fn next_fixture_number(&self) -> u32 {
        self.show.patch.iter().map(|f| f.fixture_number).max().unwrap_or(0) + 1
    }

    fn fixture_mut(&mut self, number: u32) -> Result<&mut PatchedFixture, String> {
        self.show.patch.iter_mut().find(|f| f.fixture_number == number).ok_or_else(|| format!("No fixture {number}"))
    }

    fn require_universe(&self, id: u32) -> Result<(), String> {
        if self.show.universes.iter().any(|u| u.id == id) {
            Ok(())
        } else {
            Err(format!("No universe {id}"))
        }
    }

    fn check_fits(&self, universe: u32, address: u16, count: u16, ignore: &HashSet<u32>) -> Result<(), String> {
        if address == 0 || address as usize + count as usize - 1 > UNIVERSE_SIZE {
            return Err(format!("Address {address} with {count} channels doesn't fit in a universe (1-512)"));
        }
        let hits = self.collisions(universe, address, count, ignore);
        if let Some(hit) = hits.first() {
            return Err(format!(
                "Address {address}-{} in universe {universe} overlaps {} (#{} at {})",
                address + count - 1,
                hit.name,
                hit.fixture_number,
                hit.address
            ));
        }
        Ok(())
    }
}

pub struct ShowService {
    pub(super) inner: Mutex<Inner>,
    pub(super) sink: Box<dyn ShowSink>,
    store: Option<ShowStore>,
}

impl ShowService {
    /// Starts with the last session if `store` has one, else a new show.
    pub fn new(sink: Box<dyn ShowSink>, store: Option<ShowStore>) -> Self {
        let restored = store.as_ref().and_then(|s| s.restore_session());
        let (show, path, recovered) = match restored {
            Some(r) => (r.show, r.path, r.recovered),
            None => (ShowFile::new_default("Untitled Show"), None, false),
        };
        let service = Self {
            inner: Mutex::new(Inner {
                show: ShowFile::new_default(""),
                path: None,
                library: FixtureLibrary::default(),
                resolved: Arc::new(ResolvedPatch::default()),
                issues: Vec::new(),
                undo: Vec::new(),
                redo: Vec::new(),
                revision: 0,
                saved_revision: 0,
            }),
            sink,
            store,
        };
        service.load(show, path);
        if recovered {
            // Recovered unsaved work: dirty until saved.
            service.inner.lock().unwrap().saved_revision = u64::MAX;
        }
        service
    }

    // --- Reading ---

    pub fn snapshot(&self) -> ShowSnapshot {
        let inner = self.inner.lock().unwrap();
        ShowSnapshot {
            revision: inner.revision,
            id: inner.show.id.clone(),
            name: inner.show.name.clone(),
            path: inner.path.clone(),
            dirty: inner.revision != inner.saved_revision,
            universes: inner.show.universes.clone(),
            patch: inner.show.patch.clone(),
            groups: inner.show.groups.clone(),
            undo_label: inner.undo.last().map(|s| s.label.clone()),
            redo_label: inner.redo.last().map(|s| s.label.clone()),
            unresolved: inner.resolved.unresolved.clone(),
            graph: inner.graph_view(),
            beat_offset_ms: inner.show.settings.beat_offset_ms,
        }
    }

    pub fn graph_issues(&self) -> Vec<GraphIssue> {
        self.inner.lock().unwrap().issues.clone()
    }

    pub fn fixture_types(&self) -> Vec<FixtureTypeSummary> {
        self.inner.lock().unwrap().library.summaries()
    }

    pub fn resolved(&self) -> Arc<ResolvedPatch> {
        self.inner.lock().unwrap().resolved.clone()
    }

    pub fn recent(&self) -> Vec<RecentShowEntry> {
        self.store.as_ref().map(|s| s.recent()).unwrap_or_default()
    }

    // --- Internals ---

    /// Re-resolves the patch and recompiles the graph for the engine, and
    /// announces the change.
    fn after_change(&self, inner: &mut Inner, show_changed: bool) {
        inner.revision += 1;
        if show_changed {
            inner.resolved = Arc::new(resolve(&inner.show.patch, &inner.library));
            self.sink.patch_resolved(&inner.resolved);
            let (plan, issues) = compile(&inner.show.graph, &inner.resolved, &inner.show.groups);
            inner.issues = issues;
            self.sink.graph_compiled(Arc::new(plan));
        }
        self.sink.changed(inner.revision);
    }

    /// An undoable edit of the patch, groups and/or graph. `f` either
    /// succeeds and leaves the show edited, or fails and leaves it as it was.
    pub(super) fn edit<R>(&self, label: &str, f: impl FnOnce(&mut Inner) -> Result<R, String>) -> Result<R, String> {
        let mut inner = self.inner.lock().unwrap();
        let before = inner.snapshot(label);
        match f(&mut inner) {
            Ok(result) => {
                inner.undo.push(before);
                if inner.undo.len() > UNDO_LIMIT {
                    inner.undo.remove(0);
                }
                inner.redo.clear();
                self.after_change(&mut inner, true);
                Ok(result)
            }
            Err(e) => {
                inner.restore(before);
                Err(e)
            }
        }
    }

    /// Replaces the open show (new/open/restore): drivers, tempo and patch
    /// follow it, and the undo history starts over.
    fn load(&self, show: ShowFile, path: Option<String>) {
        let mut inner = self.inner.lock().unwrap();
        let old_ids: Vec<u32> = inner.show.universes.iter().map(|u| u.id).collect();
        for id in old_ids {
            if !show.universes.iter().any(|u| u.id == id) {
                self.sink.remove_universe(id);
            }
        }
        for universe in &show.universes {
            if let Err(e) = self.sink.configure_universe(universe.id, &universe.driver) {
                eprintln!("Couldn't start universe {} ({}): {e}", universe.id, universe.name);
            }
        }
        self.sink.set_bpm(show.bpm);
        self.sink.set_beat_offset(show.settings.beat_offset_ms);
        self.sink.set_controls(show.controls.iter().map(|(k, v)| (k.clone(), *v)).collect());
        inner.show = show;
        inner.path = path;
        inner.undo.clear();
        inner.redo.clear();
        self.after_change(&mut inner, true);
        inner.saved_revision = inner.revision;
    }

    // --- Library ---

    /// Replaces the fixture library (pushed by the frontend). Returns the
    /// definitions that couldn't be read.
    pub fn set_library(&self, defs: Vec<serde_json::Value>) -> Vec<String> {
        let (library, rejected) = FixtureLibrary::from_json(defs);
        let mut inner = self.inner.lock().unwrap();
        inner.library = library;
        let dirty = inner.revision != inner.saved_revision;
        self.after_change(&mut inner, true);
        if !dirty {
            // A library refresh isn't an edit to the show.
            inner.saved_revision = inner.revision;
        }
        rejected
    }

    // --- Fixtures ---

    pub fn add_fixtures(&self, req: AddFixtures) -> Result<Vec<PatchedFixture>, String> {
        let count = req.count.unwrap_or(1).max(1);
        let label = if count == 1 { "Add fixture".to_string() } else { format!("Add {count} fixtures") };
        self.edit(&label, |inner| {
            inner.require_universe(req.universe)?;
            let def = inner.library.get(&req.fixture_type).ok_or_else(|| format!("No fixture type '{}'", req.fixture_type))?;
            let mode = def
                .modes
                .get(req.mode_index as usize)
                .ok_or_else(|| format!("'{}' has no mode {}", def.id, req.mode_index))?;
            let (model, channels) = (def.model.clone(), mode.channel_count.max(1));
            let first_number = req.first_number.unwrap_or_else(|| inner.next_fixture_number());
            let mut added = Vec::new();
            let mut next_address = req.address;
            for i in 0..count {
                let number = first_number + i;
                if inner.show.patch.iter().any(|f| f.fixture_number == number) {
                    return Err(format!("Fixture number {number} is already used"));
                }
                let address = match next_address {
                    Some(a) => a,
                    None => inner
                        .next_free_address(req.universe, channels)
                        .ok_or_else(|| format!("Universe {} has no room for {} more channels", req.universe, channels))?,
                };
                inner.check_fits(req.universe, address, channels, &HashSet::new())?;
                let base = req.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&model);
                let fixture = PatchedFixture {
                    id: uuid::Uuid::new_v4().to_string(),
                    fixture_id: req.fixture_type.clone(),
                    mode_index: req.mode_index,
                    address,
                    name: if count > 1 || req.name.is_none() { format!("{base} {}", i + 1) } else { base.to_string() },
                    universe: req.universe,
                    fixture_number: number,
                    invert_pan: false,
                    invert_tilt: false,
                    swap_pan_tilt: false,
                };
                inner.show.patch.push(fixture.clone());
                added.push(fixture);
                next_address = req.address.map(|_| address + channels);
            }
            Ok(added)
        })
    }

    pub fn update_fixture(&self, number: u32, update: FixtureUpdate) -> Result<PatchedFixture, String> {
        self.edit("Edit fixture", |inner| {
            let current = inner.fixture_mut(number)?.clone();
            let mut next = current.clone();
            if let Some(name) = update.name {
                next.name = name;
            }
            if let Some(universe) = update.universe {
                inner.require_universe(universe)?;
                next.universe = universe;
            }
            if let Some(address) = update.address {
                next.address = address;
            }
            if let Some(v) = update.invert_pan {
                next.invert_pan = v;
            }
            if let Some(v) = update.invert_tilt {
                next.invert_tilt = v;
            }
            if let Some(v) = update.swap_pan_tilt {
                next.swap_pan_tilt = v;
            }
            if next.universe != current.universe || next.address != current.address {
                let channels = inner.channel_count(&current);
                inner.check_fits(next.universe, next.address, channels, &HashSet::from([number]))?;
            }
            if let Some(new_number) = update.fixture_number.filter(|&n| n != number) {
                if new_number == 0 {
                    return Err("Fixture numbers start at 1".to_string());
                }
                if inner.show.patch.iter().any(|f| f.fixture_number == new_number) {
                    return Err(format!("Fixture number {new_number} is already used"));
                }
                next.fixture_number = new_number;
                for group in &mut inner.show.groups {
                    for n in &mut group.fixture_numbers {
                        if *n == number {
                            *n = new_number;
                        }
                    }
                }
            }
            *inner.fixture_mut(number)? = next.clone();
            Ok(next)
        })
    }

    pub fn remove_fixtures(&self, numbers: &[u32]) -> Result<(), String> {
        let label = if numbers.len() == 1 { "Remove fixture".to_string() } else { format!("Remove {} fixtures", numbers.len()) };
        self.edit(&label, |inner| {
            let gone: HashSet<u32> = numbers.iter().copied().collect();
            if let Some(missing) = numbers.iter().find(|n| !inner.show.patch.iter().any(|f| f.fixture_number == **n)) {
                return Err(format!("No fixture {missing}"));
            }
            inner.show.patch.retain(|f| !gone.contains(&f.fixture_number));
            for group in &mut inner.show.groups {
                group.fixture_numbers.retain(|n| !gone.contains(n));
            }
            Ok(())
        })
    }

    // --- Groups ---

    fn check_members(inner: &Inner, numbers: &[u32]) -> Result<(), String> {
        let mut seen = HashSet::new();
        for n in numbers {
            if !inner.show.patch.iter().any(|f| f.fixture_number == *n) {
                return Err(format!("No fixture {n}"));
            }
            if !seen.insert(*n) {
                return Err(format!("Fixture {n} is listed twice"));
            }
        }
        Ok(())
    }

    pub fn create_group(&self, name: String, fixture_numbers: Vec<u32>) -> Result<Group, String> {
        self.edit("Create group", |inner| {
            Self::check_members(inner, &fixture_numbers)?;
            let id = inner.show.groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
            let name = if name.trim().is_empty() { format!("Group {id}") } else { name };
            let group = Group { id, name, fixture_numbers };
            inner.show.groups.push(group.clone());
            Ok(group)
        })
    }

    pub fn update_group(&self, id: u32, name: Option<String>, fixture_numbers: Option<Vec<u32>>) -> Result<Group, String> {
        self.edit("Edit group", |inner| {
            if let Some(numbers) = &fixture_numbers {
                Self::check_members(inner, numbers)?;
            }
            let group = inner.show.groups.iter_mut().find(|g| g.id == id).ok_or_else(|| format!("No group {id}"))?;
            if let Some(name) = name {
                group.name = name;
            }
            if let Some(numbers) = fixture_numbers {
                group.fixture_numbers = numbers;
            }
            Ok(group.clone())
        })
    }

    pub fn delete_group(&self, id: u32) -> Result<(), String> {
        self.edit("Delete group", |inner| {
            let before = inner.show.groups.len();
            inner.show.groups.retain(|g| g.id != id);
            if inner.show.groups.len() == before {
                return Err(format!("No group {id}"));
            }
            Ok(())
        })
    }

    // --- Undo ---

    pub fn undo(&self) -> Result<String, String> {
        let mut inner = self.inner.lock().unwrap();
        let snapshot = inner.undo.pop().ok_or("Nothing to undo")?;
        let label = snapshot.label.clone();
        let current = inner.snapshot(&label);
        inner.redo.push(current);
        inner.restore(snapshot);
        self.after_change(&mut inner, true);
        Ok(label)
    }

    pub fn redo(&self) -> Result<String, String> {
        let mut inner = self.inner.lock().unwrap();
        let snapshot = inner.redo.pop().ok_or("Nothing to redo")?;
        let label = snapshot.label.clone();
        let current = inner.snapshot(&label);
        inner.undo.push(current);
        inner.restore(snapshot);
        self.after_change(&mut inner, true);
        Ok(label)
    }

    // --- Universes (not undoable: they drive hardware) ---

    pub fn add_universe(&self, name: Option<String>, driver: DriverConfig) -> Result<UniverseConfig, String> {
        let mut inner = self.inner.lock().unwrap();
        let id = inner.show.universes.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        let universe = UniverseConfig { id, name: name.unwrap_or_else(|| format!("Universe {id}")), driver };
        if let Err(e) = self.sink.configure_universe(id, &universe.driver) {
            eprintln!("Couldn't start universe {id}: {e}");
        }
        inner.show.universes.push(universe.clone());
        self.after_change(&mut inner, false);
        Ok(universe)
    }

    pub fn update_universe(&self, id: u32, name: Option<String>, driver: Option<DriverConfig>) -> Result<UniverseConfig, String> {
        let mut inner = self.inner.lock().unwrap();
        let universe = inner.show.universes.iter_mut().find(|u| u.id == id).ok_or_else(|| format!("No universe {id}"))?;
        if let Some(name) = name {
            universe.name = name;
        }
        let mut result = Ok(());
        if let Some(driver) = driver {
            result = self.sink.configure_universe(id, &driver);
            universe.driver = driver;
        }
        let universe = universe.clone();
        self.after_change(&mut inner, false);
        // The new driver is saved even when it couldn't start right now (a
        // controller that isn't plugged in yet); the status shows the error.
        result.map(|_| universe)
    }

    pub fn remove_universe(&self, id: u32) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        inner.require_universe(id)?;
        let patched = inner.show.patch.iter().filter(|f| f.universe == id).count();
        if patched > 0 {
            return Err(format!("Universe {id} still has {patched} fixture(s) patched; move or remove them first"));
        }
        inner.show.universes.retain(|u| u.id != id);
        self.sink.remove_universe(id);
        self.after_change(&mut inner, false);
        Ok(())
    }

    // --- Show and files ---

    pub fn rename_show(&self, name: String) {
        let mut inner = self.inner.lock().unwrap();
        inner.show.name = name;
        self.after_change(&mut inner, false);
    }

    /// Clamped to ±`MAX_BEAT_OFFSET_MS`.
    pub fn set_beat_offset(&self, ms: f32) {
        let ms = if ms.is_finite() { ms.clamp(-MAX_BEAT_OFFSET_MS, MAX_BEAT_OFFSET_MS) } else { 0.0 };
        let mut inner = self.inner.lock().unwrap();
        inner.show.settings.beat_offset_ms = ms;
        self.sink.set_beat_offset(ms);
        self.after_change(&mut inner, false);
    }

    pub fn new_show(&self) {
        self.load(ShowFile::new_default("Untitled Show"), None);
    }

    pub fn open(&self, path: &Path) -> Result<(), String> {
        let show = read_show_file(path)?;
        let path_str = path.to_string_lossy().to_string();
        if let Some(store) = &self.store {
            store.touch_recent(&path_str, &show.name);
        }
        self.load(show, Some(path_str));
        Ok(())
    }

    /// The file a plain Save writes to, if the show has been saved before.
    pub fn path(&self) -> Option<String> {
        self.inner.lock().unwrap().path.clone()
    }

    pub fn name(&self) -> String {
        self.inner.lock().unwrap().show.name.clone()
    }

    fn file_contents(&self, inner: &Inner) -> ShowFile {
        let mut show = inner.show.clone();
        show.schema_version = CURRENT_SCHEMA_VERSION;
        show.bpm = self.sink.bpm();
        let ids: HashSet<&str> = show.graph.nodes.iter().map(|n| n.id.as_str()).collect();
        show.controls = self.sink.controls().into_iter().filter(|(k, _)| ids.contains(k.as_str())).collect::<BTreeMap<_, _>>();
        show
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        inner.show.modified_at = chrono::Utc::now().to_rfc3339();
        let show = self.file_contents(&inner);
        write_show_file(path, &show)?;
        let path_str = path.to_string_lossy().to_string();
        if let Some(store) = &self.store {
            store.touch_recent(&path_str, &show.name);
        }
        inner.path = Some(path_str);
        inner.saved_revision = inner.revision;
        self.sink.changed(inner.revision);
        Ok(())
    }

    /// Writes an autosave snapshot if there are unsaved changes.
    pub fn autosave_if_dirty(&self) {
        let inner = self.inner.lock().unwrap();
        if inner.revision == inner.saved_revision {
            return;
        }
        let show = self.file_contents(&inner);
        drop(inner);
        if let Some(store) = &self.store {
            if let Err(e) = store.autosave(&show) {
                eprintln!("Autosave failed: {e}");
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::show::library::test_fixtures::{rgb_bar, rgb_par};

    #[derive(Default)]
    pub struct RecordingSink {
        pub patches: Mutex<Vec<Arc<ResolvedPatch>>>,
        pub configured: Mutex<Vec<u32>>,
        pub plan: Mutex<Arc<GraphPlan>>,
        pub controls: Mutex<HashMap<String, f32>>,
    }

    impl ShowSink for Arc<RecordingSink> {
        fn patch_resolved(&self, patch: &ResolvedPatch) {
            self.patches.lock().unwrap().push(Arc::new(patch.clone()));
        }
        fn graph_compiled(&self, plan: Arc<GraphPlan>) {
            *self.plan.lock().unwrap() = plan;
        }
        fn controls(&self) -> HashMap<String, f32> {
            self.controls.lock().unwrap().clone()
        }
        fn set_controls(&self, controls: HashMap<String, f32>) {
            *self.controls.lock().unwrap() = controls;
        }
        fn set_control(&self, id: &str, value: f32) {
            self.controls.lock().unwrap().insert(id.to_string(), value);
        }
        fn configure_universe(&self, id: u32, _driver: &DriverConfig) -> Result<(), String> {
            self.configured.lock().unwrap().push(id);
            Ok(())
        }
        fn remove_universe(&self, _id: u32) {}
        fn bpm(&self) -> f32 {
            120.0
        }
        fn set_bpm(&self, _bpm: f32) {}
        fn set_beat_offset(&self, _ms: f32) {}
        fn changed(&self, _revision: u64) {}
    }

    fn json(def: &crate::show::library::FixtureDefinition) -> serde_json::Value {
        let modes: Vec<serde_json::Value> = def
            .modes
            .iter()
            .map(|m| {
                let channels: Vec<serde_json::Value> = m
                    .channels
                    .iter()
                    .map(|c| serde_json::json!({ "offset": c.offset, "type": c.attribute, "cell": c.cell, "defaultValue": c.default_value }))
                    .collect();
                serde_json::json!({ "name": m.name, "channelCount": m.channel_count, "channels": channels })
            })
            .collect();
        serde_json::json!({ "id": def.id, "manufacturer": def.manufacturer, "model": def.model, "type": def.category, "modes": modes })
    }

    /// A service with the test par and bar in its library, two universes,
    /// and no file store.
    pub fn service() -> (ShowService, Arc<RecordingSink>) {
        let sink = Arc::new(RecordingSink::default());
        (service_with(Box::new(sink.clone())), sink)
    }

    /// The same, reporting to `sink`.
    pub fn service_with(sink: Box<dyn ShowSink>) -> ShowService {
        let service = ShowService::new(sink, None);
        service.set_library(vec![json(&rgb_par()), json(&rgb_bar())]);
        service.add_universe(None, DriverConfig::Null).unwrap();
        service
    }

    fn add(service: &ShowService, universe: u32, address: Option<u16>, count: u32) -> Result<Vec<PatchedFixture>, String> {
        service.add_fixtures(AddFixtures {
            fixture_type: "par".to_string(),
            mode_index: 0,
            universe,
            address,
            count: Some(count),
            first_number: None,
            name: None,
        })
    }

    #[test]
    fn adding_fixtures_finds_free_addresses_and_numbers() {
        let (service, sink) = service();
        add(&service, 1, Some(1), 1).unwrap();
        let added = add(&service, 1, None, 3).unwrap();
        assert_eq!(added.iter().map(|f| f.address).collect::<Vec<_>>(), vec![7, 13, 19]);
        assert_eq!(added.iter().map(|f| f.fixture_number).collect::<Vec<_>>(), vec![2, 3, 4]);
        assert_eq!(added[0].name, "Par 1");
        let resolved = sink.patches.lock().unwrap().last().unwrap().clone();
        assert_eq!(resolved.fixture(4).unwrap().channel("dimmer"), Some(19));
    }

    #[test]
    fn overlapping_addresses_and_unknown_universes_are_refused() {
        let (service, _) = service();
        add(&service, 1, Some(1), 1).unwrap();
        let err = add(&service, 1, Some(4), 1).unwrap_err();
        assert!(err.contains("overlaps"), "{err}");
        assert!(add(&service, 9, None, 1).unwrap_err().contains("No universe 9"));
        assert!(add(&service, 1, Some(510), 1).unwrap_err().contains("doesn't fit"));
        // A failed multi-add leaves nothing half-patched.
        assert!(add(&service, 1, Some(500), 3).is_err());
        assert_eq!(service.snapshot().patch.len(), 1);
    }

    #[test]
    fn universes_hold_their_own_fixtures_at_the_same_addresses() {
        let (service, _) = service();
        service.add_universe(Some("Stage left".into()), DriverConfig::Null).unwrap();
        add(&service, 1, Some(1), 1).unwrap();
        add(&service, 2, Some(1), 1).unwrap();
        let resolved = service.resolved();
        assert_eq!(resolved.fixture(2).unwrap().universe, 2);
        assert!(service.remove_universe(2).unwrap_err().contains("still has 1"));
    }

    #[test]
    fn renumbering_and_removing_fixtures_keeps_groups_consistent() {
        let (service, _) = service();
        add(&service, 1, None, 3).unwrap();
        let group = service.create_group("Pars".into(), vec![3, 1, 2]).unwrap();
        service.update_fixture(1, FixtureUpdate { fixture_number: Some(10), ..Default::default() }).unwrap();
        service.remove_fixtures(&[2]).unwrap();
        let snapshot = service.snapshot();
        assert_eq!(snapshot.groups[0].id, group.id);
        assert_eq!(snapshot.groups[0].fixture_numbers, vec![3, 10]);
        assert!(service.create_group("Bad".into(), vec![99]).is_err());
    }

    #[test]
    fn undo_and_redo_walk_the_history_and_track_dirtiness() {
        let (service, _) = service();
        let base = service.snapshot();
        add(&service, 1, None, 2).unwrap();
        service.create_group("All".into(), vec![1, 2]).unwrap();
        assert!(service.snapshot().dirty);
        assert_eq!(service.snapshot().undo_label.as_deref(), Some("Create group"));

        assert_eq!(service.undo().unwrap(), "Create group");
        assert!(service.snapshot().groups.is_empty());
        service.undo().unwrap();
        assert!(service.snapshot().patch.is_empty());
        assert!(service.undo().is_err());

        service.redo().unwrap();
        assert_eq!(service.snapshot().patch.len(), 2);
        assert_eq!(service.snapshot().redo_label.as_deref(), Some("Create group"));
        assert!(service.snapshot().revision > base.revision);
    }

    #[test]
    fn saving_and_reopening_round_trips_the_show() {
        let (service, _) = service();
        add(&service, 1, Some(5), 2).unwrap();
        service.create_group("Pair".into(), vec![2, 1]).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("show.lumen");
        service.save(&path).unwrap();
        assert!(!service.snapshot().dirty);

        let (other, _) = super::tests::service();
        other.open(&path).unwrap();
        let snapshot = other.snapshot();
        assert_eq!(snapshot.patch.len(), 2);
        assert_eq!(snapshot.groups[0].fixture_numbers, vec![2, 1]);
        assert!(!snapshot.dirty);
        assert!(snapshot.undo_label.is_none());
    }

    #[test]
    fn a_library_refresh_re_resolves_without_dirtying_the_show() {
        let sink = Arc::new(RecordingSink::default());
        let service = ShowService::new(Box::new(sink.clone()), None);
        service.add_universe(None, DriverConfig::Null).unwrap();
        // Patched before the library arrives (a restored session at startup).
        service.inner.lock().unwrap().show.patch.push(PatchedFixture {
            id: "x".into(),
            fixture_id: "par".into(),
            mode_index: 0,
            address: 1,
            name: "Par".into(),
            universe: 1,
            fixture_number: 1,
            invert_pan: false,
            invert_tilt: false,
            swap_pan_tilt: false,
        });
        {
            let mut inner = service.inner.lock().unwrap();
            inner.saved_revision = inner.revision;
        }
        service.set_library(vec![json(&rgb_par())]);
        assert!(service.resolved().fixture(1).is_some());
        assert!(!service.snapshot().dirty);
    }
}
