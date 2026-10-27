import { describe, expect, it } from "vitest";
import { parseLength, rateLabel } from "./beatRate";

describe("beat rate", () => {
  it("names stored rates by their cycle length", () => {
    expect(rateLabel(1)).toBe("1 beat");
    expect(rateLabel(2)).toBe("1/2 beat");
    expect(rateLabel(0.25)).toBe("4 beats (1 bar)");
    expect(rateLabel(3)).toBe("1/3 beat");
    expect(rateLabel(1 / 2.5)).toBe("2.5 beats");
    expect(rateLabel(0)).toBe("stopped");
  });

  it("reads typed lengths back as rates", () => {
    expect(parseLength("1/4")).toBe(4);
    expect(parseLength("2 beats")).toBe(0.5);
    expect(parseLength("0.75")).toBeCloseTo(1 / 0.75);
    expect(parseLength("2 bars")).toBe(0.125);
    expect(parseLength("1/200")).toBe(64); // no faster than the rate limit
    expect(parseLength("fast")).toBeUndefined();
    expect(parseLength("0")).toBeUndefined();
  });
});
