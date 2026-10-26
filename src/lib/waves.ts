// The LFO waveforms and the Fan's per-fixture phase offsets, for drawing
// them on the nodes. Mirrors `waveform_value` and `fan_position` in
// src-tauri/src/engine/waves.rs; the engine stays the source of truth.

import { curveValue, parseCurve, type CurveMode } from "./curve";

export type WaveSettings = { waveform: string; width: number; curve: string; curveMode: CurveMode };

/** An LFO's value (0-1) at cycle position `p` (0-1). Random has no fixed shape: undefined. */
export function lfoValue(wave: WaveSettings, p: number): number | undefined {
  const w = Math.min(100, Math.max(1, wave.width)) / 100;
  switch (wave.waveform) {
    case "sine":
      return (Math.sin(2 * Math.PI * p) + 1) / 2;
    case "square":
      return p < w ? 1 : 0;
    case "saw":
      return 1 - p;
    case "ramp":
      return p;
    case "step":
      return Math.min(Math.floor(p * 4), 3) / 3;
    case "pulse":
      return p < w ? Math.sin((Math.PI * p) / w) : 0;
    case "custom":
      return curveValue(parseCurve(wave.curve), wave.curveMode, Math.min(p, 1 - 1e-9));
    default:
      return undefined;
  }
}

export type FanSettings = { spread: number; groups: number; blocks: number; wings: number };

/** Where fixture `index` (of `count`) sits in the fan: (position, positions). */
export function fanPosition(fan: FanSettings, index: number, count: number): [number, number] {
  let n = Math.max(count, 1);
  let i = index;
  if (fan.wings > 1) {
    const wingLen = Math.max(Math.ceil(n / fan.wings), 1);
    const wing = Math.floor(i / wingLen);
    const within = i % wingLen;
    i = wing % 2 === 1 ? wingLen - 1 - within : within;
    n = wingLen;
  }
  if (fan.blocks > 1) {
    i = Math.floor(i / fan.blocks);
    n = Math.ceil(n / fan.blocks);
  }
  if (fan.groups > 1) {
    i %= fan.groups;
    n = Math.min(n, fan.groups);
  }
  return [Math.min(i, n - 1), n];
}

/** Each fixture's phase offset in cycles (0-1), as the Fan node outputs it. */
export function fanOffsets(fan: FanSettings, count: number): number[] {
  return Array.from({ length: count }, (_, i) => {
    const [position, positions] = fanPosition(fan, i, count);
    const offset = (-fan.spread * position) / positions;
    return ((offset % 1) + 1) % 1;
  });
}
