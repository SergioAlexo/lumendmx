// An LFO's or Shape's Rate when it runs per beat, shown the musical way: as
// how long one cycle is, in beats. The stored rate is cycles per beat, so a
// cycle of 4 beats is rate 0.25.

export const MAX_RATE = 64;

/** The cycle lengths in the dropdown, longest first. */
export const BEAT_PRESETS: { length: number; label: string }[] = [
  { length: 16, label: "16 beats" },
  { length: 8, label: "8 beats" },
  { length: 4, label: "4 beats (1 bar)" },
  { length: 2, label: "2 beats" },
  { length: 1, label: "1 beat" },
  { length: 1 / 2, label: "1/2 beat" },
  { length: 1 / 4, label: "1/4 beat" },
  { length: 1 / 8, label: "1/8 beat" },
  { length: 1 / 16, label: "1/16 beat" },
];

const near = (a: number, b: number) => Math.abs(a - b) < 1e-6;

/** "3/4 beat", "2.5 beats": a length in beats, as a fraction when it is a simple one under a beat. */
export function lengthLabel(length: number): string {
  if (near(length, Math.round(length)) && length >= 1) return `${Math.round(length)} ${Math.round(length) === 1 ? "beat" : "beats"}`;
  if (length < 1) {
    for (let d = 2; d <= 16; d++) {
      const n = Math.round(length * d);
      if (n > 0 && Math.abs(length - n / d) < 1e-4) {
        const g = gcd(n, d);
        return `${n / g}/${d / g} beat`;
      }
    }
  }
  return `${Number(length.toFixed(2))} beats`;
}

function gcd(a: number, b: number): number {
  return b === 0 ? a : gcd(b, a % b);
}

/** What the field shows for a stored rate (cycles per beat). */
export function rateLabel(rate: number): string {
  if (!(rate > 0)) return "stopped";
  const preset = BEAT_PRESETS.find((p) => near(1 / p.length, rate));
  return preset ? preset.label : lengthLabel(1 / rate);
}

/** Reads what was typed as a cycle length in beats: "2", "1/3", "0.75 beats",
 * "2 bars". Returns the rate to store, or undefined if it isn't a length. */
export function parseLength(text: string): number | undefined {
  const m = /^\s*([\d.]+)\s*(?:\/\s*([\d.]+))?\s*(beats?|bars?)?\s*$/i.exec(text);
  if (!m) return undefined;
  let length = Number(m[1]) / (m[2] === undefined ? 1 : Number(m[2]));
  if (m[3] && m[3].toLowerCase().startsWith("bar")) length *= 4;
  if (!Number.isFinite(length) || length <= 0) return undefined;
  return Math.min(MAX_RATE, 1 / length);
}
