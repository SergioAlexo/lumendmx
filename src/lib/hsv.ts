/** Colour-picker values for the HSV node: "#rrggbb" to and from hue, saturation and value (each 0-1), like the engine's `hsv_to_rgb`. */

export function hsvToHex(h: number, s: number, v: number): string {
  const hue = (((h % 1) + 1) % 1) * 6;
  const sat = Math.min(1, Math.max(0, s));
  const val = Math.min(1, Math.max(0, v));
  const c = val * sat;
  const x = c * (1 - Math.abs((hue % 2) - 1));
  const [r, g, b] = [[c, x, 0], [x, c, 0], [0, c, x], [0, x, c], [x, 0, c], [c, 0, x]][Math.floor(hue) % 6];
  const m = val - c;
  return `#${[r, g, b].map((p) => Math.round((p + m) * 255).toString(16).padStart(2, "0")).join("")}`;
}

export function hexToHsv(hex: string): { h: number; s: number; v: number } {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  const max = Math.max(r, g, b);
  const d = max - Math.min(r, g, b);
  let h = 0;
  if (d > 0) {
    if (max === r) h = ((g - b) / d + 6) % 6;
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
  }
  return { h: h / 6, s: max === 0 ? 0 : d / max, v: max };
}
