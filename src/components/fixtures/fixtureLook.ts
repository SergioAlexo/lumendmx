import type { ChannelType, FixtureDefinition, FixtureMode } from "../../lib/fixtures/types";
import type { PatchedFixture } from "../../lib/showfile/generated";

export function findMode(library: FixtureDefinition[], fixture: PatchedFixture): { def?: FixtureDefinition; mode?: FixtureMode } {
  const def = library.find((d) => d.id === fixture.fixtureId);
  return { def, mode: def?.modes[fixture.modeIndex] };
}

/** Live value (0-255) of each of `mode`'s channels, keyed by offset, read from
 * the fixture's universe frame. */
export function channelValues(fixture: PatchedFixture, mode: FixtureMode, frame: Uint8Array | undefined): Record<number, number> {
  const values: Record<number, number> = {};
  for (const ch of mode.channels) {
    // address and offset are both 1-based, so this is the 0-based frame index.
    const idx = fixture.address + ch.offset - 2;
    values[ch.offset] = frame && idx >= 0 && idx < frame.length ? frame[idx] : 0;
  }
  return values;
}

/** Approximates the colour the fixture is putting out right now, for swatches. */
export function swatchColor(mode: FixtureMode, values: Record<number, number>): string {
  const offsetFor = (t: ChannelType) => mode.channels.find((c) => c.type === t)?.offset;
  const valueOf = (t: ChannelType) => {
    const offset = offsetFor(t);
    return offset === undefined ? 0 : (values[offset] ?? 0);
  };

  const hasDimmer = offsetFor("dimmer") !== undefined;
  const dimmer = hasDimmer ? valueOf("dimmer") / 255 : 1;

  let r = valueOf("red");
  let g = valueOf("green");
  let b = valueOf("blue");
  // Fold the extra emitters into RGB (approximate hues; UV is shown as the
  // violet it visibly glows).
  const emitters: [ChannelType, [number, number, number]][] = [
    ["white", [255, 255, 255]],
    ["amber", [255, 150, 0]],
    ["uv", [110, 0, 255]],
  ];
  for (const [type, [er, eg, eb]] of emitters) {
    const level = valueOf(type) / 255;
    r = Math.max(r, er * level);
    g = Math.max(g, eg * level);
    b = Math.max(b, eb * level);
  }
  // CMY mixing: each flag subtracts its complement from white.
  const cmy = (["cyan", "magenta", "yellow"] as ChannelType[]).map((t) => (offsetFor(t) === undefined ? null : valueOf(t)));
  if (cmy.some((v) => v !== null)) {
    r = Math.max(r, 255 - (cmy[0] ?? 0));
    g = Math.max(g, 255 - (cmy[1] ?? 0));
    b = Math.max(b, 255 - (cmy[2] ?? 0));
  }

  if (r === 0 && g === 0 && b === 0 && hasDimmer) {
    // No colour channels at all (a dimmer-only fixture): a plain glow
    // scaled by the dimmer.
    r = g = b = 255;
  }

  return `rgb(${Math.round(r * dimmer)}, ${Math.round(g * dimmer)}, ${Math.round(b * dimmer)})`;
}

export function hasAttribute(mode: FixtureMode | undefined, type: ChannelType): boolean {
  return mode?.channels.some((c) => c.type === type) ?? false;
}

/** Fastest flash rate drawn: past this a screen can't show it anyway. */
const MAX_STROBE_HZ = 25;

/** How fast the fixture is strobing right now, in flashes per second, or 0
 * when it isn't. Follows the strobe channel's capabilities (a sub-range with a
 * rate in Hz is interpolated); a plain strobe channel runs 1-20 Hz across 1-255. */
export function strobeHz(mode: FixtureMode, values: Record<number, number>): number {
  const channel = mode.channels.find((c) => c.type === "strobe");
  const value = channel ? (values[channel.offset] ?? 0) : 0;
  if (!channel || value <= 0) return 0;
  const caps = channel.capabilities ?? [];
  const cap = caps.find((c) => value >= c.dmxRange[0] && value <= c.dmxRange[1]);
  if (caps.length > 0 && cap?.type !== "strobe") return 0;
  const [lo, hi] = cap?.dmxRange ?? [1, 255];
  const t = hi > lo ? (value - lo) / (hi - lo) : 1;
  const range = cap?.physicalRange;
  const hz = range && range.unit === "hz" ? range.min + (range.max - range.min) * t : 1 + 19 * t;
  return Math.min(MAX_STROBE_HZ, Math.max(0.5, hz));
}

/** Inline style that flashes a swatch at `hz` (see `strobe-flash` in index.css). */
export function strobeStyle(hz: number): { animation: string } | undefined {
  return hz > 0 ? { animation: `strobe-flash ${1 / hz}s step-end infinite` } : undefined;
}
