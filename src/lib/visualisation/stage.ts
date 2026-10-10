/**
 * Stage geometry shared by the 2D plot and the 3D view. Positions are metres:
 * x left to right, y from the front of the stage (0) upstage, z height above
 * the floor. They are stored on each patched fixture (`stage`, show file v6).
 */
import type { StagePosition } from "../showfile/generated";

export const STAGE_WIDTH_M = 12;
export const STAGE_DEPTH_M = 8;
export const STAGE_HEIGHT_M = 6;
/** Default hang height for fixtures that haven't been given one. */
export const DEFAULT_FIXTURE_HEIGHT_M = 4.5;
/** Dragging in the plot snaps to this grid. */
export const SNAP_M = 0.25;

function clamp(v: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, v));
}

export function clampToStage(p: StagePosition): StagePosition {
  return {
    x: clamp(p.x, 0, STAGE_WIDTH_M),
    y: clamp(p.y, 0, STAGE_DEPTH_M),
    z: clamp(p.z, 0, STAGE_HEIGHT_M),
  };
}

export function snap(value: number, step = SNAP_M): number {
  return Math.round(value / step) * step;
}

/**
 * Where to draw each fixture: its saved position, or - for fixtures not yet
 * placed - a slot on an even grid along a truss line, in patch order, so a
 * fresh show shows something sensible straight away. Returns `placed: false`
 * for the auto-laid-out ones.
 */
export function layoutFixtures<T extends { id: string; stage?: StagePosition }>(
  fixtures: T[],
): Map<string, { position: StagePosition; placed: boolean }> {
  const out = new Map<string, { position: StagePosition; placed: boolean }>();
  const unplaced = fixtures.filter((f) => !f.stage);
  const perRow = Math.max(1, Math.min(unplaced.length, 12));
  unplaced.forEach((f, i) => {
    const row = Math.floor(i / perRow);
    const col = i % perRow;
    const inRow = Math.min(perRow, unplaced.length - row * perRow);
    out.set(f.id, {
      position: {
        x: ((col + 1) * STAGE_WIDTH_M) / (inRow + 1),
        y: STAGE_DEPTH_M * (0.7 - row * 0.15),
        z: DEFAULT_FIXTURE_HEIGHT_M,
      },
      placed: false,
    });
  });
  for (const f of fixtures) {
    if (f.stage) out.set(f.id, { position: clampToStage(f.stage), placed: true });
  }
  return out;
}

/** Metres -> pixels in a plot of `widthPx` x `heightPx` (y flipped: front of stage at the bottom). */
export function metresToPx(p: { x: number; y: number }, widthPx: number, heightPx: number): { x: number; y: number } {
  return { x: (p.x / STAGE_WIDTH_M) * widthPx, y: (1 - p.y / STAGE_DEPTH_M) * heightPx };
}

export function pxToMetres(p: { x: number; y: number }, widthPx: number, heightPx: number): { x: number; y: number } {
  return { x: (p.x / widthPx) * STAGE_WIDTH_M, y: (1 - p.y / heightPx) * STAGE_DEPTH_M };
}
