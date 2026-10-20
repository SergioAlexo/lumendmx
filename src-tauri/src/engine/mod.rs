//! The lighting engine core: HTP/LTP merge, the global clocks, and the tick
//! that sends merged frames to the outputs. See docs/ENGINE.md.

pub mod audio;
pub mod commands;
pub mod effects;
pub mod groups;
pub mod manager;
pub mod merge;
pub mod state;
pub mod tempo;
pub mod timecode;
