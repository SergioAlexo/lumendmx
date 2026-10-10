//! Session continuity: an unclean-shutdown marker, and a "live session"
//! snapshot that lets a reloaded webview pick up exactly where it was.
//!
//! The engine's own data (cues, presets, running effects, output drivers)
//! already lives in Rust and survives a webview reload. What a reload loses
//! is the frontend-owned show (patch, universes, workspaces, settings), so
//! the frontend mirrors that here and re-reads it on startup instead of
//! restoring from disk over the top of the running engine.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use crate::showfile::schema::ShowFile;

const MARKER_NAME: &str = "session.lock";

/// Created at launch, removed on a clean exit. Finding it at launch means the
/// previous run ended without cleaning up: a crash, kill, power cut.
pub struct SessionGuard {
    marker: PathBuf,
    was_unclean: bool,
}

impl SessionGuard {
    pub fn begin(dir: &Path) -> Self {
        let marker = dir.join(MARKER_NAME);
        let was_unclean = marker.exists();
        let _ = fs::create_dir_all(dir);
        let _ = fs::write(&marker, format!("pid {}\nstarted {}\n", std::process::id(), Utc::now().to_rfc3339()));
        Self { marker, was_unclean }
    }

    pub fn was_unclean(&self) -> bool {
        self.was_unclean
    }

    /// Removes the marker. Safe to call more than once.
    pub fn end(&self) {
        let _ = fs::remove_file(&self.marker);
    }
}

/// What to reopen at launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionSource {
    Autosave,
    RecentFile,
}

/// Whichever was touched last wins: an autosave newer than the last
/// open/save holds unsaved work; otherwise the last show file is current.
/// After an unclean shutdown an autosave is trusted even when its clock
/// ties with the last save, since the user never got to choose.
pub fn choose_session_source(
    autosave_at: Option<DateTime<Utc>>,
    recent_opened_at: Option<DateTime<Utc>>,
    unclean_shutdown: bool,
) -> Option<SessionSource> {
    match (autosave_at, recent_opened_at) {
        (Some(autosave), Some(recent)) if autosave > recent || (unclean_shutdown && autosave == recent) => {
            Some(SessionSource::Autosave)
        }
        (_, Some(_)) => Some(SessionSource::RecentFile),
        (Some(_), None) => Some(SessionSource::Autosave),
        (None, None) => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct LiveSession {
    pub show: ShowFile,
    pub path: Option<String>,
    pub dirty: bool,
}

#[derive(Default)]
pub struct LiveSessionStore(Mutex<Option<LiveSession>>);

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct SessionStatus {
    /// The previous run did not shut down cleanly.
    pub unclean_shutdown: bool,
}

#[tauri::command]
pub fn session_status(guard: State<SessionGuard>) -> SessionStatus {
    SessionStatus { unclean_shutdown: guard.was_unclean() }
}

/// The frontend calls this just before a deliberate relaunch (updater), which
/// may not run the normal exit hook.
#[tauri::command]
pub fn session_clean_shutdown(guard: State<SessionGuard>) {
    guard.end();
}

#[tauri::command]
pub fn session_set_live(store: State<LiveSessionStore>, session: LiveSession) {
    *store.0.lock().unwrap() = Some(session);
}

/// The snapshot a previous webview left behind, or `None` on a cold start.
#[tauri::command]
pub fn session_get_live(store: State<LiveSessionStore>) -> Option<LiveSession> {
    store.0.lock().unwrap().clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(hour: u32) -> Option<DateTime<Utc>> {
        Some(Utc.with_ymd_and_hms(2026, 9, 30, hour, 0, 0).unwrap())
    }

    #[test]
    fn newer_autosave_beats_the_last_opened_file() {
        assert_eq!(choose_session_source(at(12), at(11), false), Some(SessionSource::Autosave));
    }

    #[test]
    fn a_save_or_open_after_the_last_autosave_wins() {
        assert_eq!(choose_session_source(at(11), at(12), false), Some(SessionSource::RecentFile));
        assert_eq!(choose_session_source(at(11), at(12), true), Some(SessionSource::RecentFile));
    }

    #[test]
    fn falls_back_to_whichever_exists() {
        assert_eq!(choose_session_source(None, at(12), false), Some(SessionSource::RecentFile));
        assert_eq!(choose_session_source(at(12), None, false), Some(SessionSource::Autosave));
        assert_eq!(choose_session_source(None, None, true), None);
    }

    #[test]
    fn a_tie_after_a_crash_prefers_the_autosave() {
        assert_eq!(choose_session_source(at(12), at(12), true), Some(SessionSource::Autosave));
        assert_eq!(choose_session_source(at(12), at(12), false), Some(SessionSource::RecentFile));
    }

    #[test]
    fn marker_detects_an_unclean_previous_run() {
        let dir = tempfile::tempdir().unwrap();
        let first = SessionGuard::begin(dir.path());
        assert!(!first.was_unclean());
        // No end(): simulates a crash.
        let second = SessionGuard::begin(dir.path());
        assert!(second.was_unclean());
        second.end();
        second.end();
        assert!(!SessionGuard::begin(dir.path()).was_unclean());
    }
}
