//! Node presets: a few connected nodes saved under a name, to drop into any
//! show later. Kept app-wide in the data directory (`node-presets.json`),
//! not in the show. Spawning one is an ordinary undoable graph edit.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::graph_ops::{prune_edges, tidy_any_inputs};
use super::service::ShowService;
use crate::graph::model::{Edge, Node, NodePosition};
use crate::graph::registry::NodeKind;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct NodePreset {
    pub id: String,
    pub name: String,
    /// Positions are relative: the top-left node sits at 0, 0.
    pub nodes: Vec<Node>,
    /// Only the links between the preset's own nodes.
    pub edges: Vec<Edge>,
}

impl ShowService {
    pub fn node_presets(&self) -> Vec<NodePreset> {
        self.presets.lock().unwrap().clone()
    }

    /// Saves `ids` (and the links between them) as a new preset. A Frame
    /// among them brings the nodes inside it.
    pub fn save_node_preset(&self, name: &str, ids: &[String]) -> Result<NodePreset, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Give the preset a name".to_string());
        }
        let preset = {
            let inner = self.inner.lock().unwrap();
            let graph = &inner.show.graph;
            let mut wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
            if let Some(missing) = wanted.iter().find(|id| graph.node(id).is_none()) {
                return Err(format!("No node {missing}"));
            }
            let frames: Vec<&str> = graph.nodes.iter().filter(|n| n.kind == NodeKind::Frame && wanted.contains(n.id.as_str())).map(|n| n.id.as_str()).collect();
            let inside: HashSet<String> = frames.into_iter().flat_map(|f| graph.descendants(f)).collect();
            wanted.extend(inside.iter().map(String::as_str));
            let mut nodes: Vec<Node> = graph.nodes.iter().filter(|n| wanted.contains(n.id.as_str())).cloned().collect();
            if nodes.is_empty() {
                return Err("Select the nodes to save first".to_string());
            }
            let min_x = nodes.iter().map(|n| n.position.x).fold(f32::INFINITY, f32::min);
            let min_y = nodes.iter().map(|n| n.position.y).fold(f32::INFINITY, f32::min);
            for n in &mut nodes {
                n.position = NodePosition { x: n.position.x - min_x, y: n.position.y - min_y };
            }
            let edges = graph
                .edges
                .iter()
                .filter(|e| wanted.contains(e.from_node.as_str()) && wanted.contains(e.to_node.as_str()))
                .cloned()
                .collect();
            NodePreset { id: uuid::Uuid::new_v4().to_string(), name: name.to_string(), nodes, edges }
        };
        let mut presets = self.presets.lock().unwrap();
        presets.push(preset.clone());
        presets.sort_by_key(|p| p.name.to_lowercase());
        self.write_presets(&presets)?;
        Ok(preset)
    }

    pub fn delete_node_preset(&self, id: &str) -> Result<(), String> {
        let mut presets = self.presets.lock().unwrap();
        let before = presets.len();
        presets.retain(|p| p.id != id);
        if presets.len() == before {
            return Err(format!("No preset {id}"));
        }
        self.write_presets(&presets)
    }

    /// Adds a copy of the preset with its top-left node at `position`, and
    /// returns the new nodes' ids.
    pub fn spawn_node_preset(&self, id: &str, position: NodePosition) -> Result<Vec<String>, String> {
        let preset = self.presets.lock().unwrap().iter().find(|p| p.id == id).cloned().ok_or_else(|| format!("No preset {id}"))?;
        self.edit(&format!("Add {}", preset.name), |inner| {
            let graph = &mut inner.show.graph;
            let mut new_ids = HashMap::new();
            for node in &preset.nodes {
                let new_id = graph.next_id();
                new_ids.insert(node.id.as_str(), new_id.clone());
                graph.nodes.push(Node {
                    id: new_id,
                    kind: node.kind,
                    position: NodePosition { x: position.x + node.position.x, y: position.y + node.position.y },
                    params: node.params.clone(),
                    frame: None,
                });
            }
            // The copies keep the preset's frames; a frame the preset
            // lacks is whichever one the copy lands in.
            for node in &preset.nodes {
                let new_id = &new_ids[node.id.as_str()];
                let inner_frame = node.frame.as_deref().and_then(|f| new_ids.get(f)).cloned();
                if let Some(n) = graph.node_mut(new_id) {
                    n.frame = inner_frame;
                }
            }
            for node in preset.nodes.iter().filter(|n| n.frame.as_deref().and_then(|f| new_ids.get(f)).is_none()) {
                graph.reframe(&new_ids[node.id.as_str()]);
            }
            for e in &preset.edges {
                if let (Some(from), Some(to)) = (new_ids.get(e.from_node.as_str()), new_ids.get(e.to_node.as_str())) {
                    graph.edges.push(Edge { from_node: from.clone(), from_socket: e.from_socket.clone(), to_node: to.clone(), to_socket: e.to_socket.clone() });
                }
            }
            tidy_any_inputs(graph);
            prune_edges(graph);
            Ok(preset.nodes.iter().map(|n| new_ids[n.id.as_str()].clone()).collect())
        })
    }

    fn write_presets(&self, presets: &[NodePreset]) -> Result<(), String> {
        match &self.store {
            Some(store) => store.write_node_presets(presets),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::graph::model::ParamValue;
    use crate::show::graph_ops::ParamChanges;
    use crate::show::service::tests::service;

    fn params(pairs: &[(&str, ParamValue)]) -> ParamChanges {
        pairs.iter().map(|(k, v)| (k.to_string(), Some(v.clone()))).collect()
    }

    #[test]
    fn a_preset_keeps_its_nodes_and_inner_links_and_spawns_fresh_copies() {
        let (service, _) = service();
        let at = |x, y| NodePosition { x, y };
        let lfo = service.add_node(NodeKind::Lfo, at(100.0, 50.0), params(&[("rate", ParamValue::Number(2.0))])).unwrap().id;
        let fan = service.add_node(NodeKind::Fan, at(0.0, 80.0), BTreeMap::new()).unwrap().id;
        let out = service.add_node(NodeKind::Output, at(300.0, 50.0), BTreeMap::new()).unwrap().id;
        let link = |from: &str, fs: &str, to: &str, ts: &str| Edge { from_node: from.into(), from_socket: fs.into(), to_node: to.into(), to_socket: ts.into() };
        service.connect(link(&fan, "phase", &lfo, "phase")).unwrap();
        service.connect(link(&lfo, "value", &out, "intensity")).unwrap();

        // The Output isn't in the preset, so neither is the link into it.
        let preset = service.save_node_preset("Wave", &[lfo.clone(), fan.clone()]).unwrap();
        assert_eq!(preset.edges.len(), 1);
        let positions: Vec<_> = preset.nodes.iter().map(|n| (n.position.x, n.position.y)).collect();
        assert_eq!(positions, vec![(100.0, 0.0), (0.0, 30.0)]);

        let spawned = service.spawn_node_preset(&preset.id, at(1000.0, 1000.0)).unwrap();
        assert_eq!(spawned.len(), 2);
        let graph = service.snapshot().graph;
        let new_lfo = graph.nodes.iter().find(|n| n.node.id == spawned[0]).unwrap();
        assert_eq!(new_lfo.node.params.get("rate"), Some(&ParamValue::Number(2.0)));
        assert_eq!((new_lfo.node.position.x, new_lfo.node.position.y), (1100.0, 1000.0));
        assert!(graph.edges.iter().any(|e| e.from_node == spawned[1] && e.to_node == spawned[0]));
        assert_eq!(graph.edges.len(), 3);

        service.undo().unwrap();
        assert_eq!(service.snapshot().graph.nodes.len(), 3);

        // Saving a frame saves what's inside it.
        let frame = service
            .add_node(NodeKind::Frame, at(-10.0, -10.0), params(&[("width", ParamValue::Number(200.0)), ("height", ParamValue::Number(200.0))]))
            .unwrap()
            .id;
        let framed = service.save_node_preset("Framed", std::slice::from_ref(&frame)).unwrap();
        let mut kinds: Vec<_> = framed.nodes.iter().map(|n| n.kind).collect();
        kinds.sort_by_key(|k| format!("{k:?}"));
        assert_eq!(kinds, vec![NodeKind::Fan, NodeKind::Frame, NodeKind::Lfo], "the Output at x 300 is outside");
        service.delete_node_preset(&preset.id).unwrap();
        assert_eq!(service.node_presets().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Framed"]);
    }
}
