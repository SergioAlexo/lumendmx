import { describe, expect, it } from "vitest";
import { colorAt, formatStops, parseStops } from "./gradient";

describe("gradient stops", () => {
  it("spreads stops without a place evenly, and keeps the places given", () => {
    expect(parseStops("#ff0000, #00ff00, #0000ff", false).map((s) => s.place)).toEqual([0, 0.5, 1]);
    expect(parseStops("#ff0000, #00ff00, #0000ff, #ffffff", true).map((s) => s.place)).toEqual([0, 0.25, 0.5, 0.75]);
    expect(parseStops("#0000FF@0.9, junk, #ff0000@0.2", false)).toEqual([
      { color: "#ff0000", place: 0.2 },
      { color: "#0000ff", place: 0.9 },
    ]);
  });

  it("writes stops sorted, with their places", () => {
    expect(formatStops([{ color: "#0000ff", place: 1 }, { color: "#ff0000", place: 0.12345 }])).toBe("#ff0000@0.123, #0000ff@1");
  });

  it("blends like the engine: held past the ends, or round from last to first when wrapped", () => {
    const stops = parseStops("#ff0000@0.25, #0000ff@0.75", false);
    expect(colorAt(stops, false, 0)).toBe("#ff0000");
    expect(colorAt(stops, false, 1)).toBe("#0000ff");
    expect(colorAt(stops, false, 0.5)).toBe("#800080");
    expect(colorAt(stops, true, 0)).toBe("#800080"); // halfway from 0.75 round to 0.25
    expect(colorAt(stops, true, 0.75)).toBe("#0000ff");
  });
});
