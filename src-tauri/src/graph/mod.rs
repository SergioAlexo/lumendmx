//! The node graph: what the show plays. `model` is the stored graph,
//! `registry` the node types, `compile` turns a graph into a plan, `eval`
//! runs that plan each engine tick and `write` maps values onto fixtures.

pub mod compile;
pub mod eval;
pub mod gradient;
pub mod model;
pub mod registry;
pub mod write;
