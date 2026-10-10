//! Tauri commands for New/Open/Save/Save As/Autosave/Recent, plus restoring
//! the last session on launch.
//!
//! File dialogs run natively in Rust via `rfd` rather than through a JS-facing
//! plugin, so no extra Tauri capability grants are needed for them.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use tauri::{AppHandle, Manager};

use super::migrate::migrate;
use super::schema::{RecentShowEntry, RestoredSession, ShowFile, CURRENT_SCHEMA_VERSION};

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

fn recent_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app_data_dir(app)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("recent.json"))
}

fn read_recent(app: &AppHandle) -> Vec<RecentShowEntry> {
    let Ok(path) = recent_file_path(app) else {
        return Vec::new();
    };
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_recent(app: &AppHandle, entries: &[RecentShowEntry]) {
    if let Ok(path) = recent_file_path(app) {
        if let Ok(text) = serde_json::to_string_pretty(entries) {
            let _ = fs::write(path, text);
        }
    }
}

fn touch_recent(app: &AppHandle, path: &str, name: &str) {
    let mut entries = read_recent(app);
    entries.retain(|e| e.path != path);
    entries.insert(
        0,
        RecentShowEntry {
            path: path.to_string(),
            name: name.to_string(),
            opened_at: chrono::Utc::now().to_rfc3339(),
        },
    );
    entries.truncate(10);
    write_recent(app, &entries);
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.trim().is_empty() {
        "Untitled Show".to_string()
    } else {
        cleaned
    }
}

#[tauri::command]
pub fn showfile_new() -> ShowFile {
    ShowFile::new_default("Untitled Show")
}

#[tauri::command]
pub fn showfile_open(app: AppHandle, path: Option<String>) -> Result<(ShowFile, String), String> {
    let chosen = match path {
        Some(p) => Some(PathBuf::from(p)),
        None => rfd::FileDialog::new()
            .add_filter("LumenDMX show", &["lumen"])
            .pick_file(),
    };
    let Some(chosen) = chosen else {
        return Err("No file selected".to_string());
    };

    let show = read_show_file(&chosen)?;

    let path_str = chosen.to_string_lossy().to_string();
    touch_recent(&app, &path_str, &show.name);
    Ok((show, path_str))
}

/// Writes `show` to `path` (Save), or prompts for a location first when `path`
/// is `None` (Save As / first save). Returns the path actually used and the
/// show with `schemaVersion`/`modifiedAt` refreshed, so the frontend's copy
/// stays in sync.
#[tauri::command]
pub fn showfile_save(
    app: AppHandle,
    mut show: ShowFile,
    path: Option<String>,
) -> Result<(String, ShowFile), String> {
    let chosen = match path {
        Some(p) => PathBuf::from(p),
        None => rfd::FileDialog::new()
            .add_filter("LumenDMX show", &["lumen"])
            .set_file_name(format!("{}.lumen", sanitize_filename(&show.name)))
            .save_file()
            .ok_or_else(|| "Save cancelled".to_string())?,
    };

    show.schema_version = CURRENT_SCHEMA_VERSION;
    show.modified_at = chrono::Utc::now().to_rfc3339();

    write_show_file(&chosen, &show)?;

    let path_str = chosen.to_string_lossy().to_string();
    touch_recent(&app, &path_str, &show.name);
    Ok((path_str, show))
}

fn read_show_file(path: &Path) -> Result<ShowFile, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let raw: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("Invalid show file: {e}"))?;
    migrate(raw)
}

fn write_show_file(path: &Path, show: &ShowFile) -> Result<(), String> {
    let text = serde_json::to_string_pretty(show).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, text).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

/// Writes a timestamped snapshot into the app data dir's autosave folder for
/// this show and prunes anything past the last 10. Never touches the user's
/// own save file.
#[tauri::command]
pub fn showfile_autosave(app: AppHandle, show: ShowFile) -> Result<(), String> {
    let dir = app_data_dir(&app)?.join("autosave").join(&show.id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let filename = format!(
        "autosave-{}.lumen",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%.3f")
    );
    write_show_file(&dir.join(filename), &show)?;
    prune_autosaves(&dir, 10);
    Ok(())
}

fn prune_autosaves(dir: &Path, keep: usize) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<PathBuf> = read_dir
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "lumen"))
        .collect();
    files.sort();
    if files.len() > keep {
        for old in &files[..files.len() - keep] {
            let _ = fs::remove_file(old);
        }
    }
}

