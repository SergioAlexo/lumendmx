//! Editing the node graph. Every edit is undoable and recompiles the graph
//! (through `ShowService::edit`), whether it comes from the editor or an
//! MCP client.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Deserialize;
use ts_rs::TS;

use super::service::ShowService;
use crate::graph::model::{Edge, Graph, Node, NodePosition, ParamValue};
use crate::graph::registry::{node_sockets, NodeKind};

/// A param change: a value sets it, `null` resets it to the default.
pub type ParamChanges = BTreeMap<String, Option<ParamValue>>;

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct NodeMove {
    pub id: String,
    pub position: NodePosition,
}

/// Whether `graph` has a path of edges from `from` to `to`.
fn reaches(graph: &Graph, from: &str, to: &str) -> bool {
    let mut stack = vec![from];
    let mut seen = HashSet::new();
    while let Some(n) = stack.pop() {
        if n == to {
            return true;
        }
        if seen.insert(n) {
            stack.extend(graph.edges.iter().filter(|e| e.from_node == n).map(|e| e.to_node.as_str()));
        }
    }
    false
}

/// Drops edges whose sockets no longer exist (a Switch lost inputs, an
/// Output dropped an attribute).
fn prune_edges(graph: &mut Graph) {
    let sockets: HashMap<&str, (HashSet<String>, HashSet<String>)> = graph
        .nodes
        .iter()
        .map(|n| {
            let (inputs, outputs) = node_sockets(n.kind, &n.params);
            (n.id.as_str(), (inputs.into_iter().map(|s| s.id).collect(), outputs.into_iter().map(|s| s.id).collect()))
        })
        .collect();
    graph.edges.retain(|e| {
        sockets.get(e.from_node.as_str()).is_some_and(|(_, outs)| outs.contains(&e.from_socket))
            && sockets.get(e.to_node.as_str()).is_some_and(|(ins, _)| ins.contains(&e.to_socket))
    });
}

fn apply_params(node: &mut Node, changes: ParamChanges) {
    for (id, value) in changes {
        match value {
            Some(v) => {
                node.params.insert(id, v);
            }
            None => {
                node.params.remove(&id);
            }
        }
    }
}

impl ShowService {
    pub fn add_node(&self, kind: NodeKind, position: NodePosition, params: ParamChanges) -> Result<Node, String> {
        self.edit("Add node", |inner| {
            let graph = &mut inner.show.graph;
            let mut node = Node { id: graph.next_id(), kind, position, params: BTreeMap::new() };
            apply_params(&mut node, params);
            graph.nodes.push(node.clone());
            Ok(node)
        })
    }

    pub fn remove_nodes(&self, ids: &[String]) -> Result<(), String> {
        let label = if ids.len() == 1 { "Delete node".to_string() } else { format!("Delete {} nodes", ids.len()) };
        self.edit(&label, |inner| {
            let graph = &mut inner.show.graph;
            if let Some(missing) = ids.iter().find(|id| graph.node(id).is_none()) {
                return Err(format!("No node {missing}"));
            }
            graph.nodes.retain(|n| !ids.contains(&n.id));
            graph.edges.retain(|e| !ids.contains(&e.from_node) && !ids.contains(&e.to_node));
            Ok(())
        })
    }

    /// Links an output socket to an input socket, replacing whatever fed
    /// that input before.
    pub fn connect(&self, edge: Edge) -> Result<(), String> {
        self.edit("Connect", |inner| {
            let graph = &mut inner.show.graph;
            let from = graph.node(&edge.from_node).ok_or_else(|| format!("No node {}", edge.from_node))?;
            let to = graph.node(&edge.to_node).ok_or_else(|| format!("No node {}", edge.to_node))?;
            let (_, outputs) = node_sockets(from.kind, &from.params);
            let (inputs, _) = node_sockets(to.kind, &to.params);
            let out = outputs
                .iter()
                .find(|s| s.id == edge.from_socket)
                .ok_or_else(|| format!("{} has no output \"{}\"", edge.from_node, edge.from_socket))?;
            let input = inputs
                .iter()
                .find(|s| s.id == edge.to_socket)
                .ok_or_else(|| format!("{} has no input \"{}\"", edge.to_node, edge.to_socket))?;
            if !out.ty.feeds(input.ty) {
                return Err(format!("A {:?} output can't feed a {:?} input", out.ty, input.ty));
            }
            if edge.from_node == edge.to_node || reaches(graph, &edge.to_node, &edge.from_node) {
                return Err("That link would make a loop".to_string());
            }
            graph.edges.retain(|e| !(e.to_node == edge.to_node && e.to_socket == edge.to_socket));
            graph.edges.push(edge.clone());
            Ok(())
        })
    }

    pub fn disconnect(&self, to_node: &str, to_socket: &str) -> Result<(), String> {
        self.edit("Disconnect", |inner| {
            let graph = &mut inner.show.graph;
            let before = graph.edges.len();
            graph.edges.retain(|e| !(e.to_node == to_node && e.to_socket == to_socket));
            if graph.edges.len() == before {
                return Err(format!("Nothing is connected to {to_node}.{to_socket}"));
            }
            Ok(())
        })
    }

