//! Main-window placement across monitors.
//!
//! What Tauri allows: reading monitor geometry, positioning/sizing the
//! window, fullscreen and maximize. What it does not: a reliable way to
//! open additional windows on a chosen monitor on Wayland (the compositor
//! decides), and no per-monitor DPI-independent layout restore beyond the
//! physical-pixel rectangle we save. Secondary output/visualiser windows are
//! therefore not provided; the main window can be moved to any monitor and
//! remembers where it was.

use serde::Serialize;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow};
use ts_rs::TS;

use super::prefs::{PrefsStore, WindowPlacement};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn intersection_area(&self, other: &Rect) -> i64 {
        let left = self.x.max(other.x) as i64;
        let top = self.y.max(other.y) as i64;
        let right = (self.x as i64 + self.width as i64).min(other.x as i64 + other.width as i64);
        let bottom = (self.y as i64 + self.height as i64).min(other.y as i64 + other.height as i64);
        if right <= left || bottom <= top {
            0
        } else {
            (right - left) * (bottom - top)
        }
    }
}

/// Smallest slice of the window that must lie on some monitor for a saved
/// placement to be usable (so a disconnected projector can't strand it).
const MIN_VISIBLE_PX: i64 = 200 * 100;

/// `Some(placement)` if it is still reachable on the current monitors,
/// clamped so the window isn't bigger than the monitor it mostly sits on;
/// `None` means "use the default placement".
pub fn resolve_placement(saved: &WindowPlacement, monitors: &[Rect]) -> Option<WindowPlacement> {
    let window = Rect { x: saved.x, y: saved.y, width: saved.width, height: saved.height };
    let best = monitors.iter().max_by_key(|m| m.intersection_area(&window))?;
    if best.intersection_area(&window) < MIN_VISIBLE_PX.min(window.width as i64 * window.height as i64) {
        return None;
    }
    let width = saved.width.min(best.width);
    let height = saved.height.min(best.height);
    let x = saved.x.clamp(best.x, best.x + (best.width - width) as i32);
    let y = saved.y.clamp(best.y, best.y + (best.height - height) as i32);
    Some(WindowPlacement { x, y, width, height, maximized: saved.maximized })
}

fn monitor_rects(window: &WebviewWindow) -> Vec<Rect> {
    window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| Rect { x: m.position().x, y: m.position().y, width: m.size().width, height: m.size().height })
        .collect()
}

/// Applies the saved placement at startup, if it is still on screen.
pub fn restore(app: &AppHandle, prefs: &PrefsStore) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let Some(saved) = prefs.get().window else {
        return;
    };
    let Some(placement) = resolve_placement(&saved, &monitor_rects(&window)) else {
        tracing::info!("saved window placement is off-screen; using the default");
        return;
    };
    let _ = window.set_size(PhysicalSize::new(placement.width, placement.height));
    let _ = window.set_position(PhysicalPosition::new(placement.x, placement.y));
    if placement.maximized {
        let _ = window.maximize();
    }
}

/// Records the current placement (call on close). A maximized window keeps
/// its previous restored size, so un-maximizing later still works.
pub fn save(app: &AppHandle, prefs: &PrefsStore) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let maximized = window.is_maximized().unwrap_or(false);
    let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    let previous = prefs.get().window;
    let placement = if maximized {
        match previous {
            Some(p) => WindowPlacement { maximized: true, ..p },
            None => WindowPlacement { x: pos.x, y: pos.y, width: size.width, height: size.height, maximized: true },
        }
    } else {
        WindowPlacement { x: pos.x, y: pos.y, width: size.width, height: size.height, maximized: false }
    };
    prefs.set_window(Some(placement));
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct MonitorInfo {
    pub index: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    /// The monitor the main window is mostly on.
    pub current: bool,
}

#[tauri::command]
pub fn window_monitors(window: WebviewWindow) -> Vec<MonitorInfo> {
    let current = window.current_monitor().ok().flatten().map(|m| (m.position().x, m.position().y));
    window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, m)| MonitorInfo {
            index: i as u32,
            name: m.name().cloned().unwrap_or_else(|| format!("Display {}", i + 1)),
            x: m.position().x,
            y: m.position().y,
            width: m.size().width,
            height: m.size().height,
            scale_factor: m.scale_factor(),
            current: current == Some((m.position().x, m.position().y)),
        })
        .collect()
}

/// Moves the main window to the given monitor, keeping its size (clamped to
/// the monitor).
#[tauri::command]
pub fn window_move_to_monitor(window: WebviewWindow, index: u32, prefs: State<PrefsStore>) -> Result<(), String> {
    let monitors = window.available_monitors().map_err(|e| e.to_string())?;
    let target = monitors.get(index as usize).ok_or_else(|| format!("No monitor {index}"))?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let (mw, mh) = (target.size().width, target.size().height);
    let width = size.width.min(mw);
    let height = size.height.min(mh);
    let was_fullscreen = window.is_fullscreen().unwrap_or(false);
    if was_fullscreen {
        let _ = window.set_fullscreen(false);
    }
    let _ = window.unmaximize();
    window.set_size(PhysicalSize::new(width, height)).map_err(|e| e.to_string())?;
    window
        .set_position(PhysicalPosition::new(
            target.position().x + ((mw - width) / 2) as i32,
            target.position().y + ((mh - height) / 2) as i32,
        ))
        .map_err(|e| e.to_string())?;
    if was_fullscreen {
        let _ = window.set_fullscreen(true);
    }
    save(window.app_handle(), &prefs);
    Ok(())
}

#[tauri::command]
pub fn window_set_fullscreen(window: WebviewWindow, fullscreen: bool) -> Result<(), String> {
    window.set_fullscreen(fullscreen).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Rect = Rect { x: 0, y: 0, width: 1920, height: 1080 };
    const RIGHT: Rect = Rect { x: 1920, y: 0, width: 1920, height: 1080 };

    fn placed(x: i32, y: i32, w: u32, h: u32) -> WindowPlacement {
        WindowPlacement { x, y, width: w, height: h, maximized: false }
    }

    #[test]
    fn an_on_screen_placement_is_kept() {
        let p = placed(100, 50, 1400, 900);
        assert_eq!(resolve_placement(&p, &[MAIN]), Some(p));
    }

    #[test]
    fn a_placement_on_a_missing_monitor_is_dropped() {
        // Saved on a second monitor that is no longer connected.
        assert_eq!(resolve_placement(&placed(2000, 100, 1400, 900), &[MAIN]), None);
        assert_eq!(resolve_placement(&placed(100, 100, 1400, 900), &[]), None);
    }

    #[test]
    fn the_second_monitor_is_honoured_when_present() {
        let p = placed(2000, 100, 1400, 900);
        assert_eq!(resolve_placement(&p, &[MAIN, RIGHT]), Some(p));
    }

    #[test]
    fn oversized_or_edge_placements_are_clamped_onto_the_monitor() {
        let got = resolve_placement(&placed(1000, 600, 3000, 2000), &[MAIN]).unwrap();
        assert_eq!((got.width, got.height), (1920, 1080));
        assert_eq!((got.x, got.y), (0, 0));
        let got = resolve_placement(&placed(1500, 700, 1400, 900), &[MAIN]).unwrap();
        assert!(got.x + got.width as i32 <= 1920 && got.y + got.height as i32 <= 1080);
    }
}