#[tauri::command]
pub fn showfile_recent(app: AppHandle) -> Vec<RecentShowEntry> {
    read_recent(&app)
}

/// The newest autosave across every show's autosave folder, and when it was
/// written.
fn newest_autosave(app: &AppHandle) -> Option<(PathBuf, DateTime<Utc>)> {
    let root = app_data_dir(app).ok()?.join("autosave");
    fs::read_dir(root)
        .ok()?
        .filter_map(|dir| fs::read_dir(dir.ok()?.path()).ok())
        .flatten()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().is_none_or(|e| e != "lumen") {
                return None;
            }
            let modified = fs::metadata(&path).ok()?.modified().ok()?;
            Some((path, DateTime::<Utc>::from(modified)))
        })
        .max_by_key(|(_, at)| *at)
}

#[derive(Debug, PartialEq)]
enum SessionSource {
    Autosave,
    RecentFile,
}

/// Whichever was touched last wins: an autosave newer than the last
/// open/save holds unsaved work; otherwise the last show file is current.
fn choose_session_source(
    autosave_at: Option<DateTime<Utc>>,
    recent_opened_at: Option<DateTime<Utc>>,
) -> Option<SessionSource> {
    match (autosave_at, recent_opened_at) {
        (Some(autosave), Some(recent)) if autosave > recent => Some(SessionSource::Autosave),
        (_, Some(_)) => Some(SessionSource::RecentFile),
        (Some(_), None) => Some(SessionSource::Autosave),
        (None, None) => None,
    }
}

/// Brings back the last session on launch: the newest autosave if it's
/// newer than the last open/save (reattached to its show file's path when
/// that show was saved before), else the most recent show file. `None`
/// means there's nothing to restore and the caller should start a new show.
#[tauri::command]
pub fn showfile_restore_session(app: AppHandle) -> Option<RestoredSession> {
    let recent = read_recent(&app);
    let last_file = recent.iter().find(|e| Path::new(&e.path).exists());
    let autosave = newest_autosave(&app);

    let from_autosave = || {
        let (path, _) = autosave.as_ref()?;
        let show = read_show_file(path).ok()?;
        let file_path = recent
            .iter()
            .find(|e| read_show_file(Path::new(&e.path)).is_ok_and(|s| s.id == show.id))
            .map(|e| e.path.clone());
        Some(RestoredSession { show, path: file_path, recovered: true })
    };
    let from_recent_file = || {
        let entry = last_file?;
        let show = read_show_file(Path::new(&entry.path)).ok()?;
        touch_recent(&app, &entry.path, &show.name);
        Some(RestoredSession { show, path: Some(entry.path.clone()), recovered: false })
    };

    // An unparseable timestamp still counts as "there is a recent file",
    // just the oldest possible one.
    let recent_opened_at = last_file.map(|e| {
        DateTime::parse_from_rfc3339(&e.opened_at).map_or(DateTime::<Utc>::MIN_UTC, |t| t.with_timezone(&Utc))
    });
    match choose_session_source(autosave.as_ref().map(|(_, at)| *at), recent_opened_at)? {
        SessionSource::Autosave => from_autosave().or_else(from_recent_file),
        SessionSource::RecentFile => from_recent_file().or_else(from_autosave),
    }
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
        assert_eq!(choose_session_source(at(12), at(11)), Some(SessionSource::Autosave));
    }

    #[test]
    fn a_save_or_open_after_the_last_autosave_wins() {
        assert_eq!(choose_session_source(at(11), at(12)), Some(SessionSource::RecentFile));
    }

    #[test]
    fn falls_back_to_whichever_exists() {
        assert_eq!(choose_session_source(None, at(12)), Some(SessionSource::RecentFile));
        assert_eq!(choose_session_source(at(12), None), Some(SessionSource::Autosave));
        assert_eq!(choose_session_source(None, None), None);
    }
}
