import { describe, expect, it } from "vitest";
import { curveValue, formatCurve, parseCurve } from "./curve";

// The same cases as `a_curve_interpolates_between_points_and_loops_round` in
// src-tauri/src/engine/waves.rs: the editor must draw what the engine plays.
describe("curve", () => {
  it("interpolates between points and loops round", () => {
    const tri = parseCurve("0:0, 0.5:1, 1:0");
    expect(curveValue(tri, "linear", 0.25)).toBe(0.5);
    expect(curveValue(tri, "linear", 0.5)).toBe(1);

    const inner = parseCurve(" 0.75 : 1 ,0.25:0, junk, 2:x");
    expect(inner).toEqual([
      [0.25, 0],
      [0.75, 1],
    ]);
    expect(curveValue(inner, "linear", 0.5)).toBe(0.5);
    expect(curveValue(inner, "linear", 0)).toBe(0.5);

    const step = parseCurve("0:0.2, 0.5:0.8");
    expect(curveValue(step, "step", 0.1)).toBe(0.2);
    expect(curveValue(step, "step", 0.6)).toBe(0.8);

    expect(curveValue(parseCurve("0:0, 0.5:1"), "smooth", 0.25)).toBeCloseTo(0.5);
    expect(curveValue([], "linear", 0.3)).toBe(0);
  });

  it("formats sorted and rounded", () => {
    expect(
      formatCurve([
        [0.5, 1 / 3],
        [0, 1],
      ]),
    ).toBe("0:1, 0.5:0.333");
  });
});
