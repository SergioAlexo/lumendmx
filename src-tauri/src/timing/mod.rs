//! Timing sources (BUILD_PLAN Phase 8): audio beat detection, MIDI clock,
//! MTC, LTC. They write into the engine's shared tempo / timecode / audio
//! state; the engine tick reads from there. Nothing here touches output.

pub mod analysis;
pub mod mtc;
pub mod ltc;
pub mod audio_input;
pub mod commands;
pub mod manager;
