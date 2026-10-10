import { describe, expect, it } from "vitest";
import type { FixtureChannel, FixtureDefinition } from "../fixtures/types";
import { deriveFixtureLook, litColor, parseHexColor, rgbToHex } from "./fixtureLook";
import { formatDmxValue } from "./dmxFormat";
import { clampToStage, layoutFixtures, metresToPx, pxToMetres, snap, STAGE_DEPTH_M, STAGE_WIDTH_M } from "./stage";
import { sourceName } from "./sources";

function def(channels: FixtureChannel[], extra: Partial<FixtureDefinition> = {}): FixtureDefinition {
  return {
    id: "t",
    manufacturer: "T",
    model: "T",
    type: "Par",
    modes: [{ name: "m", channelCount: channels.length, channels }],
    ...extra,
  };
}

/** Reader over a flat array of values starting at channel 1 of universe 1. */
function reader(values: number[]) {
  return (universe: number, channel: number) => (universe === 1 ? (values[channel - 1] ?? 0) : 0);
}

const inst = { address: 1, universe: 1 };

describe("deriveFixtureLook", () => {
  const rgbd = def([
    { offset: 1, type: "dimmer" },
    { offset: 2, type: "red" },
    { offset: 3, type: "green" },
    { offset: 4, type: "blue" },
  ]);

  it("separates brightness (dimmer x colour peak) from the colour itself", () => {
    const look = deriveFixtureLook(rgbd, rgbd.modes[0], inst, reader([255, 128, 0, 0]));
    expect(look.color).toEqual({ r: 255, g: 0, b: 0 });
    expect(look.intensity).toBeCloseTo(128 / 255);
    expect(rgbToHex(litColor(look))).toBe("#800000");
  });

  it("is dark when the dimmer is closed", () => {
    const look = deriveFixtureLook(rgbd, rgbd.modes[0], inst, reader([0, 255, 255, 255]));
    expect(look.intensity).toBe(0);
  });

  it("uses the colour peak as intensity when there is no dimmer", () => {
    const d = def([
      { offset: 1, type: "red" },
      { offset: 2, type: "green" },
      { offset: 3, type: "blue" },
    ]);
    const look = deriveFixtureLook(d, d.modes[0], inst, reader([0, 51, 0]));
    expect(look.intensity).toBeCloseTo(0.2);
    expect(look.color).toEqual({ r: 0, g: 255, b: 0 });
  });

  it("treats a lone dimmer as a white fixture", () => {
    const d = def([{ offset: 1, type: "dimmer" }]);
    const look = deriveFixtureLook(d, d.modes[0], inst, reader([255]));
    expect(look.color).toEqual({ r: 255, g: 255, b: 255 });
    expect(look.intensity).toBe(1);
  });

  it("mixes white and amber additively and subtracts CMY from white", () => {
    const rgbw = def([
      { offset: 1, type: "red" },
      { offset: 2, type: "white" },
    ]);
    const mixed = deriveFixtureLook(rgbw, rgbw.modes[0], inst, reader([0, 255]));
    expect(mixed.color).toEqual({ r: 255, g: 255, b: 255 });

    const cmy = def([
      { offset: 1, type: "cyan" },
      { offset: 2, type: "magenta" },
      { offset: 3, type: "yellow" },
    ]);
    const look = deriveFixtureLook(cmy, cmy.modes[0], inst, reader([255, 0, 0]));
    expect(look.color).toEqual({ r: 0, g: 255, b: 255 });
  });

  it("filters through a colour wheel slot", () => {
    const d = def([
      { offset: 1, type: "dimmer" },
      {
        offset: 2,
        type: "colorwheel",
        capabilities: [
          { dmxRange: [0, 9], type: "colorwheel" },
          { dmxRange: [10, 19], type: "colorwheel", color: "#0000ff" },
        ],
      },
    ]);
    expect(deriveFixtureLook(d, d.modes[0], inst, reader([255, 15])).color).toEqual({ r: 0, g: 0, b: 255 });
    expect(deriveFixtureLook(d, d.modes[0], inst, reader([255, 5])).color).toEqual({ r: 255, g: 255, b: 255 });
  });

  it("reads pan/tilt from physical ranges, 16-bit pairs, inversion and swap", () => {
    const mover = def(
      [
        { offset: 1, type: "pan", fineOffset: 2, physicalRange: { min: 0, max: 540, unit: "deg" } },
        { offset: 3, type: "tilt", physicalRange: { min: 0, max: 270, unit: "deg" } },
        { offset: 4, type: "dimmer" },
      ],
      { physical: { beamAngleDeg: [10, 30] } },
    );
    const look = deriveFixtureLook(mover, mover.modes[0], inst, reader([255, 255, 255, 255]));
    expect(look.moving).toBe(true);
    expect(look.panDeg).toBeCloseTo(270);
    expect(look.tiltDeg).toBeCloseTo(135);

    const centred = deriveFixtureLook(mover, mover.modes[0], inst, reader([128, 0, 128, 255]));
    expect(centred.panDeg).toBeCloseTo(0.5, 0);

    const inverted = deriveFixtureLook(mover, mover.modes[0], { ...inst, invertPan: true }, reader([255, 255, 0, 255]));
    expect(inverted.panDeg).toBeCloseTo(-270);

    const swapped = deriveFixtureLook(mover, mover.modes[0], { ...inst, swapPanTilt: true }, reader([255, 255, 0, 255]));
    expect(swapped.panDeg).toBeCloseTo(-135);
    expect(swapped.tiltDeg).toBeCloseTo(270);
  });

  it("interpolates the beam angle over the zoom channel", () => {
    const d = def(
      [
        { offset: 1, type: "dimmer" },
        { offset: 2, type: "zoom" },
      ],
      { physical: { beamAngleDeg: [10, 40] } },
    );
    expect(deriveFixtureLook(d, d.modes[0], inst, reader([255, 0])).beamAngleDeg).toBe(10);
    expect(deriveFixtureLook(d, d.modes[0], inst, reader([255, 255])).beamAngleDeg).toBe(40);
    const plain = def([{ offset: 1, type: "dimmer" }]);
    expect(deriveFixtureLook(plain, plain.modes[0], inst, reader([255])).beamAngleDeg).toBeGreaterThan(0);
  });

  it("averages the cells of a multi-cell fixture, weighted by brightness", () => {
    const bar = def([
      { offset: 1, type: "red", cell: 1 },
      { offset: 2, type: "green", cell: 1 },
      { offset: 3, type: "blue", cell: 1 },
      { offset: 4, type: "red", cell: 2 },
      { offset: 5, type: "green", cell: 2 },
      { offset: 6, type: "blue", cell: 2 },
    ]);
    const look = deriveFixtureLook(bar, bar.modes[0], inst, reader([255, 0, 0, 0, 0, 0]));
    expect(look.color).toEqual({ r: 255, g: 0, b: 0 });
    expect(look.intensity).toBeCloseTo(0.5);
  });
});

