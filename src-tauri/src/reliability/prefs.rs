//! Machine-level preferences, kept in `preferences.json` in the app data dir.
//! They describe this computer and operator (UI scale, window placement,
//! default rates), not a show, so they are deliberately not in the show file.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::State;
use ts_rs::TS;

use super::fsutil::write_atomic;
use crate::engine::manager::EngineManager;
use crate::output::config::{DEFAULT_NETWORK_RATE_HZ, DEFAULT_SERIAL_RATE_HZ};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct WindowPlacement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct Preferences {
    /// Refresh rate a newly added serial driver (FTDI, Enttec Pro) starts with.
    pub serial_rate_hz: u32,
    /// Refresh rate a newly added network driver (Art-Net, sACN) starts with.
    pub network_rate_hz: u32,
    /// How often the UI receives live DMX frames (the engine tick is fixed).
    pub display_hz: u32,
    /// Fade times a new cue starts with in the record form.
    pub default_fade_in_ms: u32,
    pub default_fade_out_ms: u32,
    /// A GO within this many ms of the previous one on the same playback is
    /// ignored (guards against double-presses). 0 turns it off.
    pub go_debounce_ms: u32,
    /// GO on a paused playback resumes it first, instead of stepping while
    /// still paused.
    pub go_resumes_paused: bool,
    /// Ask before deleting recorded data.
    pub confirm_on_delete: bool,
    /// Seconds between autosaves of a modified show.
    pub autosave_interval_s: u32,
    /// UI zoom, 0.75 to 1.5.
    pub ui_scale: f32,
    /// Larger hit targets for touchscreens.
    pub touch_mode: bool,
    /// Last main-window position, restored on launch when still on screen.
    pub window: Option<WindowPlacement>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            serial_rate_hz: DEFAULT_SERIAL_RATE_HZ,
            network_rate_hz: DEFAULT_NETWORK_RATE_HZ,
            display_hz: 20,
            default_fade_in_ms: 1000,
            default_fade_out_ms: 1000,
            go_debounce_ms: 0,
            go_resumes_paused: false,
            confirm_on_delete: true,
            autosave_interval_s: 60,
            ui_scale: 1.0,
            touch_mode: false,
            window: None,
        }
    }
}

impl Preferences {
    /// Clamps every field to its supported range, so a hand-edited or
    /// corrupt file can't produce nonsense (a 0 Hz output, a 10x zoom).
    pub fn sanitized(mut self) -> Self {
        self.serial_rate_hz = self.serial_rate_hz.clamp(1, 44);
        self.network_rate_hz = self.network_rate_hz.clamp(1, 44);
        self.display_hz = self.display_hz.clamp(5, 60);
        self.default_fade_in_ms = self.default_fade_in_ms.min(600_000);
        self.default_fade_out_ms = self.default_fade_out_ms.min(600_000);
        self.go_debounce_ms = self.go_debounce_ms.min(5_000);
        self.autosave_interval_s = self.autosave_interval_s.clamp(10, 3_600);
        self.ui_scale = if self.ui_scale.is_finite() { self.ui_scale.clamp(0.75, 1.5) } else { 1.0 };
        self
    }

    /// Parses stored JSON; anything unreadable falls back to defaults, and
    /// missing fields take their default individually.
    pub fn from_json(text: &str) -> Self {
        serde_json::from_str::<Preferences>(text).map(Preferences::sanitized).unwrap_or_default()
    }
}

pub struct PrefsStore {
    path: PathBuf,
    current: Mutex<Preferences>,
}

impl PrefsStore {
    pub fn load(path: PathBuf) -> Self {
        let current = match fs::read_to_string(&path) {
            Ok(text) => Preferences::from_json(&text),
            Err(_) => Preferences::default(),
        };
        Self { path, current: Mutex::new(current) }
    }

    pub fn get(&self) -> Preferences {
        self.current.lock().unwrap().clone()
    }

    pub fn set(&self, prefs: Preferences) -> Result<Preferences, String> {
        let prefs = prefs.sanitized();
        let text = serde_json::to_string_pretty(&prefs).map_err(|e| e.to_string())?;
        write_atomic(&self.path, text.as_bytes())?;
        *self.current.lock().unwrap() = prefs.clone();
        Ok(prefs)
    }

    /// Updates just the saved window placement (called on close).
    pub fn set_window(&self, window: Option<WindowPlacement>) {
        let mut prefs = self.get();
        prefs.window = window;
        let _ = self.set(prefs);
    }
}

/// Pushes the preferences the engine reads live into it.
pub fn apply_to_engine(engine: &EngineManager, prefs: &Preferences) {
    let tuning = engine.tuning();
    tuning.display_hz.store(prefs.display_hz, Ordering::Relaxed);
    tuning.go_debounce_ms.store(prefs.go_debounce_ms, Ordering::Relaxed);
    tuning.go_resumes_paused.store(prefs.go_resumes_paused, Ordering::Relaxed);
}

#[tauri::command]
pub fn prefs_get(store: State<PrefsStore>) -> Preferences {
    store.get()
}

#[tauri::command]
pub fn prefs_set(
    store: State<PrefsStore>,
    engine: State<EngineManager>,
    prefs: Preferences,
) -> Result<Preferences, String> {
    let saved = store.set(prefs)?;
    apply_to_engine(&engine, &saved);
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_survive_a_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = PrefsStore::load(dir.path().join("preferences.json"));
        assert_eq!(store.get(), Preferences::default());
        let mut prefs = store.get();
        prefs.ui_scale = 1.25;
        prefs.touch_mode = true;
        store.set(prefs).unwrap();
        let reloaded = PrefsStore::load(dir.path().join("preferences.json"));
        assert_eq!(reloaded.get().ui_scale, 1.25);
        assert!(reloaded.get().touch_mode);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let prefs = Preferences { ui_scale: 9.0, display_hz: 0, serial_rate_hz: 500, ..Default::default() }.sanitized();
        assert_eq!(prefs.ui_scale, 1.5);
        assert_eq!(prefs.display_hz, 5);
        assert_eq!(prefs.serial_rate_hz, 44);
        assert_eq!(Preferences { ui_scale: f32::NAN, ..Default::default() }.sanitized().ui_scale, 1.0);
    }

    #[test]
    fn corrupt_or_partial_files_fall_back_per_field() {
        assert_eq!(Preferences::from_json("not json"), Preferences::default());
        let partial = Preferences::from_json(r#"{"touchMode": true}"#);
        assert!(partial.touch_mode);
        assert_eq!(partial.display_hz, 20);
    }

    #[test]
    fn window_placement_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        let store = PrefsStore::load(path.clone());
        let placement = WindowPlacement { x: -1920, y: 40, width: 1400, height: 900, maximized: false };
        store.set_window(Some(placement.clone()));
        assert_eq!(PrefsStore::load(path).get().window, Some(placement));
    }
}
