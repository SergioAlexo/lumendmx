// A Custom LFO's drawn wave, as stored in its "curve" param: "x:y, x:y",
// both 0-1. Mirrors `Curve` in src-tauri/src/engine/waves.rs, so the editor
// draws exactly what the engine plays.

export type CurvePoint = [x: number, y: number];
export type CurveMode = "linear" | "smooth" | "step";

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

/** Reads the points, clamped and sorted by x; unreadable pairs are skipped. */
export function parseCurve(text: string): CurvePoint[] {
  const points: CurvePoint[] = [];
  for (const pair of text.split(",")) {
    const [x, y, ...rest] = pair.split(":");
    if (y === undefined || rest.length > 0 || x.trim() === "" || y.trim() === "") continue;
    const [px, py] = [Number(x), Number(y)];
    if (Number.isFinite(px) && Number.isFinite(py)) points.push([clamp01(px), clamp01(py)]);
  }
  return points.sort((a, b) => a[0] - b[0]);
}

const round = (v: number) => String(Number(v.toFixed(3)));

export function formatCurve(points: CurvePoint[]): string {
  return [...points]
    .sort((a, b) => a[0] - b[0])
    .map(([x, y]) => `${round(x)}:${round(y)}`)
    .join(", ");
}

/** The value at phase `p` (0-1), looping from the last point round to the first. */
export function curveValue(points: CurvePoint[], mode: CurveMode, p: number): number {
  if (points.length === 0) return 0;
  const first = points[0];
  const last = points[points.length - 1];
  const next = points.findIndex(([x]) => x > p);
  const [a, b]: [CurvePoint, CurvePoint] =
    next === 0 ? [[last[0] - 1, last[1]], first] : next > 0 ? [points[next - 1], points[next]] : [last, [first[0] + 1, first[1]]];
  const span = b[0] - a[0];
  let t = span <= 0 ? 1 : (p - a[0]) / span;
  if (mode === "smooth") t = (1 - Math.cos(Math.PI * t)) / 2;
  if (mode === "step") t = 0;
  return a[1] + (b[1] - a[1]) * t;
}
