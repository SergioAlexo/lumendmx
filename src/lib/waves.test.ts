import { describe, expect, it } from "vitest";
import { fanOffsets, fanPosition, lfoValue, type WaveSettings } from "./waves";

const wave = (waveform: string, width = 50): WaveSettings => ({ waveform, width, curve: "0:0, 0.5:1, 1:0", curveMode: "linear" });
const fan = (over: Partial<{ spread: number; groups: number; blocks: number; wings: number }> = {}) => ({ spread: 1, groups: 0, blocks: 0, wings: 0, ...over });

describe("lfoValue", () => {
  it("hits the same points as the engine's waveforms", () => {
    expect(lfoValue(wave("sine"), 0.25)).toBeCloseTo(1);
    expect(lfoValue(wave("sine"), 0.75)).toBeCloseTo(0);
    expect(lfoValue(wave("square"), 0.1)).toBe(1);
    expect(lfoValue(wave("square"), 0.6)).toBe(0);
    expect(lfoValue(wave("square", 75), 0.6)).toBe(1);
    expect(lfoValue(wave("ramp"), 0)).toBe(0);
    expect(lfoValue(wave("saw"), 0)).toBe(1);
    expect(lfoValue(wave("step"), 0.99)).toBe(1);
    expect(lfoValue(wave("pulse", 10), 0.05)).toBeCloseTo(1);
    expect(lfoValue(wave("custom"), 0.25)).toBeCloseTo(0.5);
    expect(lfoValue(wave("random"), 0.3)).toBeUndefined();
  });
});

describe("fan", () => {
  it("spaces four fixtures like the engine's chase test: 0, .75, .5, .25", () => {
    expect(fanOffsets(fan(), 4)).toEqual([0, 0.75, 0.5, 0.25]);
  });

  it("runs the other way with a negative spread, and not at all at zero", () => {
    expect(fanOffsets(fan({ spread: -1 }), 4)).toEqual([0, 0.25, 0.5, 0.75]);
    expect(fanOffsets(fan({ spread: 0 }), 4)).toEqual([0, 0, 0, 0]);
  });

  it("splits into groups, blocks and mirrored wings", () => {
    expect(fanPosition(fan({ groups: 2 }), 3, 8)).toEqual([1, 2]);
    expect(fanPosition(fan({ blocks: 2 }), 3, 8)).toEqual([1, 4]);
    expect(fanPosition(fan({ wings: 2 }), 5, 8)).toEqual([2, 4]); // second wing runs back
  });
});
