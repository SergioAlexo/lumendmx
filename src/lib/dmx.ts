import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ArtNetNode, DetectedController, DriverConfig, DriverDefaults, UniverseStatus } from "./showfile/generated";

export const UNIVERSE_SIZE = 512;

export interface SerialPortDescriptor {
  port_name: string;
  /** Human-readable label (USB product/manufacturer when available). */
  label: string;
}

export function listSerialPorts(): Promise<SerialPortDescriptor[]> {
  return invoke("list_serial_ports");
}

export function getUniverseData(universeId: number): Promise<number[]> {
  return invoke("output_get_universe_data", { universeId });
}

export function getUniverseStatus(universeId: number): Promise<UniverseStatus | null> {
  return invoke("output_universe_status", { universeId });
}

export function getAllStatuses(): Promise<UniverseStatus[]> {
  return invoke("output_all_statuses");
}

export function getUniverseConfig(universeId: number): Promise<DriverConfig | null> {
  return invoke("output_get_universe_config", { universeId });
}

export function getDriverDefaults(): Promise<DriverDefaults> {
  return invoke("output_driver_defaults");
}

export function getDriverLabel(driver: DriverConfig): Promise<string> {
  return invoke("output_driver_label", { driver });
}

/** Broadcasts an ArtPoll and collects replies for a few seconds. */
export function discoverArtNetNodes(): Promise<ArtNetNode[]> {
  return invoke("output_discover_artnet_nodes");
}

/** USB DMX widgets plugged in now, plus Art-Net nodes answering a poll
 * (takes ~2s), each with driver settings ready to use for a universe. */
export function detectControllers(): Promise<DetectedController[]> {
  return invoke("output_detect_controllers");
}

/** Global safety override across every active universe. */
export function setBlackout(active: boolean): Promise<void> {
  return invoke("output_set_blackout", { active });
}

export function getBlackout(): Promise<boolean> {
  return invoke("output_get_blackout");
}

export function onUniverseStatusChanged(cb: (status: UniverseStatus) => void) {
  return listen<UniverseStatus>("dmx://universe-status-changed", (event) => cb(event.payload));
}

/** The label a universe's controller shows under, e.g. "Enttec DMX USB Pro
 * — /dev/cu.usbserial-EN1234" or "Art-Net 2.0.0.10". */
export function controllerLabel(driver: DriverConfig): string {
  switch (driver.kind) {
    case "null":
      return "No output";
    case "ftdi":
      return `FTDI Open DMX${driver.port ? ` — ${driver.port}` : ""}`;
    case "enttecPro":
      return `Enttec DMX USB Pro${driver.port ? ` — ${driver.port}` : ""}`;
    case "artNet":
      return `Art-Net ${driver.destination} (${driver.net}:${driver.subnet}:${driver.universe})`;
    case "sacn":
      return `sACN universe ${driver.universe}`;
  }
}
