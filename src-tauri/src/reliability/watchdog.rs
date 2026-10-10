//! Watchdog: notices a stalled engine tick or a stalled output runner and
//! tells the UI. It only *observes* (an atomic heartbeat the tick thread
//! bumps, and the status snapshots the output runners already publish); it
//! never touches the output threads' timing.
//!
//! Recovery is limited to what is safe: the tick loop catches a panic in one
//! tick and carries on (see `EngineManager::start_tick`), and the watchdog
//! reports a hung tick so the operator can act. A deadlocked tick cannot be
//! interrupted from outside in safe Rust.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use ts_rs::TS;

use crate::engine::manager::EngineManager;
use crate::output::manager::OutputManager;
use crate::output::runner::UniverseStatus;

pub const HEALTH_EVENT: &str = "reliability://health";
/// The tick is ~22.7ms; this many ms without one is a stall.
pub const TICK_STALL_MS: u64 = 1_000;
/// Consecutive watchdog polls with a connected-but-silent output before it
/// counts as stalled.
pub const OUTPUT_STALL_POLLS: u32 = 3;
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// A tick slower than this is counted as slow (the budget is 22.7ms).
pub const SLOW_TICK_US: u64 = 20_000;

/// Shared between the tick thread (writer) and the watchdog (reader).
pub struct Heartbeat {
    started: Instant,
    last_tick_ms: AtomicU64,
    ticks: AtomicU64,
    last_tick_us: AtomicU64,
    max_tick_us: AtomicU64,
    slow_ticks: AtomicU64,
    panics: AtomicU64,
}

impl Heartbeat {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            last_tick_ms: AtomicU64::new(0),
            ticks: AtomicU64::new(0),
            last_tick_us: AtomicU64::new(0),
            max_tick_us: AtomicU64::new(0),
            slow_ticks: AtomicU64::new(0),
            panics: AtomicU64::new(0),
        }
    }

    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// Called by the tick thread after each completed tick.
    pub fn record_tick(&self, duration: Duration) {
        let us = duration.as_micros() as u64;
        self.last_tick_us.store(us, Ordering::Relaxed);
        self.max_tick_us.fetch_max(us, Ordering::Relaxed);
        if us > SLOW_TICK_US {
            self.slow_ticks.fetch_add(1, Ordering::Relaxed);
        }
        self.ticks.fetch_add(1, Ordering::Relaxed);
        self.last_tick_ms.store(self.now_ms(), Ordering::Release);
    }

    pub fn record_panic(&self) {
        self.panics.fetch_add(1, Ordering::Relaxed);
    }

    /// Ms since the last completed tick (since start, before the first).
    pub fn tick_age_ms(&self) -> u64 {
        self.now_ms().saturating_sub(self.last_tick_ms.load(Ordering::Acquire))
    }
}

