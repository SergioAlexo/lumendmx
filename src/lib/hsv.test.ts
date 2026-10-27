import { describe, expect, it } from "vitest";
import { hexToHsv, hsvToHex } from "./hsv";

describe("hsv colours", () => {
  it("makes the primary colours", () => {
    expect(hsvToHex(0, 1, 1)).toBe("#ff0000");
    expect(hsvToHex(1 / 3, 1, 1)).toBe("#00ff00");
    expect(hsvToHex(2 / 3, 1, 1)).toBe("#0000ff");
    expect(hsvToHex(0, 0, 1)).toBe("#ffffff");
    expect(hsvToHex(0.5, 1, 0)).toBe("#000000");
  });

  it("wraps the hue", () => {
    expect(hsvToHex(1, 1, 1)).toBe("#ff0000");
    expect(hsvToHex(-1 / 3, 1, 1)).toBe("#0000ff");
  });

  it("round-trips a picked colour", () => {
    for (const hex of ["#ff0000", "#f89b29", "#20d0ff", "#808080", "#123456"]) {
      const { h, s, v } = hexToHsv(hex);
      expect(hsvToHex(h, s, v)).toBe(hex);
    }
  });
});
