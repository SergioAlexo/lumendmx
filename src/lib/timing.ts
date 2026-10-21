import { invoke } from "@tauri-apps/api/core";
import type {
  AudioDeviceInfo,
  MidiEvent,
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

/** MIDI channel messages received after `after` (the last `seq` seen). */
export function midiEvents(after: number): Promise<MidiEvent[]> {
  return invoke("midi_events", { after });
}

export const TEMPO_SOURCES: { id: TempoSource; label: string }[] = [
  { id: "manual", label: "Manual" },
  { id: "tap", label: "Tap" },
  { id: "audio", label: "Audio" },
  { id: "midiClock", label: "MIDI clock" },
  { id: "mixxx", label: "Mixxx" },
];

export const TIMECODE_SOURCES: { id: TimecodeSource; label: string }[] = [
  { id: "none", label: "None" },
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
