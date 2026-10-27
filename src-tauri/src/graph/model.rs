//! The node graph as stored in the show file and edited by the UI and MCP
//! clients: nodes with parameters, and edges from an output socket of one
//! node to an input socket of another.

use std::collections::{BTreeMap, HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::registry::NodeKind;

const FRAME_DEFAULT_SIZE: (f32, f32) = (400.0, 300.0);

/// A parameter value. Numbers, switches and text cover every node's
/// settings (colours are "#rrggbb" text, fixture lists "1-4, 7").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(untagged)]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum ParamValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

impl ParamValue {
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            ParamValue::Number(n) => Some(*n as f32),
            ParamValue::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            ParamValue::Text(t) => t.trim().parse().ok(),
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ParamValue::Bool(b) => Some(*b),
            ParamValue::Number(n) => Some(*n != 0.0),
            ParamValue::Text(_) => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            ParamValue::Text(t) => Some(t),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct NodePosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct Node {
    /// Unique within the graph, e.g. "n12".
    pub id: String,
    pub kind: NodeKind,
    pub position: NodePosition,
    /// Settings by param id. Also holds the value of an unconnected input,
    /// under the input socket's id. Missing entries use the defaults.
    #[serde(default)]
    pub params: BTreeMap<String, ParamValue>,
    /// The Frame this node sits in, if any. Kept apart from positions so a
    /// collapsed frame's hidden box can't claim nodes it only overlaps.
    #[serde(default)]
    pub frame: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct Edge {
    pub from_node: String,
    pub from_socket: String,
    pub to_node: String,
    pub to_socket: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl Graph {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// The next free "n<number>" id.
    pub fn next_id(&self) -> String {
        let max = self.nodes.iter().filter_map(|n| n.id.strip_prefix('n')?.parse::<u32>().ok()).max().unwrap_or(0);
        format!("n{}", max + 1)
    }
}

impl Node {
    fn frame_size(&self) -> (f32, f32) {
        let size = |id: &str, default: f32| self.params.get(id).and_then(ParamValue::as_f32).unwrap_or(default);
        (size("width", FRAME_DEFAULT_SIZE.0), size("height", FRAME_DEFAULT_SIZE.1))
    }

    fn is_collapsed(&self) -> bool {
        self.params.get("collapsed").and_then(ParamValue::as_bool).unwrap_or(false)
    }

    /// Whether a Frame's (expanded) box holds the point.
    fn frame_holds(&self, p: NodePosition) -> bool {
        let (w, h) = self.frame_size();
        self.kind == NodeKind::Frame && (0.0..=w).contains(&(p.x - self.position.x)) && (0.0..=h).contains(&(p.y - self.position.y))
    }
}

/// Which node sits in which Frame. Membership is stored on the nodes
/// (`Node::frame`) and changes only when a node is dropped, added or a
/// frame is resized, never from where things happen to overlap later.
impl Graph {
    /// Every node inside frame `id`, at any depth.
    pub fn descendants(&self, id: &str) -> HashSet<String> {
        let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
        for n in &self.nodes {
            if let Some(f) = &n.frame {
                children.entry(f.as_str()).or_default().push(n.id.as_str());
            }
        }
        let mut found = HashSet::new();
        let mut stack = vec![id];
        while let Some(f) = stack.pop() {
            for &c in children.get(f).into_iter().flatten() {
                if found.insert(c.to_string()) {
                    stack.push(c);
                }
            }
        }
        found
    }

    /// Whether `id` is inside frame `ancestor`, at any depth.
    pub fn is_inside(&self, id: &str, ancestor: &str) -> bool {
        let mut at = self.node(id).and_then(|n| n.frame.as_deref());
        for _ in 0..=self.nodes.len() {
            match at {
                Some(f) if f == ancestor => return true,
                Some(f) => at = self.node(f).and_then(|n| n.frame.as_deref()),
                None => return false,
            }
        }
        false
    }

    /// The smallest expanded frame holding `p`, other than those in `skip`.
    /// A collapsed frame is drawn as a small node, so it takes nothing.
    pub fn frame_at(&self, p: NodePosition, skip: &HashSet<String>) -> Option<String> {
        self.nodes
            .iter()
            .filter(|f| f.kind == NodeKind::Frame && !f.is_collapsed() && !skip.contains(&f.id) && f.frame_holds(p))
            .min_by(|a, b| {
                let area = |f: &Node| f.frame_size().0 * f.frame_size().1;
                area(a).total_cmp(&area(b))
            })
            .map(|f| f.id.clone())
    }

    /// Puts the node into the frame under it, or none. Not into itself or
    /// anything inside it.
    pub fn reframe(&mut self, id: &str) {
        let Some(node) = self.node(id) else { return };
        let mut skip = self.descendants(id);
        skip.insert(id.to_string());
        let frame = self.frame_at(node.position, &skip);
        if let Some(n) = self.node_mut(id) {
            n.frame = frame;
        }
    }

    /// A frame was created or resized: nodes now inside it, and sharing its
    /// own parent, join it; members left outside go to its parent.
    pub fn refit(&mut self, frame: &str) {
        let Some(f) = self.node(frame).cloned() else { return };
        let inside = self.descendants(frame);
        for n in &mut self.nodes {
            if n.id == frame {
                continue;
            }
            let holds = f.frame_holds(n.position);
            if n.frame.as_deref() == Some(frame) && !holds {
                n.frame = f.frame.clone();
            } else if holds && n.frame == f.frame && !inside.contains(&n.id) {
                n.frame = Some(frame.to_string());
            }
        }
    }

    /// Nodes of frames about to be removed move up to the nearest frame that stays.
    pub fn release_from(&mut self, removed: &HashSet<String>) {
        let parents: HashMap<String, Option<String>> = self.nodes.iter().map(|n| (n.id.clone(), n.frame.clone())).collect();
        for n in &mut self.nodes {
            let mut up = n.frame.clone();
            while let Some(f) = up.as_ref().filter(|f| removed.contains(*f)) {
                up = parents.get(f).cloned().flatten();
            }
            n.frame = up;
        }
    }

    /// Drops frame links that point nowhere, at a non-frame, or in a loop.
    pub fn sanitize_frames(&mut self) {
        let frames: HashSet<String> = self.nodes.iter().filter(|n| n.kind == NodeKind::Frame).map(|n| n.id.clone()).collect();
        for n in &mut self.nodes {
            if n.frame.as_ref().is_some_and(|f| !frames.contains(f) || *f == n.id) {
                n.frame = None;
            }
        }
        let ids: Vec<String> = self.nodes.iter().map(|n| n.id.clone()).collect();
        for id in ids {
            if self.is_inside(&id, &id) {
                if let Some(n) = self.node_mut(&id) {
                    n.frame = None;
                }
            }
        }
    }

    /// For a graph with frames but no membership at all (written without
    /// it): works it out from where nodes sit, collapsed frames included.
    pub fn infer_frames_if_unset(&mut self) {
        if self.nodes.iter().any(|n| n.frame.is_some()) || !self.nodes.iter().any(|n| n.kind == NodeKind::Frame) {
            return;
        }
        let snapshot = self.clone();
        for n in &mut self.nodes {
            n.frame = snapshot
                .nodes
                .iter()
                .filter(|f| f.id != n.id && f.frame_holds(n.position))
                .min_by(|a, b| (a.frame_size().0 * a.frame_size().1).total_cmp(&(b.frame_size().0 * b.frame_size().1)))
                .map(|f| f.id.clone());
        }
        self.sanitize_frames();
    }
}
