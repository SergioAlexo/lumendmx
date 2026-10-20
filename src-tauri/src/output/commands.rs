use std::time::Duration;

use serde::Serialize;
use tauri::State;
use ts_rs::TS;

use super::config::{DriverConfig, DEFAULT_NETWORK_RATE_HZ, DEFAULT_SERIAL_RATE_HZ};
use super::input::DmxInputManager;
use super::manager::OutputManager;
use super::runner::UniverseStatus;
use super::artnet;

#[derive(Serialize)]
pub struct SerialPortDescriptor {
    pub port_name: String,
    /// Human-readable label (USB product/manufacturer when available) so the
    /// UI can tell the user which entry is their FTDI/Enttec adapter instead
    /// of a bare COMn.
    pub label: String,
}

/// Async so the IOKit scan doesn't block the UI thread.
#[tauri::command]
pub async fn list_serial_ports() -> Result<Vec<SerialPortDescriptor>, String> {
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
pub fn output_driver_defaults() -> DriverDefaults {
    DriverDefaults {
        serial_rate_hz: DEFAULT_SERIAL_RATE_HZ,
        network_rate_hz: DEFAULT_NETWORK_RATE_HZ,
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

/// A DMX controller found on this machine or network, with the driver
/// settings to use it as a universe's output.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/lib/showfile/generated/")]
pub struct DetectedController {
    pub label: String,
    pub driver: DriverConfig,
}

/// What a USB serial device is as a DMX controller, if anything. Enttec's
/// Pro announces itself in its product string; any other FTDI-chip device
/// (vendor 0x0403) is taken to be an Open DMX-style widget.
fn classify_serial(port_name: &str, vid: u16, product: Option<&str>) -> Option<DriverConfig> {
    // macOS lists every serial device twice; the call-out (`cu.`) node is
    // the one to open.
    if port_name.starts_with("/dev/tty.") {
        return None;
    }
    let product = product.unwrap_or_default().to_ascii_lowercase();
    if product.contains("dmx usb pro") || product.contains("enttec pro") {
        return Some(DriverConfig::EnttecPro { port: Some(port_name.to_string()), universe_index: 0, rate_hz: DEFAULT_SERIAL_RATE_HZ });
    }
    (vid == 0x0403).then(|| DriverConfig::Ftdi { port: Some(port_name.to_string()), rate_hz: DEFAULT_SERIAL_RATE_HZ })
}

/// USB DMX widgets plugged in now, plus Art-Net nodes answering a poll
/// (~2s). Async so the poll doesn't block the UI thread.
#[tauri::command]
pub async fn output_detect_controllers() -> Vec<DetectedController> {
    let mut found = Vec::new();
    for port in serialport::available_ports().unwrap_or_default() {
        if let serialport::SerialPortType::UsbPort(info) = &port.port_type {
            if let Some(driver) = classify_serial(&port.port_name, info.vid, info.product.as_deref()) {
                let name = info.product.clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "USB DMX".to_string());
                found.push(DetectedController { label: format!("{name} — {}", port.port_name), driver });
            }
        }
    }
    for node in artnet::discover_nodes(Duration::from_secs(2)).unwrap_or_default() {
        let ip = format!("{}.{}.{}.{}", node.ip[0], node.ip[1], node.ip[2], node.ip[3]);
        let name = if node.long_name.is_empty() { node.short_name.clone() } else { node.long_name.clone() };
        found.push(DetectedController {
            label: format!("Art-Net: {} — {ip}", if name.is_empty() { "node".to_string() } else { name }),
            driver: DriverConfig::ArtNet { destination: ip, net: 0, subnet: 0, universe: 0, rate_hz: DEFAULT_NETWORK_RATE_HZ },
        });
    }
    found
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_devices_are_classified_by_chip_and_product() {
        assert!(matches!(classify_serial("/dev/cu.usbserial-1", 0x0403, Some("DMX USB PRO")), Some(DriverConfig::EnttecPro { .. })));
        assert!(matches!(classify_serial("/dev/cu.usbserial-2", 0x0403, Some("FT232R USB UART")), Some(DriverConfig::Ftdi { .. })));
        assert_eq!(classify_serial("/dev/tty.usbserial-2", 0x0403, None), None);
        assert_eq!(classify_serial("/dev/cu.usbmodem1", 0x2341, Some("Arduino Uno")), None);
    }
}