    pub fn set_params(&self, id: &str, changes: ParamChanges) -> Result<Node, String> {
        self.edit("Edit node", |inner| {
            let graph = &mut inner.show.graph;
            let node = graph.node_mut(id).ok_or_else(|| format!("No node {id}"))?;
            apply_params(node, changes);
            let node = node.clone();
            prune_edges(graph);
            Ok(node)
        })
    }

    pub fn move_nodes(&self, moves: &[NodeMove]) -> Result<(), String> {
        self.edit("Move nodes", |inner| {
            for m in moves {
                let node = inner.show.graph.node_mut(&m.id).ok_or_else(|| format!("No node {}", m.id))?;
                node.position = m.position;
            }
            Ok(())
        })
    }

    /// Replaces the whole graph (an MCP client writing a full graph at
    /// once). Node ids must be unique; edges to missing sockets are dropped.
    pub fn replace_graph(&self, mut graph: Graph) -> Result<(), String> {
        self.edit("Replace graph", |inner| {
            let mut seen = HashSet::new();
            if let Some(dup) = graph.nodes.iter().find(|n| !seen.insert(n.id.as_str())) {
                return Err(format!("Node id {} is used twice", dup.id));
            }
            prune_edges(&mut graph);
            inner.show.graph = graph;
            Ok(())
        })
    }

    /// A playback-bar Fader or Button value. Live, so not an undoable edit.
    pub fn set_control(&self, id: &str, value: f32) -> Result<(), String> {
        let inner = self.inner.lock().unwrap();
        let node = inner.show.graph.node(id).ok_or_else(|| format!("No node {id}"))?;
        if !matches!(node.kind, NodeKind::Fader | NodeKind::Button) {
            return Err(format!("{id} isn't a Fader or Button"));
        }
        self.sink.set_control(id, value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::show::service::tests::service;

    fn edge(from: &str, fs: &str, to: &str, ts: &str) -> Edge {
        Edge { from_node: from.into(), from_socket: fs.into(), to_node: to.into(), to_socket: ts.into() }
    }

    fn add(service: &ShowService, kind: NodeKind) -> String {
        service.add_node(kind, NodePosition::default(), BTreeMap::new()).unwrap().id
    }

    #[test]
    fn connecting_checks_types_replaces_inputs_and_refuses_loops() {
        let (service, _) = service();
        let a = add(&service, NodeKind::Math);
        let b = add(&service, NodeKind::Math);
        let shape = add(&service, NodeKind::Shape);
        service.connect(edge(&a, "value", &b, "a")).unwrap();
        assert!(service.connect(edge(&b, "value", &a, "a")).unwrap_err().contains("loop"));
        assert!(service.connect(edge(&shape, "position", &a, "b")).unwrap_err().contains("can't feed"));
        let c = add(&service, NodeKind::Number);
        service.connect(edge(&c, "value", &b, "a")).unwrap();
        let edges = service.snapshot().graph.edges;
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from_node, c);
    }

    #[test]
    fn shrinking_a_switch_drops_links_to_its_removed_inputs() {
        let (service, _) = service();
        let mut params = ParamChanges::new();
        params.insert("count".into(), Some(ParamValue::Number(3.0)));
        let switch = service.add_node(NodeKind::Switch, NodePosition::default(), params).unwrap().id;
        let color = add(&service, NodeKind::Color);
        service.connect(edge(&color, "color", &switch, "in2")).unwrap();
        let mut shrink = ParamChanges::new();
        shrink.insert("count".into(), None); // back to the default of 2
        service.set_params(&switch, shrink).unwrap();
        assert!(service.snapshot().graph.edges.is_empty());
    }

    #[test]
    fn graph_edits_undo_and_compile_issues_show_on_nodes() {
        let (service, sink) = service();
        let out = add(&service, NodeKind::Output);
        let mut params = ParamChanges::new();
        params.insert("fixtures".into(), Some(ParamValue::Text("1-2".into())));
        service.set_params(&out, params).unwrap();
        let view = service.snapshot().graph;
        assert_eq!(view.nodes[0].issues, vec!["Fixtures 1, 2 not patched".to_string()]);
        assert_eq!(sink.plan.lock().unwrap().outputs.len(), 1);

        service.undo().unwrap();
        service.undo().unwrap();
        assert!(service.snapshot().graph.nodes.is_empty());
        assert!(sink.plan.lock().unwrap().outputs.is_empty());
    }

    #[test]
    fn controls_only_exist_for_faders_and_buttons() {
        let (service, sink) = service();
        let fader = add(&service, NodeKind::Fader);
        let lfo = add(&service, NodeKind::Lfo);
        service.set_control(&fader, 0.7).unwrap();
        assert!(service.set_control(&lfo, 0.7).is_err());
        assert_eq!(sink.controls.lock().unwrap()[&fader], 0.7);
    }
}
