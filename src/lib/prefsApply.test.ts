import { describe, expect, it } from "vitest";
import { applyPreferencesToRoot, rootFontSizePx } from "./prefsApply";

describe("prefsApply", () => {
  it("scales the root font size and clamps to the supported range", () => {
    expect(rootFontSizePx(1)).toBe(16);
    expect(rootFontSizePx(1.25)).toBe(20);
    expect(rootFontSizePx(5)).toBe(24);
    expect(rootFontSizePx(0.1)).toBe(12);
    expect(rootFontSizePx(Number.NaN)).toBe(16);
  });

  it("writes the scale and touch class onto the root element", () => {
    const classes = new Set<string>();
    const root = {
      style: { fontSize: "" },
      classList: {
        toggle(name: string, force?: boolean) {
          if (force) classes.add(name);
          else classes.delete(name);
          return !!force;
        },
      },
    };
    applyPreferencesToRoot(root, { uiScale: 1.5, touchMode: true });
    expect(root.style.fontSize).toBe("24px");
    expect(classes.has("touch")).toBe(true);
    applyPreferencesToRoot(root, { uiScale: 1, touchMode: false });
    expect(classes.has("touch")).toBe(false);
  });
});
