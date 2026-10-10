//! "Export diagnostics": one zip with recent logs, the in-app message ring,
//! the current show, preferences, health and system info, for sending to
//! whoever is helping debug a problem.

use std::io::{Cursor, Write};
use std::sync::Arc;

use tauri::{AppHandle, Manager};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use super::logging::{log_dir, log_files, LogRing};
use super::prefs::PrefsStore;
use super::session::SessionGuard;
use super::watchdog::{build_report_for_diagnostics, WatchdogState};
use crate::engine::manager::EngineManager;
use crate::output::manager::OutputManager;
use crate::showfile::schema::ShowFile;

/// Zips `(name, bytes)` entries. Pure, so it can be tested without an app.
pub fn build_zip(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in files {
        writer.start_file(name.as_str(), options).map_err(|e| e.to_string())?;
        writer.write_all(bytes).map_err(|e| e.to_string())?;
    }
    Ok(writer.finish().map_err(|e| e.to_string())?.into_inner())
}

pub fn system_info(app_version: &str, unclean_shutdown: bool) -> String {
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0);
    format!(
        "LumenDMX {app_version}\nOS: {} ({})\nCPU threads: {cpus}\nPrevious shutdown unclean: {unclean_shutdown}\nGenerated: {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        chrono::Utc::now().to_rfc3339(),
    )
}

/// Asks where to save, writes the zip, and returns the path (`None` if the
/// user cancelled). `show` is the frontend's current assembled show, so the
/// export holds unsaved work too.
#[tauri::command]
pub fn diagnostics_export(app: AppHandle, show: Option<ShowFile>) -> Result<Option<String>, String> {
    let ring = app.state::<Arc<LogRing>>();
    let prefs = app.state::<PrefsStore>();
    let guard = app.state::<SessionGuard>();
    let engine = app.state::<EngineManager>();
    let watchdog = app.state::<WatchdogState>();
    let output = app.state::<OutputManager>();
    let default_name = format!("lumendmx-diagnostics-{}.zip", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    let Some(target) = rfd::FileDialog::new().add_filter("Zip archive", &["zip"]).set_file_name(default_name).save_file()
    else {
        return Ok(None);
    };

    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let version = app.package_info().version.to_string();
    files.push(("system.txt".into(), system_info(&version, guard.was_unclean()).into_bytes()));
    files.push(("messages.txt".into(), ring.to_text().into_bytes()));
    if let Ok(text) = serde_json::to_string_pretty(&prefs.get()) {
        files.push(("preferences.json".into(), text.into_bytes()));
    }
    let health = build_report_for_diagnostics(&engine, &watchdog);
    if let Ok(text) = serde_json::to_string_pretty(&health) {
        files.push(("health.json".into(), text.into_bytes()));
    }
    if let Ok(text) = serde_json::to_string_pretty(&output.all_statuses()) {
        files.push(("outputs.json".into(), text.into_bytes()));
    }
    if let Some(show) = show {
        if let Ok(text) = serde_json::to_string_pretty(&show) {
            files.push(("show.lumen".into(), text.into_bytes()));
        }
    }
    if let Ok(dir) = log_dir(&app) {
        for path in log_files(&dir) {
            if let (Some(name), Ok(bytes)) = (path.file_name(), std::fs::read(&path)) {
                files.push((format!("logs/{}", name.to_string_lossy()), bytes));
            }
        }
    }

    let bytes = build_zip(&files)?;
    super::fsutil::write_atomic(&target, &bytes)?;
    tracing::info!("diagnostics exported to {}", target.display());
    Ok(Some(target.to_string_lossy().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn zip_round_trips_its_entries() {
        let bytes = build_zip(&[
            ("a.txt".to_string(), b"hello".to_vec()),
            ("logs/b.log".to_string(), b"world".to_vec()),
        ])
        .unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(archive.len(), 2);
        let mut text = String::new();
        archive.by_name("logs/b.log").unwrap().read_to_string(&mut text).unwrap();
        assert_eq!(text, "world");
    }

    #[test]
    fn system_info_mentions_version_and_shutdown_state() {
        let info = system_info("1.2.3", true);
        assert!(info.contains("LumenDMX 1.2.3"));
        assert!(info.contains("unclean: true"));
    }
}
