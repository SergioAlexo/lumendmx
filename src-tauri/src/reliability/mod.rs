//! Show safety, preferences, logging and health monitoring (Phase 13).
//!
//! Everything here is observational or policy: none of it sits on the DMX
//! output path, and the FTDI thread in `output/` is untouched.

pub mod backup;
pub mod diagnostics;
pub mod fsutil;
pub mod go_guard;
pub mod logging;
pub mod prefs;
pub mod session;
pub mod show_mode;
pub mod watchdog;
pub mod window;
