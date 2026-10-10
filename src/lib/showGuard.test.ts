import { describe, expect, it, vi, beforeEach } from "vitest";
import { checkEditAllowed, needsDeleteConfirmation } from "./showGuard";

describe("showGuard", () => {
  it("allows edits until show mode is on", () => {
    expect(checkEditAllowed(false, "delete a cue")).toEqual({ allowed: true });
    const blocked = checkEditAllowed(true, "delete a cue");
    expect(blocked.allowed).toBe(false);
    expect(blocked.message).toContain("delete a cue");
  });

  it("confirms deletes only when asked to and not already blocked", () => {
    expect(needsDeleteConfirmation(true, false)).toBe(true);
    expect(needsDeleteConfirmation(false, false)).toBe(false);
    expect(needsDeleteConfirmation(true, true)).toBe(false);
  });
});

vi.mock("./reliability", () => ({
  showModeGet: vi.fn(async () => false),
  showModeSet: vi.fn(async () => undefined),
  logWrite: vi.fn(async () => undefined),
  prefsGet: vi.fn(),
  prefsSet: vi.fn(),
}));

describe("show mode store", () => {
  beforeEach(async () => {
    const { useShowModeStore } = await import("../store/useShowModeStore");
    useShowModeStore.setState({ locked: false });
  });

  it("guardEdit follows the lock and logs the refusal", async () => {
    const { useShowModeStore, guardEdit } = await import("../store/useShowModeStore");
    const reliability = await import("./reliability");
    expect(guardEdit("record a cue")).toBe(true);
    await useShowModeStore.getState().setLocked(true);
    expect(reliability.showModeSet).toHaveBeenCalledWith(true);
    expect(guardEdit("record a cue")).toBe(false);
    expect(reliability.logWrite).toHaveBeenCalledWith("warn", "show mode", expect.stringContaining("record a cue"));
  });

  it("confirmDelete asks only when the preference is on", async () => {
    const { useShowModeStore, confirmDelete } = await import("../store/useShowModeStore");
    const { usePrefsStore, DEFAULT_PREFS } = await import("../store/usePrefsStore");
    const confirm = vi.fn(() => true);
    vi.stubGlobal("window", { confirm });
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, confirmOnDelete: true } });
    expect(confirmDelete("group A")).toBe(true);
    expect(confirm).toHaveBeenCalledTimes(1);

    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, confirmOnDelete: false } });
    expect(confirmDelete("group A")).toBe(true);
    expect(confirm).toHaveBeenCalledTimes(1);

    useShowModeStore.setState({ locked: true });
    expect(confirmDelete("group A")).toBe(false);
    vi.unstubAllGlobals();
  });
});
