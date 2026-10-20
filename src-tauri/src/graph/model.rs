//! The node graph as stored in the show file and edited by the UI and MCP
//! clients: nodes with parameters, and edges from an output socket of one
//! node to an input socket of another.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::registry::NodeKind;

/// A parameter value. Numbers, switches and text cover every node's
/// settings (colours are "#rrggbb" text, fixture lists "1-4, 7").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct NodePosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct Edge {
    pub from_node: String,
    pub from_socket: String,
    pub to_node: String,
    pub to_socket: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
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
