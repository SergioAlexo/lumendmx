//! Editing the node graph. Every edit is undoable and recompiles the graph
//! (through `ShowService::edit`), whether it comes from the editor or an
//! MCP client.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Deserialize;
use ts_rs::TS;

use super::service::ShowService;
use crate::graph::model::{Edge, Graph, Node, NodePosition, ParamValue};
use crate::graph::registry::{node_sockets, NodeKind, MAX_ANY_INPUTS};

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
pub(super) fn prune_edges(graph: &mut Graph) {
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

/// Gives every Any node one free input after its last connected one. Inputs
/// aren't renumbered, so a socket id stays put while edges come and go.
pub(super) fn tidy_any_inputs(graph: &mut Graph) {
    for node in graph.nodes.iter_mut().filter(|n| n.kind == NodeKind::Any) {
        let last = graph
            .edges
            .iter()
            .filter(|e| e.to_node == node.id)
            .filter_map(|e| e.to_socket.strip_prefix("in")?.parse::<usize>().ok())
            .max();
        let count = last.map_or(2, |i| i + 2).clamp(2, MAX_ANY_INPUTS);
        node.params.insert("count".into(), ParamValue::Number(count as f64));
    }
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
            let mut node = Node { id: graph.next_id(), kind, position, params: BTreeMap::new(), frame: None };
            apply_params(&mut node, params);
            let id = node.id.clone();
            graph.nodes.push(node);
            graph.reframe(&id);
            if kind == NodeKind::Frame {
                graph.refit(&id);
            }
            Ok(graph.node(&id).expect("just added").clone())
        })
    }

    pub fn remove_nodes(&self, ids: &[String]) -> Result<(), String> {
        let label = if ids.len() == 1 { "Delete node".to_string() } else { format!("Delete {} nodes", ids.len()) };
        self.edit(&label, |inner| {
            let graph = &mut inner.show.graph;
            if let Some(missing) = ids.iter().find(|id| graph.node(id).is_none()) {
                return Err(format!("No node {missing}"));
            }
            graph.release_from(&ids.iter().cloned().collect());
            graph.nodes.retain(|n| !ids.contains(&n.id));
            graph.edges.retain(|e| !ids.contains(&e.from_node) && !ids.contains(&e.to_node));
            tidy_any_inputs(graph);
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
            tidy_any_inputs(graph);
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
            tidy_any_inputs(graph);
            Ok(())
        })
    }

    pub fn set_params(&self, id: &str, changes: ParamChanges) -> Result<Node, String> {
        self.edit("Edit node", |inner| {
            let graph = &mut inner.show.graph;
            let node = graph.node_mut(id).ok_or_else(|| format!("No node {id}"))?;
            let resized = node.kind == NodeKind::Frame && (changes.contains_key("width") || changes.contains_key("height"));
            apply_params(node, changes);
            if resized {
                graph.refit(id);
            }
            tidy_any_inputs(graph);
            let node = graph.node(id).expect("found above").clone();
            prune_edges(graph);
            Ok(node)
        })
    }

    pub fn move_nodes(&self, moves: &[NodeMove]) -> Result<(), String> {
        self.edit("Move nodes", |inner| {
            let graph = &mut inner.show.graph;
            for m in moves {
                let node = graph.node_mut(&m.id).ok_or_else(|| format!("No node {}", m.id))?;
                node.position = m.position;
            }
            // A node moved along with its frame stays in it; any other lands in the frame under it.
            let moved: HashSet<&str> = moves.iter().map(|m| m.id.as_str()).collect();
            for m in moves {
                let carried = graph.node(&m.id).and_then(|n| n.frame.as_deref()).is_some_and(|f| moved.contains(f) || moved.iter().any(|a| graph.is_inside(f, a)));
                if !carried {
                    graph.reframe(&m.id);
                }
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
            graph.infer_frames_if_unset();
            graph.sanitize_frames();
            tidy_any_inputs(&mut graph);
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
    fn an_any_node_keeps_one_free_input_after_its_last_link() {
        let (service, _) = service();
        let any = add(&service, NodeKind::Any);
        let (a, b) = (add(&service, NodeKind::Button), add(&service, NodeKind::Button));
        let inputs = |service: &ShowService| {
            let view = service.snapshot().graph;
            view.nodes.iter().find(|n| n.node.id == any).unwrap().inputs.iter().map(|s| s.id.clone()).collect::<Vec<_>>()
        };
        assert_eq!(inputs(&service), ["in0", "in1", "enabled"]);
        service.connect(edge(&a, "gate", &any, "in0")).unwrap();
        service.connect(edge(&b, "gate", &any, "in1")).unwrap();
        assert_eq!(inputs(&service), ["in0", "in1", "in2", "enabled"]);
        // Unplugging the first leaves a gap rather than renumbering the second.
        service.disconnect(&any, "in0").unwrap();
        assert_eq!(inputs(&service), ["in0", "in1", "in2", "enabled"]);
        assert_eq!(service.snapshot().graph.edges[0].to_socket, "in1");
        service.remove_nodes(std::slice::from_ref(&b)).unwrap();
        assert_eq!(inputs(&service), ["in0", "in1", "enabled"]);
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
        let fader = add(&service, NodeKind::Button);
        let lfo = add(&service, NodeKind::Lfo);
        service.set_control(&fader, 0.7).unwrap();
        assert!(service.set_control(&lfo, 0.7).is_err());
        assert_eq!(sink.controls.lock().unwrap()[&fader], 0.7);
    }

    fn frame(service: &ShowService, x: f32, y: f32, size: f32) -> String {
        let params = [("width", ParamValue::Number(size as f64)), ("height", ParamValue::Number(size as f64))];
        service.add_node(NodeKind::Frame, NodePosition { x, y }, params.iter().map(|(k, v)| (k.to_string(), Some(v.clone()))).collect()).unwrap().id
    }

    fn at(service: &ShowService, x: f32, y: f32) -> String {
        service.add_node(NodeKind::Number, NodePosition { x, y }, ParamChanges::new()).unwrap().id
    }

    fn parent(service: &ShowService, id: &str) -> Option<String> {
        service.snapshot().graph.nodes.iter().find(|n| n.node.id == id).unwrap().node.frame.clone()
    }

    fn collapse(service: &ShowService, id: &str) {
        service.set_params(id, [("collapsed".to_string(), Some(ParamValue::Bool(true)))].into()).unwrap();
    }

    #[test]
    fn a_node_joins_the_frame_it_is_dropped_in_and_leaves_when_dropped_outside() {
        let (service, _) = service();
        let f = frame(&service, 0.0, 0.0, 200.0);
        let n = at(&service, 50.0, 50.0);
        assert_eq!(parent(&service, &n), Some(f.clone()));
        service.move_nodes(&[NodeMove { id: n.clone(), position: NodePosition { x: 500.0, y: 500.0 } }]).unwrap();
        assert_eq!(parent(&service, &n), None);
    }

    #[test]
    fn overlapping_collapsed_frames_do_not_swap_nodes() {
        let (service, _) = service();
        let (a, b) = (frame(&service, 0.0, 0.0, 200.0), frame(&service, 1000.0, 0.0, 200.0));
        let (na, nb) = (at(&service, 50.0, 50.0), at(&service, 1050.0, 50.0));
        collapse(&service, &a);
        collapse(&service, &b);
        // Drag B, with its node, so their hidden boxes overlap.
        let moves = [
            NodeMove { id: b.clone(), position: NodePosition { x: 100.0, y: 0.0 } },
            NodeMove { id: nb.clone(), position: NodePosition { x: 150.0, y: 50.0 } },
        ];
        service.move_nodes(&moves).unwrap();
        assert_eq!(parent(&service, &na), Some(a.clone()));
        assert_eq!(parent(&service, &nb), Some(b.clone()));
        assert_eq!(parent(&service, &b), None, "B's top-left lies in A's hidden box but B isn't in A");
    }

    #[test]
    fn resizing_a_frame_takes_in_and_lets_go_of_nodes() {
        let (service, _) = service();
        let f = frame(&service, 0.0, 0.0, 100.0);
        let (inside, outside) = (at(&service, 10.0, 10.0), at(&service, 150.0, 10.0));
        assert_eq!((parent(&service, &inside), parent(&service, &outside)), (Some(f.clone()), None));
        let size = |n: f64| -> ParamChanges { [("width".to_string(), Some(ParamValue::Number(n)))].into() };
        service.set_params(&f, size(200.0)).unwrap();
        assert_eq!(parent(&service, &outside), Some(f.clone()));
        service.set_params(&f, size(50.0)).unwrap();
        assert_eq!(parent(&service, &outside), None);
        assert_eq!(parent(&service, &inside), Some(f));
    }

    #[test]
    fn removing_a_frame_hands_its_nodes_to_the_frame_around_it() {
        let (service, _) = service();
        let outer = frame(&service, 0.0, 0.0, 400.0);
        let inner = frame(&service, 10.0, 10.0, 100.0);
        let n = at(&service, 20.0, 20.0);
        assert_eq!((parent(&service, &inner), parent(&service, &n)), (Some(outer.clone()), Some(inner.clone())));
        service.remove_nodes(std::slice::from_ref(&inner)).unwrap();
        assert_eq!(parent(&service, &n), Some(outer));
    }
}
