import { invoke } from "@tauri-apps/api/core";
import type {
  AudioDeviceInfo,
  AudioTrigger,
  MidiPortInfo,
  TempoSource,
  TimecodeSource,
  TimingSettings,
  TimingStatus,
} from "./showfile/generated";

// Wrappers for the Phase 8 timing commands (src-tauri/src/timing/commands.rs).

export function timingStatus(): Promise<TimingStatus> {
  return invoke("timing_status");
}

/** Applies all timing settings at once; the engine starts/stops inputs to match. */
export function timingConfigure(settings: TimingSettings): Promise<TimingStatus> {
  return invoke("timing_configure", { settings });
}

export function listAudioInputs(): Promise<AudioDeviceInfo[]> {
  return invoke("timing_list_audio_inputs");
}

export function listMidiInputs(): Promise<MidiPortInfo[]> {
  return invoke("timing_list_midi_inputs");
}

export function listTriggers(): Promise<AudioTrigger[]> {
  return invoke("timing_list_triggers");
}

/** Id 0 creates; returns the trigger's id. */
export function upsertTrigger(trigger: AudioTrigger): Promise<number> {
  return invoke("timing_upsert_trigger", { trigger });
}

export function deleteTrigger(id: number): Promise<void> {
  return invoke("timing_delete_trigger", { id });
}

export function timecodePlay(): Promise<void> {
  return invoke("timing_timecode_play");
}

export function timecodePause(): Promise<void> {
  return invoke("timing_timecode_pause");
}

export function timecodeStop(): Promise<void> {
  return invoke("timing_timecode_stop");
}

export function timecodeSeek(ms: number): Promise<void> {
  return invoke("timing_timecode_seek", { ms });
}

export function setTimecodeLearn(on: boolean): Promise<void> {
  return invoke("timing_set_timecode_learn", { on });
}

/** Sets (or, with null, clears) when a Timecode cuelist fires a cue. */
export function setCueTimecode(cuelistId: number, cueNumber: number, ms: number | null): Promise<void> {
  return invoke("engine_set_cue_timecode", { cuelistId, cueNumber, ms });
}

export const TEMPO_SOURCES: { id: TempoSource; label: string }[] = [
  { id: "manual", label: "Manual" },
  { id: "tap", label: "Tap" },
  { id: "audio", label: "Audio" },
  { id: "midiClock", label: "MIDI clock" },
];

export const TIMECODE_SOURCES: { id: TimecodeSource; label: string }[] = [
  { id: "internal", label: "Internal (Timeline)" },
  { id: "mtc", label: "MIDI timecode (MTC)" },
  { id: "ltc", label: "Audio timecode (LTC)" },
];

/** `hh:mm:ss.ff` at `fps` (default 25) for display. */
export function formatTimecode(ms: number, fps = 25): string {
  const total = Math.max(0, ms);
  const secs = Math.floor(total / 1000);
  const frames = Math.floor(((total % 1000) / 1000) * fps);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(Math.floor(secs / 3600))}:${pad(Math.floor(secs / 60) % 60)}:${pad(secs % 60)}.${pad(frames)}`;
}

/** Parses `s`, `m:s`, `h:m:s` (seconds may be fractional) into ms. */
export function parseTimecode(text: string): number | null {
  const parts = text.trim().split(":");
  if (parts.length === 0 || parts.length > 3 || parts.some((p) => p === "" || !Number.isFinite(Number(p)))) return null;
  const nums = parts.map(Number);
  if (nums.some((n) => n < 0)) return null;
  return nums.reduce((acc, n) => acc * 60 + n, 0) * 1000;
}
