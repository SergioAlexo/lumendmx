import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  HealthReport,
  LiveSession,
  LogEntry,
  MonitorInfo,
  Preferences,
  SessionStatus,
  ShowFile,
} from "./showfile/generated";

export function prefsGet(): Promise<Preferences> {
  return invoke("prefs_get");
}

/** Saves and returns the (range-clamped) preferences. */
export function prefsSet(prefs: Preferences): Promise<Preferences> {
  return invoke("prefs_set", { prefs });
}

export function showModeGet(): Promise<boolean> {
  return invoke("show_mode_get");
}

export function showModeSet(enabled: boolean): Promise<void> {
  return invoke("show_mode_set", { enabled });
}

export function healthReport(): Promise<HealthReport> {
  return invoke("health_report");
}

export function onHealthChanged(handler: (report: HealthReport) => void): Promise<UnlistenFn> {
  return listen<HealthReport>("reliability://health", (e) => handler(e.payload));
}

export function sessionStatus(): Promise<SessionStatus> {
  return invoke("session_status");
}

/** Tells Rust this exit is deliberate (before an updater relaunch). */
export function sessionCleanShutdown(): Promise<void> {
  return invoke("session_clean_shutdown");
}

export function sessionSetLive(session: LiveSession): Promise<void> {
  return invoke("session_set_live", { session });
}

export function sessionGetLive(): Promise<LiveSession | null> {
  return invoke("session_get_live");
}

export function logRecent(): Promise<LogEntry[]> {
  return invoke("log_recent");
}

export function logClear(): Promise<void> {
  return invoke("log_clear");
}

export function logWrite(level: "error" | "warn" | "info", source: string, message: string): Promise<void> {
  return invoke("log_write", { level, source, message });
}

export function logDirPath(): Promise<string> {
  return invoke("log_dir_path");
}

export function onLogMessage(handler: (entry: LogEntry) => void): Promise<UnlistenFn> {
  return listen<LogEntry>("log://message", (e) => handler(e.payload));
}

/** Asks where to save, then writes the diagnostics zip. Returns the path, or
 * null if cancelled. `show` is the current (possibly unsaved) show. */
export function diagnosticsExport(show: ShowFile | null): Promise<string | null> {
  return invoke("diagnostics_export", { show });
}

export function windowMonitors(): Promise<MonitorInfo[]> {
  return invoke("window_monitors");
}

export function windowMoveToMonitor(index: number): Promise<void> {
  return invoke("window_move_to_monitor", { index });
}

export function windowSetFullscreen(fullscreen: boolean): Promise<void> {
  return invoke("window_set_fullscreen", { fullscreen });
}
