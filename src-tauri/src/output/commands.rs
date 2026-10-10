use std::time::Duration;

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use super::config::DriverConfig;
use crate::reliability::prefs::PrefsStore;
use crate::reliability::show_mode::ShowLock;
use super::input::DmxInputManager;
use super::manager::OutputManager;
use super::runner::UniverseStatus;
use super::{artnet, UNIVERSE_SIZE};

#[derive(Serialize)]
pub struct SerialPortDescriptor {
    pub port_name: String,
    /// Human-readable label (USB product/manufacturer when available) so the
    /// UI can tell the user which entry is their FTDI/Enttec adapter instead
    /// of a bare COMn.
    pub label: String,
}

#[tauri::command]
pub fn list_serial_ports() -> Result<Vec<SerialPortDescriptor>, String> {
    serialport::available_ports()
        .map(|ports| {
            ports
                .into_iter()
                .map(|p| {
                    let label = match &p.port_type {
                        serialport::SerialPortType::UsbPort(info) => {
                            let name = info
                                .product
                                .as_deref()
                                .filter(|s| !s.trim().is_empty())
                                .or(info.manufacturer.as_deref())
                                .map(str::to_string)
                                .unwrap_or_else(|| format!("USB {:04x}:{:04x}", info.vid, info.pid));
                            format!("{} — {}", p.port_name, name)
                        }
                        _ => p.port_name.clone(),
                    };
                    SerialPortDescriptor {
                        port_name: p.port_name,
                        label,
                    }
                })
                .collect()
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn output_configure_universe(
    manager: State<OutputManager>,
    lock: State<ShowLock>,
    universe_id: u32,
    driver: DriverConfig,
) -> Result<(), String> {
    lock.guard("change an output")?;
    manager.configure(universe_id, driver)
}

#[tauri::command]
pub fn output_remove_universe(
    manager: State<OutputManager>,
    lock: State<ShowLock>,
    universe_id: u32,
) -> Result<(), String> {
    lock.guard("remove an output")?;
    manager.remove(universe_id);
    Ok(())
}

/// Overwrites all 512 channels of `universe_id`. Errors if that universe has
/// no driver configured (call `output_configure_universe` first).
#[tauri::command]
pub fn output_update_universe_data(
    manager: State<OutputManager>,
    universe_id: u32,
    channels: Vec<u8>,
) -> Result<(), String> {
    if channels.len() != UNIVERSE_SIZE {
        return Err(format!(
            "Expected {UNIVERSE_SIZE} channels, got {}",
            channels.len()
        ));
    }
    let mut data = [0u8; UNIVERSE_SIZE];
    data.copy_from_slice(&channels);
    manager.set_universe_data(universe_id, data)
}

#[tauri::command]
pub fn output_get_universe_data(manager: State<OutputManager>, universe_id: u32) -> Result<Vec<u8>, String> {
    manager.get_universe_data(universe_id).map(|d| d.to_vec())
}

#[tauri::command]
pub fn output_universe_status(manager: State<OutputManager>, universe_id: u32) -> Option<UniverseStatus> {
    manager.status(universe_id)
}

#[tauri::command]
pub fn output_all_statuses(manager: State<OutputManager>) -> Vec<UniverseStatus> {
    manager.all_statuses()
}

/// Global safety override: forces every active universe's wire output to 0
/// without touching any universe's stored buffer, so releasing blackout
/// resumes exactly where the show was.
#[tauri::command]
pub fn output_set_blackout(manager: State<OutputManager>, active: bool) -> Result<(), String> {
    manager.set_blackout(active);
    Ok(())
}

#[tauri::command]
pub fn output_get_blackout(manager: State<OutputManager>) -> bool {
    manager.get_blackout()
}

/// The driver config a universe is currently running, if any (e.g. to
/// prefill the Setup screen's edit form for it).
#[tauri::command]
pub fn output_get_universe_config(manager: State<OutputManager>, universe_id: u32) -> Option<DriverConfig> {
    manager.driver_config(universe_id)
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct DriverDefaults {
    pub serial_rate_hz: u32,
    pub network_rate_hz: u32,
}

/// Default output rates for the Setup screen's "add a universe" form (30Hz
/// for serial widgets, 40Hz for network protocols).
#[tauri::command]
pub fn output_driver_defaults(prefs: State<PrefsStore>) -> DriverDefaults {
    let prefs = prefs.get();
    DriverDefaults {
        serial_rate_hz: prefs.serial_rate_hz,
        network_rate_hz: prefs.network_rate_hz,
    }
}

/// Human-readable label for a driver kind (e.g. "FTDI Open DMX"), for the
/// Setup screen's universe list.
#[tauri::command]
pub fn output_driver_label(driver: DriverConfig) -> String {
    driver.label().to_string()
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct ArtNetNode {
    pub ip: String,
    pub short_name: String,
    pub long_name: String,
}

/// Broadcasts an ArtPoll and collects replies for ~2s. Best-effort discovery
/// for the Setup screen; real interop still needs an on-network check (see
/// HARDWARE_CHECKS.md).
#[tauri::command]
pub fn output_discover_artnet_nodes() -> Result<Vec<ArtNetNode>, String> {
    let nodes = artnet::discover_nodes(Duration::from_secs(2))?;
    Ok(nodes
        .into_iter()
        .map(|n| ArtNetNode {
            ip: format!("{}.{}.{}.{}", n.ip[0], n.ip[1], n.ip[2], n.ip[3]),
            short_name: n.short_name,
            long_name: n.long_name,
        })
        .collect())
}

// --- DMX input (see output::input's scope note: decode-only for now, not yet
// merged into any universe's output) ---

#[tauri::command]
pub fn input_start_artnet(manager: State<DmxInputManager>) -> Result<(), String> {
    manager.start_artnet_listener()
}

#[tauri::command]
pub fn input_get_artnet_frame(
    manager: State<DmxInputManager>,
    net: u8,
    subnet: u8,
    universe: u8,
) -> Option<Vec<u8>> {
    manager.artnet_frame(net, subnet, universe).map(|d| d.to_vec())
}

#[tauri::command]
pub fn input_start_sacn(manager: State<DmxInputManager>, universe: u16) -> Result<(), String> {
    manager.start_sacn_listener(universe)
}

#[tauri::command]
pub fn input_stop_sacn(manager: State<DmxInputManager>, universe: u16) {
    manager.stop_sacn_listener(universe);
}

#[tauri::command]
pub fn input_get_sacn_frame(manager: State<DmxInputManager>, universe: u16) -> Option<Vec<u8>> {
    manager.sacn_frame(universe).map(|d| d.to_vec())
}