describe("helpers", () => {
  it("parses hex colours", () => {
    expect(parseHexColor("#f00")).toEqual({ r: 255, g: 0, b: 0 });
    expect(parseHexColor("00ff00")).toEqual({ r: 0, g: 255, b: 0 });
    expect(parseHexColor("nope")).toBeNull();
  });

  it("formats DMX values", () => {
    expect(formatDmxValue(255, "raw")).toBe("255");
    expect(formatDmxValue(128, "percent")).toBe("50");
    expect(formatDmxValue(10, "hex")).toBe("0A");
    expect(formatDmxValue(999, "raw")).toBe("255");
  });

  it("maps source codes to names, defaulting unknown codes", () => {
    expect(sourceName(1)).toBe("programmer");
    expect(sourceName(3)).toBe("tracked");
    expect(sourceName(4)).toBe("effect");
    expect(sourceName(99)).toBe("default");
    expect(sourceName(undefined)).toBe("default");
  });
});

describe("stage layout", () => {
  it("keeps saved positions (clamped) and lays unplaced fixtures out on the stage", () => {
    const layout = layoutFixtures([
      { id: "a", stage: { x: 99, y: -3, z: 2 } },
      { id: "b" },
      { id: "c" },
    ]);
    expect(layout.get("a")).toEqual({ position: { x: STAGE_WIDTH_M, y: 0, z: 2 }, placed: true });
    const b = layout.get("b")!;
    const c = layout.get("c")!;
    expect(b.placed).toBe(false);
    expect(b.position.x).toBeLessThan(c.position.x);
    expect(c.position.x).toBeLessThan(STAGE_WIDTH_M);
    expect(b.position.y).toBeLessThan(STAGE_DEPTH_M);
  });

  it("round-trips metres and pixels with the front of stage at the bottom", () => {
    const px = metresToPx({ x: 6, y: 0 }, 600, 400);
    expect(px).toEqual({ x: 300, y: 400 });
    expect(pxToMetres(px, 600, 400)).toEqual({ x: 6, y: 0 });
  });

  it("snaps and clamps", () => {
    expect(snap(1.1)).toBe(1);
    expect(snap(1.2)).toBe(1.25);
    expect(clampToStage({ x: -1, y: 100, z: 100 }).y).toBe(STAGE_DEPTH_M);
  });
});
