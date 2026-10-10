import { describe, expect, it } from "vitest";
import { formatTimecode, parseTimecode } from "./timing";

describe("timecode text", () => {
  it("formats milliseconds as hh:mm:ss.ff", () => {
    expect(formatTimecode(0)).toBe("00:00:00.00");
    expect(formatTimecode(3_723_480, 25)).toBe("01:02:03.12");
  });

  it("parses s, m:s and h:m:s", () => {
    expect(parseTimecode("12.5")).toBe(12_500);
    expect(parseTimecode("1:30")).toBe(90_000);
    expect(parseTimecode("1:01:01")).toBe(3_661_000);
    expect(parseTimecode("")).toBeNull();
    expect(parseTimecode("a:b")).toBeNull();
    expect(parseTimecode("-5")).toBeNull();
  });
});
