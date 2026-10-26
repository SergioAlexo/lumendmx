// A Gradient node's "stops" param: "#rrggbb" or "#rrggbb@0.25" (a place along
// the gradient, 0-1), comma-separated. Mirrors src-tauri/src/graph/gradient.rs,
// so the editor draws what the engine plays.

export type Stop = { color: string; place: number };

const HEX = /^#[0-9a-f]{6}$/i;
const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

/** The valid stops, sorted by place. A stop without a place gets its even share. */
export function parseStops(text: string, wrap: boolean): Stop[] {
  const tokens = text.split(",").flatMap((token) => {
    const [color, place] = token.split("@");
    const hex = color.trim().toLowerCase();
    if (!HEX.test(hex)) return [];
    const parsed = place === undefined ? NaN : Number(place);
    return [{ color: hex, place: Number.isFinite(parsed) ? clamp01(parsed) : undefined }];
  });
  const n = tokens.length;
  const stops = tokens.map((t, i) => ({ color: t.color, place: t.place ?? (n < 2 ? 0 : wrap ? i / n : i / (n - 1)) }));
  return stops.sort((a, b) => a.place - b.place);
}

const round = (v: number) => String(Number(v.toFixed(3)));

export function formatStops(stops: Stop[]): string {
  return [...stops]
    .sort((a, b) => a.place - b.place)
    .map((s) => `${s.color}@${round(s.place)}`)
    .join(", ");
}

const rgb = (hex: string): [number, number, number] => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)) as [number, number, number];
const toHex = (c: number[]) => `#${c.map((v) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, "0")).join("")}`;
const mix = (a: string, b: string, t: number) => {
  const [x, y] = [rgb(a), rgb(b)];
  return toHex(x.map((v, i) => v + (y[i] - v) * t));
};

/** The colour at `t`, as the engine blends it. */
export function colorAt(stops: Stop[], wrap: boolean, t: number): string {
  if (stops.length === 0) return "#000000";
  if (stops.length === 1) return stops[0].color;
  const first = stops[0];
  const last = stops[stops.length - 1];
  t = wrap ? ((t % 1) + 1) % 1 : clamp01(t);
  if (t >= first.place && t < last.place) {
    let i = stops.length - 2;
    while (i > 0 && stops[i].place > t) i--;
    const span = stops[i + 1].place - stops[i].place;
    return span <= 0 ? stops[i + 1].color : mix(stops[i].color, stops[i + 1].color, (t - stops[i].place) / span);
  }
  if (!wrap) return t < first.place ? first.color : last.color;
  const span = first.place + 1 - last.place;
  const at = t >= last.place ? t : t + 1;
  return span <= 0 ? first.color : mix(last.color, first.color, (at - last.place) / span);
}

/** A CSS gradient with the stops where the engine puts them. */
export function stopsCss(stops: Stop[], wrap: boolean): string {
  if (stops.length === 0) return "transparent";
  if (stops.length === 1) return stops[0].color;
  const parts = stops.map((s) => `${s.color} ${(s.place * 100).toFixed(2)}%`);
  // Wrapped, the ends are where the last stop is part-way back to the first.
  if (wrap) {
    parts.unshift(`${colorAt(stops, true, 0)} 0%`);
    parts.push(`${colorAt(stops, true, 1 - 1e-9)} 100%`);
  }
  return `linear-gradient(to right, ${parts.join(", ")})`;
}
