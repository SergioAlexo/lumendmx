//! Pre-update backups and "what version was I on" tracking, the realistic
//! half of update rollback.
//!
//! `tauri-plugin-updater` installs whatever its endpoint's manifest points
//! at; it keeps no copy of the previous binary. So rollback here means:
//! (1) before installing, copy the user's data (preferences, recent list,
//! autosaves) aside so a bad build can't strand them, and (2) remember the
//! previous version so the UI can offer its installer from the releases
//! page, or, if the release owner re-points the manifest at an older build,
//! install it through the updater with downgrades allowed.

use std::fs;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

const KEEP_BACKUPS: usize = 3;
const LAST_VERSION_FILE: &str = "last_version.txt";

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Backup folders named `pre-update-...` sort chronologically; deletes all
/// but the newest `keep`.
pub fn prune_backups(dir: &Path, keep: usize) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    let mut folders: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir()).collect();
    folders.sort();
    if folders.len() > keep {
        for old in &folders[..folders.len() - keep] {
            let _ = fs::remove_dir_all(old);
        }
    }
}

/// Copies the data worth keeping into `backups/pre-update-<version>-<time>`
/// and returns the folder.
pub fn backup_user_data(data_dir: &Path, version: &str, stamp: &str) -> Result<PathBuf, String> {
    let root = data_dir.join("backups");
    let target = root.join(format!("pre-update-{stamp}-v{version}"));
    fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    for name in ["preferences.json", "recent.json"] {
        let source = data_dir.join(name);
        if source.exists() {
            fs::copy(&source, target.join(name)).map_err(|e| e.to_string())?;
        }
    }
    let autosaves = data_dir.join("autosave");
    if autosaves.exists() {
        copy_dir(&autosaves, &target.join("autosave")).map_err(|e| e.to_string())?;
    }
    prune_backups(&root, KEEP_BACKUPS);
    Ok(target)
}

/// Records `current` as the running version and returns the version that was
/// recorded before, if it differs (i.e. the app was just updated or rolled
/// back).
pub fn record_version(data_dir: &Path, current: &str) -> Option<String> {
    let path = data_dir.join(LAST_VERSION_FILE);
    let previous = fs::read_to_string(&path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let _ = fs::create_dir_all(data_dir);
    let _ = fs::write(&path, current);
    previous.filter(|p| p != current)
}

#[tauri::command]
pub fn update_prepare_backup(app: AppHandle, version: String) -> Result<String, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let target = backup_user_data(&dir, &version, &stamp)?;
    tracing::info!("pre-update backup written to {}", target.display());
    Ok(target.to_string_lossy().to_string())
}

/// The version that ran before this launch, when it differs from `current`.
#[tauri::command]
pub fn update_record_version(app: AppHandle, current: String) -> Result<Option<String>, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let previous = record_version(&dir, &current);
    if let Some(previous) = &previous {
        tracing::info!("version changed from {previous} to {current}");
    }
    Ok(previous)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_copies_prefs_recent_and_autosaves() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("preferences.json"), "{}").unwrap();
        fs::create_dir_all(dir.path().join("autosave/show-1")).unwrap();
        fs::write(dir.path().join("autosave/show-1/a.lumen"), "x").unwrap();

        let target = backup_user_data(dir.path(), "0.2.0", "20261010T000000").unwrap();
        assert!(target.join("preferences.json").exists());
        assert!(target.join("autosave/show-1/a.lumen").exists());
        assert!(!target.join("recent.json").exists());
    }

    #[test]
    fn only_the_newest_backups_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..5 {
            backup_user_data(dir.path(), "0.2.0", &format!("2026101{i}T000000")).unwrap();
        }
        let count = fs::read_dir(dir.path().join("backups")).unwrap().count();
        assert_eq!(count, KEEP_BACKUPS);
        assert!(!dir.path().join("backups/pre-update-20261010T000000-v0.2.0").exists());
    }

    #[test]
    fn version_change_is_reported_once() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(record_version(dir.path(), "0.1.0"), None); // first launch
        assert_eq!(record_version(dir.path(), "0.1.0"), None); // unchanged
        assert_eq!(record_version(dir.path(), "0.2.0"), Some("0.1.0".to_string()));
        assert_eq!(record_version(dir.path(), "0.2.0"), None);
        assert_eq!(record_version(dir.path(), "0.1.0"), Some("0.2.0".to_string())); // rollback
    }
}
