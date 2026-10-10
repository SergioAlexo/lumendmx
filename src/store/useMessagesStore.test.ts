import { describe, expect, it } from "vitest";
import type { LogEntry } from "../lib/showfile/generated";
import { appendEntry, countProblems } from "./useMessagesStore";
import { hasProblem } from "./useHealthStore";

const entry = (id: number, level: string): LogEntry => ({
  id,
  timestamp: "2026-10-10T00:00:00.000Z",
  level,
  source: "test",
  message: `m${id}`,
});

describe("messages", () => {
  it("ignores an entry it already holds and caps the buffer", () => {
    let entries: LogEntry[] = [];
    entries = appendEntry(entries, entry(1, "info"));
    expect(appendEntry(entries, entry(1, "info"))).toBe(entries);
    for (let i = 2; i <= 1100; i++) entries = appendEntry(entries, entry(i, "info"));
    expect(entries).toHaveLength(1000);
    expect(entries[0].id).toBe(101);
  });

  it("counts unseen warnings and errors only", () => {
    const entries = [entry(1, "error"), entry(2, "info"), entry(3, "warn"), entry(4, "error")];
    expect(countProblems(entries, 0)).toBe(3);
    expect(countProblems(entries, 3)).toBe(1);
    expect(countProblems(entries, 4)).toBe(0);
  });
});

describe("health", () => {
  it("flags a stalled engine or stalled outputs", () => {
    const ok = {
      engine: "ok" as const,
      ticks: 1,
      lastTickAgeMs: 5,
      lastTickUs: 400,
      maxTickUs: 900,
      slowTicks: 0,
      tickPanics: 0,
      stalledOutputs: [],
    };
    expect(hasProblem(null)).toBe(false);
    expect(hasProblem(ok)).toBe(false);
    expect(hasProblem({ ...ok, engine: "stalled" })).toBe(true);
    expect(hasProblem({ ...ok, stalledOutputs: [2] })).toBe(true);
  });
});
