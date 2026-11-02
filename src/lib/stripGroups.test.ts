import { describe, expect, it } from "vitest";
import { GROUP_HEADER_H, GROUP_PAD, groupAt, groupBoxes, groupName } from "./stripGroups";

const rect = (x: number, y: number, w = 80, h = 40) => ({ x, y, w, h });

describe("strip groups", () => {
  it("reads a group from a control's params", () => {
    expect(groupName({ panelGroup: " Chase " })).toBe("Chase");
    expect(groupName({ panelGroup: "  " })).toBeNull();
    expect(groupName({})).toBeNull();
    expect(groupName({ panelGroup: 3 })).toBeNull();
  });

  it("wraps a group's members with padding and a title row", () => {
    const boxes = groupBoxes(
      [
        { name: "A", rect: rect(100, 100) },
        { name: "A", rect: rect(200, 120) },
        { name: null, rect: rect(0, 0) },
      ],
      new Set(),
    );
    expect([...boxes.keys()]).toEqual(["A"]);
    expect(boxes.get("A")).toEqual({
      x: 100 - GROUP_PAD,
      y: 100 - GROUP_PAD - GROUP_HEADER_H,
      w: 180 + 2 * GROUP_PAD,
      h: 60 + 2 * GROUP_PAD + GROUP_HEADER_H,
    });
  });

  it("shrinks a folded group to its title", () => {
    const box = groupBoxes([{ name: "A", rect: rect(100, 100) }], new Set(["A"])).get("A")!;
    expect(box.h).toBe(GROUP_HEADER_H);
    expect(box.x).toBe(100 - GROUP_PAD);
  });

  it("finds the group a dropped control lands in", () => {
    const boxes = groupBoxes([{ name: "A", rect: rect(100, 100) }], new Set());
    expect(groupAt(boxes, rect(110, 110))).toBe("A");
    expect(groupAt(boxes, rect(600, 110))).toBeNull();
  });
});
