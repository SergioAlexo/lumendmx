import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { EngineFrame, TempoStatus } from "./showfile/generated";

export function onUniverseFrame(cb: (frame: EngineFrame) => void) {
  return listen<EngineFrame>("engine://universe-frame", (event) => cb(event.payload));
}

/** 0-100%. Scales every intensity channel of the output. */
export function setGrandMaster(percent: number): Promise<void> {
  return invoke("engine_set_grand_master", { percent });
}

export function grandMaster(): Promise<number> {
  return invoke("engine_grand_master");
}

export function tempoStatus(): Promise<TempoStatus> {
  return invoke("engine_tempo_status");
}

export function setBpm(bpm: number): Promise<void> {
  return invoke("engine_set_bpm", { bpm });
}

/** Passes the tap's own timestamp so IPC latency doesn't skew the interval. */
export function tapTempo(): Promise<number> {
  return invoke("engine_tap_tempo", { nowMs: performance.now() });
}

export type { EngineFrame, TempoStatus };
