import { describe, expect, it } from "vitest";
import { autosaveDue, recoveryNotice, startupPlan } from "./sessionPolicy";

describe("sessionPolicy", () => {
  it("a live session means a webview reload: adopt it, do not reload from disk", () => {
    expect(startupPlan(true)).toBe("adopt-live");
    expect(startupPlan(false)).toBe("restore-from-disk");
  });

  it("explains a restore: crash beats plain unsaved work", () => {
    expect(recoveryNotice(true, true)).toBe("crash");
    expect(recoveryNotice(false, true)).toBe("crash");
    expect(recoveryNotice(true, false)).toBe("unsaved");
    expect(recoveryNotice(false, false)).toBeNull();
  });

  it("autosaves only dirty shows once the interval has passed", () => {
    const base = { dirty: true, nowMs: 100_000, lastAutosaveMs: 40_000, intervalS: 60 };
    expect(autosaveDue(base)).toBe(true);
    expect(autosaveDue({ ...base, nowMs: 99_000 })).toBe(false);
    expect(autosaveDue({ ...base, dirty: false })).toBe(false);
  });
});
