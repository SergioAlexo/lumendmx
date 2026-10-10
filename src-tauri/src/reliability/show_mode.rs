//! "Show mode": a lock that disables editing and destructive commands while a
//! show is running. It lives in Rust so it holds even if the webview is
//! reloaded or a stray call bypasses the UI; the frontend mirrors it for
//! display and disables its own controls.
//!
//! Live-operation commands (GO, faders, programmer, effects start/stop,
//! tempo) are never gated.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::State;

#[derive(Clone, Default)]
pub struct ShowLock(Arc<AtomicBool>);

impl ShowLock {
    pub fn is_locked(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    pub fn set(&self, locked: bool) {
        self.0.store(locked, Ordering::SeqCst);
    }

    /// `Err` with a user-facing message when show mode is on. `action` reads
    /// as a verb phrase: "delete a group".
    pub fn guard(&self, action: &str) -> Result<(), String> {
        if self.is_locked() {
            Err(format!("Show mode is on: can't {action}. Turn show mode off to edit."))
        } else {
            Ok(())
        }
    }
}

/// Command-line verbs that change recorded data (as opposed to live
/// programmer operation, which stays available in show mode).
pub fn command_text_is_destructive(text: &str) -> bool {
    let first = text.split_whitespace().next().unwrap_or("");
    ["RECORD", "DELETE", "UPDATE", "COPY", "MOVE", "STORE"].iter().any(|verb| first.eq_ignore_ascii_case(verb))
}

#[tauri::command]
pub fn show_mode_get(lock: State<ShowLock>) -> bool {
    lock.is_locked()
}

#[tauri::command]
pub fn show_mode_set(lock: State<ShowLock>, enabled: bool) {
    lock.set(enabled);
    tracing::info!("show mode {}", if enabled { "enabled" } else { "disabled" });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_passes_until_locked() {
        let lock = ShowLock::default();
        assert!(lock.guard("delete a group").is_ok());
        lock.set(true);
        let err = lock.guard("delete a group").unwrap_err();
        assert!(err.contains("delete a group"));
        lock.set(false);
        assert!(lock.guard("delete a group").is_ok());
    }

    #[test]
    fn clones_share_one_flag() {
        let a = ShowLock::default();
        let b = a.clone();
        a.set(true);
        assert!(b.is_locked());
    }

    #[test]
    fn destructive_command_detection() {
        assert!(command_text_is_destructive("record cue 3"));
        assert!(command_text_is_destructive("  DELETE CUE 1"));
        assert!(command_text_is_destructive("UPDATE"));
        assert!(!command_text_is_destructive("1 THRU 8 @ 50"));
        assert!(!command_text_is_destructive("CLEAR"));
        assert!(!command_text_is_destructive(""));
    }
}
