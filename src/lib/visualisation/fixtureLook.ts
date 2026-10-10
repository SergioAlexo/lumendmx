/**
 * Pure derivation of what a fixture "looks like" from its DMX channel values:
 * colour, intensity, pan/tilt and beam angle. Shared by the 2D stage plot and
 * the 3D visualizer. Nothing here touches React, the engine or three.js, so
 * it is unit-tested directly (fixtureLook.test.ts).
 *
 * These are approximations for programming without a rig, not photometric
 * simulation: colour is an additive mix of the colour channels, a colour wheel
 * uses the slot colour when the definition provides one, and zoom linearly
 * interpolates the fixture's beam-angle range.
 */
import type { ChannelType, FixtureChannel, FixtureDefinition, FixtureMode } from "../fixtures/types";

export interface Rgb {
  r: number;
  g: number;
  b: number;
}

export interface FixtureLook {
  /** Colour at full brightness (each component 0-255), brightness removed. */
  color: Rgb;
  /** 0-1: dimmer (or, with no dimmer channel, the brightest colour component). */
  intensity: number;
  /** Pan in degrees from the fixture's centre position, or null if it has no pan channel. */
  panDeg: number | null;
  tiltDeg: number | null;
  /** Full beam cone angle in degrees. */
  beamAngleDeg: number;
  /** True for fixtures with pan or tilt channels. */
  moving: boolean;
}

/** The slice of a patched fixture the derivation needs. */
export interface LookInstance {
  address: number;
  universe: number;
  invertPan?: boolean;
  invertTilt?: boolean;
  swapPanTilt?: boolean;
}

/** Reads a live channel value (1-based channel, 0-255). */
export type ChannelReader = (universe: number, channel: number) => number;

const DEFAULT_PAN_RANGE_DEG = 540;
const DEFAULT_TILT_RANGE_DEG = 270;
const DEFAULT_MOVING_BEAM_DEG = 20;
const DEFAULT_STATIC_BEAM_DEG = 40;

const AMBER: Rgb = { r: 255, g: 191, b: 0 };
const UV: Rgb = { r: 110, g: 0, b: 255 };

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** Parses "#rgb" / "#rrggbb"; null if it isn't one. */
export function parseHexColor(hex: string | undefined): Rgb | null {
  if (!hex) return null;
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  let h = m[1];
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  const n = parseInt(h, 16);
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
}

export function rgbToCss({ r, g, b }: Rgb): string {
  return `rgb(${Math.round(r)}, ${Math.round(g)}, ${Math.round(b)})`;
}

export function rgbToHex({ r, g, b }: Rgb): string {
  const part = (v: number) => Math.round(clamp(v, 0, 255)).toString(16).padStart(2, "0");
  return `#${part(r)}${part(g)}${part(b)}`;
}

/** 0-1 value of a channel, combining the fine (LSB) channel for a 16-bit pair. */
function readNormalized(ch: FixtureChannel, address: number, universe: number, read: ChannelReader): number {
  const coarse = read(universe, address + ch.offset - 1);
  if (ch.fineOffset === undefined) return coarse / 255;
  const fine = read(universe, address + ch.fineOffset - 1);
  return (coarse * 256 + fine) / 65535;
}

function firstOfType(channels: FixtureChannel[], type: ChannelType): FixtureChannel | undefined {
  return channels.find((c) => c.type === type);
}

/** Colour-wheel slot colour for the current value, if the definition gives one. */
function wheelColor(ch: FixtureChannel, value: number): Rgb | null {
  const slot = ch.capabilities?.find((c) => value >= c.dmxRange[0] && value <= c.dmxRange[1]);
  return parseHexColor(slot?.color);
}

interface Mixed {
  color: Rgb;
  /** Brightest component before normalisation (0-255): how bright the colour channels say it is. */
  peak: number;
  /** False when the fixture has no colour mixing at all (a plain white dimmer). */
  hasColor: boolean;
}

function mixColor(channels: FixtureChannel[], instance: LookInstance, read: ChannelReader): Mixed {
  const val = (type: ChannelType) => {
    const ch = firstOfType(channels, type);
    return ch ? readNormalized(ch, instance.address, instance.universe, read) : null;
  };
  const r = val("red");
  const g = val("green");
  const b = val("blue");
  const w = val("white");
  const a = val("amber");
  const uv = val("uv");
  const c = val("cyan");
  const m = val("magenta");
  const y = val("yellow");

  const wheel = firstOfType(channels, "colorwheel");
  const wheelRgb = wheel
    ? wheelColor(wheel, read(instance.universe, instance.address + wheel.offset - 1))
    : null;

  const additive = r !== null || g !== null || b !== null || w !== null || a !== null || uv !== null;
  const subtractive = c !== null || m !== null || y !== null;

  let out: Rgb;
  let hasColor = true;
  if (additive) {
    out = { r: (r ?? 0) * 255, g: (g ?? 0) * 255, b: (b ?? 0) * 255 };
    if (w !== null) {
      out.r += w * 255;
      out.g += w * 255;
      out.b += w * 255;
    }
    if (a !== null) {
      out.r += a * AMBER.r;
      out.g += a * AMBER.g;
      out.b += a * AMBER.b;
    }
    if (uv !== null) {
      out.r += uv * UV.r;
      out.g += uv * UV.g;
      out.b += uv * UV.b;
    }
  } else if (subtractive) {
    // CMY flags subtract from a white source.
    out = { r: (1 - (c ?? 0)) * 255, g: (1 - (m ?? 0)) * 255, b: (1 - (y ?? 0)) * 255 };
  } else if (wheelRgb) {
    out = { ...wheelRgb };
  } else {
    out = { r: 255, g: 255, b: 255 };
    hasColor = false;
  }
  if (wheelRgb && (additive || subtractive)) {
    // A colour wheel filters whatever the mixing produced.
    out = { r: (out.r * wheelRgb.r) / 255, g: (out.g * wheelRgb.g) / 255, b: (out.b * wheelRgb.b) / 255 };
  }
  out = { r: clamp(out.r, 0, 255), g: clamp(out.g, 0, 255), b: clamp(out.b, 0, 255) };
  const peak = Math.max(out.r, out.g, out.b);
  const color = peak > 0 ? { r: (out.r * 255) / peak, g: (out.g * 255) / peak, b: (out.b * 255) / peak } : { r: 0, g: 0, b: 0 };
  return { color, peak, hasColor };
}

