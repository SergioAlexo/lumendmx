import { describe, expect, it } from "vitest";
import type { FixtureMode } from "../../lib/fixtures/types";
import { strobeHz } from "./fixtureLook";

const mode = (strobe: FixtureMode["channels"][number]): FixtureMode => ({ name: "m", channelCount: 2, channels: [{ offset: 1, type: "dimmer" }, strobe] });

describe("strobeHz", () => {
  const par = mode({
    offset: 2,
    type: "strobe",
    capabilities: [
      { dmxRange: [0, 0], type: "nofunction" },
      { dmxRange: [1, 255], type: "strobe", physicalRange: { min: 0, max: 20, unit: "hz" } },
    ],
  });

  it("is still when the strobe channel is at 0", () => {
    expect(strobeHz(par, { 2: 0 })).toBe(0);
  });

  it("follows the capability's Hz range", () => {
    expect(strobeHz(par, { 2: 255 })).toBe(20);
    expect(strobeHz(par, { 2: 128 })).toBeCloseTo(10, 0);
  });

  it("runs a plain strobe channel from 1 to 20 Hz", () => {
    const plain = mode({ offset: 2, type: "strobe" });
    expect(strobeHz(plain, { 2: 1 })).toBe(1);
    expect(strobeHz(plain, { 2: 255 })).toBe(20);
  });

  it("is still for a fixture without a strobe channel", () => {
    expect(strobeHz({ name: "m", channelCount: 1, channels: [{ offset: 1, type: "dimmer" }] }, { 1: 255 })).toBe(0);
  });
});
