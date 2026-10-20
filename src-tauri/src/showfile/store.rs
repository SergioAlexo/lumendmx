//! Where show files live on disk: reading/writing `.lumen` files, the
//! recent-shows list, autosave snapshots, and choosing what to reopen on
//! launch. Owned by `show::ShowService`; file dialogs stay in the Tauri
//! commands (they must run on the main thread).

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use super::migrate::migrate;
use super::schema::{RecentShowEntry, ShowFile};

const RECENT_LIMIT: usize = 10;
const AUTOSAVES_KEPT: usize = 10;

pub fn read_show_file(path: &Path) -> Result<ShowFile, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let raw: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("Invalid show file: {e}"))?;
    migrate(raw)
}

pub fn write_show_file(path: &Path, show: &ShowFile) -> Result<(), String> {
    let text = serde_json::to_string_pretty(show).map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, text).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

pub fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if cleaned.trim().is_empty() {
        "Untitled Show".to_string()
    } else {
        cleaned
    }
}

/// What to reopen on launch.
pub struct RestoredShow {
    pub show: ShowFile,
    /// The show's file on disk, if it has one (a recovered autosave of a
    /// never-saved show has none).
    pub path: Option<String>,
    /// True when `show` came from an autosave holding changes newer than the
    /// last open/save, i.e. it has unsaved work.
    pub recovered: bool,
}

/// The app's data directory: recent list and autosaves.
pub struct ShowStore {
    data_dir: PathBuf,
}

impl ShowStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    fn recent_file_path(&self) -> PathBuf {
        self.data_dir.join("recent.json")
    }

    pub fn recent(&self) -> Vec<RecentShowEntry> {
        let Ok(text) = fs::read_to_string(self.recent_file_path()) else {
            return Vec::new();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn touch_recent(&self, path: &str, name: &str) {
        let mut entries = self.recent();
        entries.retain(|e| e.path != path);
        entries.insert(
            0,
            RecentShowEntry { path: path.to_string(), name: name.to_string(), opened_at: Utc::now().to_rfc3339() },
        );
        entries.truncate(RECENT_LIMIT);
        let _ = fs::create_dir_all(&self.data_dir);
        if let Ok(text) = serde_json::to_string_pretty(&entries) {
            let _ = fs::write(self.recent_file_path(), text);
        }
    }

    /// Writes a timestamped snapshot into this show's autosave folder and
    /// prunes all but the newest few. Never touches the user's own file.
    pub fn autosave(&self, show: &ShowFile) -> Result<(), String> {
        let dir = self.data_dir.join("autosave").join(&show.id);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let filename = format!("autosave-{}.lumen", Utc::now().format("%Y%m%dT%H%M%S%.3f"));
        write_show_file(&dir.join(filename), show)?;
        prune_autosaves(&dir, AUTOSAVES_KEPT);
        Ok(())
    }

    /// The newest autosave across every show's autosave folder, and when it
    /// was written.
    fn newest_autosave(&self) -> Option<(PathBuf, DateTime<Utc>)> {
        fs::read_dir(self.data_dir.join("autosave"))
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

    /// Brings back the last session: the newest autosave if it's newer than
    /// the last open/save (reattached to its show file's path when that show
    /// was saved before), else the most recent show file. `None` means
    /// there's nothing (readable) to restore.
    pub fn restore_session(&self) -> Option<RestoredShow> {
        let recent = self.recent();
        let last_file = recent.iter().find(|e| Path::new(&e.path).exists());
        let autosave = self.newest_autosave();

        let from_autosave = || {
            let (path, _) = autosave.as_ref()?;
            let show = read_show_file(path).ok()?;
            let file_path = recent
                .iter()
                .find(|e| read_show_file(Path::new(&e.path)).is_ok_and(|s| s.id == show.id))
                .map(|e| e.path.clone());
            Some(RestoredShow { show, path: file_path, recovered: true })
        };
        let from_recent_file = || {
            let entry = last_file?;
            let show = read_show_file(Path::new(&entry.path)).ok()?;
            self.touch_recent(&entry.path, &show.name);
            Some(RestoredShow { show, path: Some(entry.path.clone()), recovered: false })
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