function rangeDeg(ch: FixtureChannel, fallback: number): number {
  const range = ch.physicalRange ?? ch.capabilities?.find((c) => c.physicalRange)?.physicalRange;
  if (range && range.unit === "deg" && range.max > range.min) return range.max - range.min;
  return fallback;
}

/** Look of one set of (whole-fixture or single-cell) channels. */
function lookOfChannels(
  channels: FixtureChannel[],
  def: FixtureDefinition,
  instance: LookInstance,
  read: ChannelReader,
): FixtureLook {
  const mixed = mixColor(channels, instance, read);
  const dimmer = firstOfType(channels, "dimmer");
  let intensity: number;
  if (dimmer) {
    const d = readNormalized(dimmer, instance.address, instance.universe, read);
    intensity = mixed.hasColor ? d * (mixed.peak / 255) : d;
  } else {
    // Colour-only fixtures (RGB with no master) are as bright as their brightest component.
    intensity = mixed.hasColor ? mixed.peak / 255 : 1;
  }

  const panCh = firstOfType(channels, "pan");
  const tiltCh = firstOfType(channels, "tilt");
  let panDeg: number | null = null;
  let tiltDeg: number | null = null;
  if (panCh) {
    const v = readNormalized(panCh, instance.address, instance.universe, read);
    panDeg = (v - 0.5) * rangeDeg(panCh, DEFAULT_PAN_RANGE_DEG) * (instance.invertPan ? -1 : 1);
  }
  if (tiltCh) {
    const v = readNormalized(tiltCh, instance.address, instance.universe, read);
    tiltDeg = (v - 0.5) * rangeDeg(tiltCh, DEFAULT_TILT_RANGE_DEG) * (instance.invertTilt ? -1 : 1);
  }
  if (instance.swapPanTilt && panDeg !== null && tiltDeg !== null) {
    [panDeg, tiltDeg] = [tiltDeg, panDeg];
  }

  const moving = panCh !== undefined || tiltCh !== undefined;
  const [minBeam, maxBeam] = def.physical?.beamAngleDeg ?? [
    moving ? DEFAULT_MOVING_BEAM_DEG : DEFAULT_STATIC_BEAM_DEG,
    moving ? DEFAULT_MOVING_BEAM_DEG : DEFAULT_STATIC_BEAM_DEG,
  ];
  const zoomCh = firstOfType(channels, "zoom");
  const zoom = zoomCh ? readNormalized(zoomCh, instance.address, instance.universe, read) : 0;
  // Low zoom values are the narrow end of the range (a common, not universal, convention).
  const beamAngleDeg = minBeam + (maxBeam - minBeam) * zoom;

  return { color: mixed.color, intensity: clamp(intensity, 0, 1), panDeg, tiltDeg, beamAngleDeg, moving };
}

/**
 * The fixture's current look. A multi-cell fixture (LED bar, pixel batten)
 * averages its cells' colour weighted by their brightness; its pan/tilt, if
 * any, come from the whole-fixture channels.
 */
export function deriveFixtureLook(
  def: FixtureDefinition,
  mode: FixtureMode,
  instance: LookInstance,
  read: ChannelReader,
): FixtureLook {
  const whole = mode.channels.filter((ch) => !ch.cell);
  const cells = new Map<number, FixtureChannel[]>();
  for (const ch of mode.channels) {
    if (ch.cell) cells.set(ch.cell, [...(cells.get(ch.cell) ?? []), ch]);
  }
  if (cells.size === 0) return lookOfChannels(whole, def, instance, read);

  // Shared (non-cell) channels such as a master dimmer apply to every cell.
  const base = lookOfChannels(whole, def, instance, read);
  let r = 0;
  let g = 0;
  let b = 0;
  let weight = 0;
  let intensitySum = 0;
  for (const channels of cells.values()) {
    const look = lookOfChannels([...whole.filter((c) => c.type !== "pan" && c.type !== "tilt"), ...channels], def, instance, read);
    r += look.color.r * look.intensity;
    g += look.color.g * look.intensity;
    b += look.color.b * look.intensity;
    weight += look.intensity;
    intensitySum += look.intensity;
  }
  return {
    ...base,
    color: weight > 0 ? { r: r / weight, g: g / weight, b: b / weight } : { r: 0, g: 0, b: 0 },
    intensity: clamp(intensitySum / cells.size, 0, 1),
  };
}

/** The colour as it should be painted: the fixture's colour scaled by its intensity. */
export function litColor(look: Pick<FixtureLook, "color" | "intensity">): Rgb {
  return { r: look.color.r * look.intensity, g: look.color.g * look.intensity, b: look.color.b * look.intensity };
}
