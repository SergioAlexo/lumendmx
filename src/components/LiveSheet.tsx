import { useEffect, useMemo, useState } from "react";
import * as engineApi from "../lib/engine";
import { Button } from "./ui/button";
import { cn } from "../lib/utils";
import { useUniverseChannelsStore } from "../store/useUniverseChannelsStore";
import { useSelectionStore } from "../store/useSelectionStore";
import { formatDmxValue, type ValueFormat } from "../lib/visualisation/dmxFormat";
import { SOURCE_CLASSES, SOURCE_LABELS, sourceName, type ChannelSourceName } from "../lib/visualisation/sources";
import { useVisualFixtures } from "../lib/visualisation/useVisualFixtures";
import type { ChannelType } from "../lib/fixtures/types";

/** How often the winning-source map is re-queried from the engine. Values
 * themselves arrive with the engine's frame event; sources change rarely. */
const SOURCE_POLL_MS = 250;

const COLUMN_ORDER: ChannelType[] = [
  "dimmer", "red", "green", "blue", "white", "amber", "uv", "cyan", "magenta", "yellow",
  "colorwheel", "pan", "tilt", "zoom", "focus", "iris", "frost", "gobo", "prism", "strobe", "speed",
  "function", "macro", "control", "lamp", "reset", "generic",
];

function columnRank(type: ChannelType): number {
  const i = COLUMN_ORDER.indexOf(type);
  return i === -1 ? COLUMN_ORDER.length : i;
}

function sameSources(a: Record<number, number[]>, b: Record<number, number[]>): boolean {
  const keys = Object.keys(b);
  if (Object.keys(a).length !== keys.length) return false;
  return keys.every((k) => {
    const x = a[Number(k)];
    const y = b[Number(k)];
    return x !== undefined && x.length === y.length && x.every((v, i) => v === y[i]);
  });
}

/** Polls the engine for each universe's per-channel winning source. */
function useUniverseSources(universes: number[]): Record<number, number[]> {
  const [sources, setSources] = useState<Record<number, number[]>>({});
  const key = universes.join(",");
  useEffect(() => {
    let cancelled = false;
    const ids = key ? key.split(",").map(Number) : [];
    const poll = async () => {
      try {
        const entries = await Promise.all(ids.map(async (id) => [id, await engineApi.getUniverseSources(id)] as const));
        if (cancelled) return;
        const next: Record<number, number[]> = Object.fromEntries(entries);
        // Sources rarely change: keep the old object so the table doesn't re-render.
        setSources((prev) => (sameSources(prev, next) ? prev : next));
      } catch {
        // Outside the Tauri runtime (browser preview) there is no engine: every cell stays "default".
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), SOURCE_POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [key]);
  return sources;
}

const LEGEND: ChannelSourceName[] = ["programmer", "playback", "tracked", "effect", "default"];

/** Spreadsheet of patched fixtures x attributes with live output values,
 * colour-coded by which layer currently controls each channel. */
export function LiveSheet() {
  const fixtures = useVisualFixtures();
  const frames = useUniverseChannelsStore((s) => s.channels);
  const selected = useSelectionStore((s) => s.selected);
  const select = useSelectionStore((s) => s.select);
  const toggle = useSelectionStore((s) => s.toggle);
  const [format, setFormat] = useState<ValueFormat>("percent");

  const universes = useMemo(() => [...new Set(fixtures.map((f) => f.instance.universe))].sort((a, b) => a - b), [fixtures]);
  const sources = useUniverseSources(universes);

  const columns = useMemo(() => {
    const types = new Set<ChannelType>();
    for (const { mode } of fixtures) for (const ch of mode.channels) types.add(ch.type);
    return [...types].sort((a, b) => columnRank(a) - columnRank(b));
  }, [fixtures]);

  if (fixtures.length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-8 text-sm text-muted-foreground">
        No fixtures patched. Patch fixtures in Fixtures or Setup to see them here.
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex flex-wrap items-center gap-3 border-b border-border px-4 py-2">
        <div className="flex gap-1">
          {(["percent", "raw", "hex"] as ValueFormat[]).map((f) => (
            <Button key={f} size="sm" className="h-11 min-w-11" variant={format === f ? "default" : "outline"} onClick={() => setFormat(f)}>
              {f === "percent" ? "%" : f === "raw" ? "0-255" : "Hex"}
            </Button>
          ))}
        </div>
        <div className="ml-auto flex flex-wrap items-center gap-2 text-xs">
          {LEGEND.map((name) => (
            <span key={name} className={cn("rounded-sm px-2 py-1", SOURCE_CLASSES[name])}>
              {SOURCE_LABELS[name]}
            </span>
          ))}
        </div>
      </div>
      <div className="flex-1 overflow-auto">
        <table className="w-full border-separate border-spacing-0 text-xs tabular-nums">
          <thead className="sticky top-0 z-10 bg-card">
            <tr>
              <th className="border-b border-border px-3 py-2 text-left font-medium">#</th>
              <th className="border-b border-border px-3 py-2 text-left font-medium">Fixture</th>
              {columns.map((c) => (
                <th key={c} className="border-b border-border px-2 py-2 text-center font-medium capitalize">
                  {c}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {fixtures.map(({ instance, mode }) => {
              const isSelected = selected.includes(instance.fixtureNumber);
              const frame = frames[instance.universe];
              const universeSources = sources[instance.universe];
              return (
                <tr
                  key={instance.id}
                  className={cn("cursor-pointer hover:bg-accent/40", isSelected && "bg-accent")}
                  onClick={(e) => (e.shiftKey || e.metaKey || e.ctrlKey ? toggle(instance.fixtureNumber) : select([instance.fixtureNumber]))}
                >
                  <td className="border-b border-border px-3 py-2 font-medium">{instance.fixtureNumber}</td>
                  <td className="whitespace-nowrap border-b border-border px-3 py-2">{instance.name}</td>
                  {columns.map((type) => {
                    const ch = mode.channels.find((c) => c.type === type);
                    if (!ch) return <td key={type} className="border-b border-border" />;
                    const channel = instance.address + ch.offset - 1;
                    const value = frame?.[channel - 1] ?? 0;
                    const name = sourceName(universeSources?.[channel - 1]);
                    return (
                      <td
                        key={type}
                        title={`Ch ${channel} — ${SOURCE_LABELS[name]}`}
                        className={cn("h-11 border-b border-border px-2 text-center", SOURCE_CLASSES[name])}
                      >
                        {formatDmxValue(value, format)}
                      </td>
                    );
                  })}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
