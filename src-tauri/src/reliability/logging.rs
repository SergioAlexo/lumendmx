//! File logging with daily rotation, plus an in-memory ring of recent
//! messages for the in-app Messages window.
//!
//! `tracing-appender` rotates by day (not by size) and prunes to the newest
//! `MAX_LOG_FILES` files, which bounds disk use for a console that may run for
//! weeks. The ring keeps the last `RING_CAPACITY` events and pushes each new
//! one to the UI as `log://message`.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;
use ts_rs::TS;

pub const LOG_EVENT: &str = "log://message";
pub const MAX_LOG_FILES: usize = 7;
const RING_CAPACITY: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct LogEntry {
    pub id: u32,
    pub timestamp: String,
    /// "error", "warn", "info", "debug" or "trace".
    pub level: String,
    /// Where it came from: a Rust module path, or "ui" for the frontend.
    pub source: String,
    pub message: String,
}

/// Bounded buffer of recent entries.
pub struct LogRing {
    entries: Mutex<VecDeque<LogEntry>>,
    next_id: AtomicU64,
    app: OnceLock<AppHandle>,
}

impl LogRing {
    pub fn new() -> Self {
        Self { entries: Mutex::new(VecDeque::new()), next_id: AtomicU64::new(1), app: OnceLock::new() }
    }

    pub fn push(&self, level: &str, source: &str, message: String) -> LogEntry {
        let entry = LogEntry {
            id: self.next_id.fetch_add(1, Ordering::Relaxed) as u32,
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            level: level.to_string(),
            source: source.to_string(),
            message,
        };
        {
            let mut entries = self.entries.lock().unwrap();
            if entries.len() >= RING_CAPACITY {
                entries.pop_front();
            }
            entries.push_back(entry.clone());
        }
        if let Some(app) = self.app.get() {
            let _ = app.emit(LOG_EVENT, &entry);
        }
        entry
    }

    pub fn recent(&self) -> Vec<LogEntry> {
        self.entries.lock().unwrap().iter().cloned().collect()
    }

    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }

    /// Plain-text dump, one line per entry, for diagnostics.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for e in self.recent() {
            let _ = writeln!(out, "{} {:5} {} {}", e.timestamp, e.level.to_uppercase(), e.source, e.message);
        }
        out
    }

    pub fn attach(&self, app: AppHandle) {
        let _ = self.app.set(app);
    }
}

impl Default for LogRing {
    fn default() -> Self {
        Self::new()
    }
}

struct MessageVisitor {
    message: String,
    fields: String,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            let _ = write!(self.fields, " {}={:?}", field.name(), value);
        }
    }
}

struct RingLayer(Arc<LogRing>);

impl<S: Subscriber> Layer<S> for RingLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // Debug/trace chatter stays out of the UI ring (it still goes to the file).
        if *event.metadata().level() > Level::INFO {
            return;
        }
        let mut visitor = MessageVisitor { message: String::new(), fields: String::new() };
        event.record(&mut visitor);
        let level = match *event.metadata().level() {
            Level::ERROR => "error",
            Level::WARN => "warn",
            _ => "info",
        };
        self.0.push(level, event.metadata().target(), visitor.message + &visitor.fields);
    }
}

/// Keeps the file writer's background thread alive; dropping it flushes.
pub struct LogGuard {
    _worker: WorkerGuard,
}

pub fn log_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("logs"))
}

/// Installs the global subscriber. Returns `None` if one is already set or
/// the log directory can't be created (the app still runs, unlogged to file).
pub fn init(dir: &Path, ring: Arc<LogRing>) -> Option<LogGuard> {
    std::fs::create_dir_all(dir).ok()?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("lumendmx")
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(dir)
        .ok()?;
    let (writer, worker) = tracing_appender::non_blocking(appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::rfc_3339());
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::rfc_3339());
    let filter = tracing_subscriber::filter::LevelFilter::DEBUG;

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stderr_layer)
        .with(RingLayer(ring))
        .try_init()
        .ok()?;

    // Panics land in the log too, with a location, before the default hook runs.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("panic: {info}");
        previous(info);
    }));

    Some(LogGuard { _worker: worker })
}

/// All current log files, oldest first.
pub fn log_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("lumendmx") && n.contains(".log")))
        .collect();
    files.sort();
    files
}

#[tauri::command]
pub fn log_recent(ring: State<Arc<LogRing>>) -> Vec<LogEntry> {
    ring.recent()
}

#[tauri::command]
pub fn log_clear(ring: State<Arc<LogRing>>) {
    ring.clear();
}

/// Frontend errors and warnings, forwarded so they reach the log file and
/// the Messages window like everything else.
#[tauri::command]
pub fn log_write(level: String, source: String, message: String) {
    let message = message.chars().take(4000).collect::<String>();
    match level.as_str() {
        "error" => tracing::error!(target: "ui", "{source}: {message}"),
        "warn" => tracing::warn!(target: "ui", "{source}: {message}"),
        _ => tracing::info!(target: "ui", "{source}: {message}"),
    }
}

#[tauri::command]
pub fn log_dir_path(app: AppHandle) -> Result<String, String> {
    Ok(log_dir(&app)?.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_keeps_only_the_newest_entries() {
        let ring = LogRing::new();
        for i in 0..(RING_CAPACITY + 10) {
            ring.push("info", "test", format!("m{i}"));
        }
        let recent = ring.recent();
        assert_eq!(recent.len(), RING_CAPACITY);
        assert_eq!(recent[0].message, "m10");
        assert_eq!(recent.last().unwrap().message, format!("m{}", RING_CAPACITY + 9));
    }

    #[test]
    fn ids_increase_and_clear_empties() {
        let ring = LogRing::new();
        let a = ring.push("warn", "x", "a".into());
        let b = ring.push("error", "x", "b".into());
        assert!(b.id > a.id);
        assert!(ring.to_text().contains("ERROR x b"));
        ring.clear();
        assert!(ring.recent().is_empty());
    }

    #[test]
    fn log_files_lists_only_ours_oldest_first() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["lumendmx.2026-10-02.log", "lumendmx.2026-10-01.log", "other.txt"] {
            std::fs::write(dir.path().join(name), "x").unwrap();
        }
        let names: Vec<String> = log_files(dir.path())
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, ["lumendmx.2026-10-01.log", "lumendmx.2026-10-02.log"]);
    }
}