impl Default for Heartbeat {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub enum EngineHealth {
    Ok,
    Stalled,
}

/// Pure: is a tick this old a stall?
pub fn classify_tick(age_ms: u64) -> EngineHealth {
    if age_ms > TICK_STALL_MS {
        EngineHealth::Stalled
    } else {
        EngineHealth::Ok
    }
}

/// Tracks, per universe, how many polls in a row a connected output has
/// reported ~0 frames per second.
#[derive(Default)]
pub struct OutputWatch {
    silent_polls: HashMap<u32, u32>,
}

impl OutputWatch {
    /// Returns the ids of universes whose output looks stalled.
    pub fn observe(&mut self, statuses: &[UniverseStatus]) -> Vec<u32> {
        self.silent_polls.retain(|id, _| statuses.iter().any(|s| s.universe_id == *id));
        let mut stalled = Vec::new();
        for status in statuses {
            let silent = status.connected && status.frames_per_sec < 0.5;
            let count = self.silent_polls.entry(status.universe_id).or_insert(0);
            *count = if silent { *count + 1 } else { 0 };
            if *count >= OUTPUT_STALL_POLLS {
                stalled.push(status.universe_id);
            }
        }
        stalled
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct HealthReport {
    pub engine: EngineHealth,
    pub ticks: u32,
    pub last_tick_age_ms: u32,
    pub last_tick_us: u32,
    pub max_tick_us: u32,
    pub slow_ticks: u32,
    pub tick_panics: u32,
    /// Universes whose connected output has stopped sending frames.
    pub stalled_outputs: Vec<u32>,
}

pub struct WatchdogState {
    last_report: std::sync::Mutex<Option<HealthReport>>,
}

impl WatchdogState {
    pub fn new() -> Self {
        Self { last_report: std::sync::Mutex::new(None) }
    }
}

impl Default for WatchdogState {
    fn default() -> Self {
        Self::new()
    }
}

fn build_report(heartbeat: &Heartbeat, stalled_outputs: Vec<u32>) -> HealthReport {
    let age = heartbeat.tick_age_ms();
    HealthReport {
        engine: classify_tick(age),
        ticks: heartbeat.ticks.load(Ordering::Relaxed).min(u32::MAX as u64) as u32,
        last_tick_age_ms: age.min(u32::MAX as u64) as u32,
        last_tick_us: heartbeat.last_tick_us.load(Ordering::Relaxed) as u32,
        max_tick_us: heartbeat.max_tick_us.load(Ordering::Relaxed) as u32,
        slow_ticks: heartbeat.slow_ticks.load(Ordering::Relaxed).min(u32::MAX as u64) as u32,
        tick_panics: heartbeat.panics.load(Ordering::Relaxed) as u32,
        stalled_outputs,
    }
}

/// True when the UI needs to hear about the change (health state or the set
/// of stalled outputs differs); counters changing every poll don't count.
fn health_changed(previous: &Option<HealthReport>, next: &HealthReport) -> bool {
    match previous {
        None => next.engine != EngineHealth::Ok || !next.stalled_outputs.is_empty(),
        Some(prev) => prev.engine != next.engine || prev.stalled_outputs != next.stalled_outputs,
    }
}

pub fn spawn(app: AppHandle, heartbeat: Arc<Heartbeat>) {
    std::thread::Builder::new()
        .name("watchdog".into())
        .spawn(move || {
            let mut watch = OutputWatch::default();
            let mut last: Option<HealthReport> = None;
            loop {
                std::thread::sleep(POLL_INTERVAL);
                let statuses = app.state::<OutputManager>().all_statuses();
                let report = build_report(&heartbeat, watch.observe(&statuses));
                if health_changed(&last, &report) {
                    if report.engine == EngineHealth::Stalled {
                        tracing::error!("engine tick stalled: no tick for {} ms", report.last_tick_age_ms);
                    } else if last.as_ref().is_some_and(|l| l.engine == EngineHealth::Stalled) {
                        tracing::warn!("engine tick recovered");
                    }
                    if !report.stalled_outputs.is_empty() {
                        tracing::error!("output stalled on universe(s) {:?}", report.stalled_outputs);
                    }
                    let _ = app.emit(HEALTH_EVENT, &report);
                }
                *app.state::<WatchdogState>().last_report.lock().unwrap() = Some(report.clone());
                last = Some(report);
            }
        })
        .expect("failed to start watchdog thread");
}

pub fn build_report_for_diagnostics(engine: &EngineManager, state: &WatchdogState) -> HealthReport {
    let stalled = state.last_report.lock().unwrap().as_ref().map(|r| r.stalled_outputs.clone()).unwrap_or_default();
    build_report(&engine.heartbeat(), stalled)
}

#[tauri::command]
pub fn health_report(engine: State<EngineManager>, state: State<WatchdogState>) -> HealthReport {
    build_report_for_diagnostics(&engine, &state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(id: u32, connected: bool, fps: f32) -> UniverseStatus {
        UniverseStatus { universe_id: id, connected, error: None, frames_per_sec: fps }
    }

    #[test]
    fn tick_classification_uses_the_stall_threshold() {
        assert_eq!(classify_tick(23), EngineHealth::Ok);
        assert_eq!(classify_tick(TICK_STALL_MS), EngineHealth::Ok);
        assert_eq!(classify_tick(TICK_STALL_MS + 1), EngineHealth::Stalled);
    }

    #[test]
    fn heartbeat_tracks_ticks_and_slow_ticks() {
        let hb = Heartbeat::new();
        hb.record_tick(Duration::from_micros(500));
        hb.record_tick(Duration::from_micros(25_000));
        assert_eq!(hb.ticks.load(Ordering::Relaxed), 2);
        assert_eq!(hb.slow_ticks.load(Ordering::Relaxed), 1);
        assert_eq!(hb.max_tick_us.load(Ordering::Relaxed), 25_000);
        assert!(hb.tick_age_ms() < 100);
    }

    #[test]
    fn output_needs_several_silent_polls_to_count_as_stalled() {
        let mut watch = OutputWatch::default();
        let silent = [status(1, true, 0.0)];
        assert!(watch.observe(&silent).is_empty());
        assert!(watch.observe(&silent).is_empty());
        assert_eq!(watch.observe(&silent), vec![1]);
        // A frame rate coming back clears it.
        assert!(watch.observe(&[status(1, true, 30.0)]).is_empty());
    }

    #[test]
    fn disconnected_outputs_are_not_reported_as_stalled() {
        let mut watch = OutputWatch::default();
        for _ in 0..5 {
            assert!(watch.observe(&[status(1, false, 0.0)]).is_empty());
        }
    }

    #[test]
    fn removed_universes_are_forgotten() {
        let mut watch = OutputWatch::default();
        watch.observe(&[status(1, true, 0.0), status(2, true, 0.0)]);
        watch.observe(&[status(2, true, 0.0)]);
        assert!(!watch.silent_polls.contains_key(&1));
    }

    #[test]
    fn only_state_changes_are_reported() {
        let hb = Heartbeat::new();
        hb.record_tick(Duration::from_micros(100));
        let ok = build_report(&hb, Vec::new());
        assert!(!health_changed(&None, &ok));
        assert!(!health_changed(&Some(ok.clone()), &ok));
        let mut bad = ok.clone();
        bad.stalled_outputs = vec![3];
        assert!(health_changed(&Some(ok), &bad));
    }
}
