import { invoke } from "@tauri-apps/api/core";
import type { McpSettings, McpStatus } from "./showfile/generated";

export function mcpStatus(): Promise<McpStatus> {
  return invoke("mcp_status");
}

/** Saves the settings and starts/stops the server to match. */
export function mcpConfigure(settings: McpSettings): Promise<McpStatus> {
  return invoke("mcp_configure", { settings });
}
