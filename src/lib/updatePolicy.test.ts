import { describe, expect, it } from "vitest";
import { decideInstall, releasePageUrl, shouldRunDeferred } from "./updatePolicy";

describe("updatePolicy", () => {
  it("installs only when there is an update and show mode is off", () => {
    expect(decideInstall({ showMode: false, hasUpdate: true, installing: false })).toEqual({ action: "install" });
    expect(decideInstall({ showMode: false, hasUpdate: false, installing: false })).toEqual({ action: "none" });
    expect(decideInstall({ showMode: false, hasUpdate: true, installing: true })).toEqual({ action: "none" });
  });

  it("defers during show mode", () => {
    const decision = decideInstall({ showMode: true, hasUpdate: true, installing: false });
    expect(decision.action).toBe("defer");
  });

  it("runs a deferred install once show mode ends, and only if one was requested", () => {
    const ready = { showMode: false, hasUpdate: true, installing: false };
    expect(shouldRunDeferred(true, ready)).toBe(true);
    expect(shouldRunDeferred(false, ready)).toBe(false);
    expect(shouldRunDeferred(true, { ...ready, showMode: true })).toBe(false);
  });

  it("builds a release page link with or without a leading v", () => {
    expect(releasePageUrl("a/b", "1.2.3")).toBe("https://github.com/a/b/releases/tag/v1.2.3");
    expect(releasePageUrl("a/b", "v1.2.3")).toBe("https://github.com/a/b/releases/tag/v1.2.3");
  });
});
