import { describe, expect, it } from "vitest";
import { formatTimecode } from "./timing";

describe("timecode text", () => {
  it("formats milliseconds as hh:mm:ss.ff", () => {
    expect(formatTimecode(0)).toBe("00:00:00.00");
    expect(formatTimecode(3_723_480, 25)).toBe("01:02:03.12");
  });
});
