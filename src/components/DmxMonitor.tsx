import { useMemo, useState } from "react";
import { Select } from "./ui/input";
import { Button } from "./ui/button";
import { cn } from "../lib/utils";
import { PRIMARY_UNIVERSE_ID } from "../lib/constants";
import { EMPTY_UNIVERSES, useShowStore } from "../store/useShowStore";
import { useUniverseChannelsStore } from "../store/useUniverseChannelsStore";
import { useSelectionStore } from "../store/useSelectionStore";
import { formatDmxValue, type ValueFormat } from "../lib/visualisation/dmxFormat";
import { useVisualFixtures } from "../lib/visualisation/useVisualFixtures";

const FORMATS: { id: ValueFormat; label: string }[] = [
  { id: "raw", label: "0-255" },
  { id: "percent", label: "%" },
  { id: "hex", label: "Hex" },
];

const EMPTY_FRAME = new Uint8Array(512);

/** Per-universe 512-channel grid of live output values. Read-only: it shows the
 * merged frames the engine emits at ~20Hz (`engine://universe-frame`). */
export function DmxMonitor() {
  const universes = useShowStore((s) => s.meta?.universes) ?? EMPTY_UNIVERSES;
  const frames = useUniverseChannelsStore((s) => s.channels);
  const fixtures = useVisualFixtures();
  const select = useSelectionStore((s) => s.select);
  const [universe, setUniverse] = useState(PRIMARY_UNIVERSE_ID);
  const [format, setFormat] = useState<ValueFormat>("raw");

  const options = useMemo(() => {
    const ids = new Set<number>([PRIMARY_UNIVERSE_ID, ...universes.map((u) => u.id), ...Object.keys(frames).map(Number)]);
    return [...ids].sort((a, b) => a - b);
  }, [universes, frames]);

  // channel (1-based) -> owning fixture + the channel's label, for tooltips and click-to-select.
  const owners = useMemo(() => {
    const map = new Map<number, { number: number; name: string; label: string }>();
    for (const { instance, mode } of fixtures) {
      if (instance.universe !== universe) continue;
      for (const ch of mode.channels) {
        map.set(instance.address + ch.offset - 1, {
          number: instance.fixtureNumber,
          name: instance.name,
          label: ch.label ?? ch.type,
        });
      }
    }
    return map;
  }, [fixtures, universe]);

  const frame = frames[universe] ?? EMPTY_FRAME;
  const universeName = universes.find((u) => u.id === universe)?.name;
  const live = universe in frames;

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex flex-wrap items-center gap-3 border-b border-border px-4 py-2">
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          Universe
          <Select className="h-11 w-48" value={universe} onChange={(e) => setUniverse(Number(e.target.value))}>
            {options.map((id) => (
              <option key={id} value={id}>
                {id}
                {universes.find((u) => u.id === id) ? ` — ${universes.find((u) => u.id === id)?.name}` : ""}
              </option>
            ))}
          </Select>
        </label>
        <div className="flex gap-1">
          {FORMATS.map((f) => (
            <Button
              key={f.id}
              size="sm"
              className="h-11 min-w-11"
              variant={format === f.id ? "default" : "outline"}
              onClick={() => setFormat(f.id)}
            >
              {f.label}
            </Button>
          ))}
        </div>
        <span className="ml-auto text-xs text-muted-foreground">
          {live ? `${universeName ?? `Universe ${universe}`} — live` : "No output frames yet (engine not running)"}
        </span>
      </div>
      <div className="flex-1 overflow-auto p-4">
        <div className="grid gap-1" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(3.25rem, 1fr))" }}>
          {Array.from({ length: 512 }, (_, i) => {
            const value = frame[i] ?? 0;
            const owner = owners.get(i + 1);
            return (
              <button
                key={i}
                type="button"
                title={owner ? `Ch ${i + 1} — #${owner.number} ${owner.name} (${owner.label})` : `Ch ${i + 1}`}
                onClick={() => owner && select([owner.number])}
                className={cn(
                  "relative flex h-11 flex-col items-center justify-center rounded-sm border border-border text-xs tabular-nums",
                  owner ? "cursor-pointer" : "cursor-default text-muted-foreground",
                )}
                style={{ backgroundColor: `hsl(var(--primary) / ${(value / 255) * 0.6})` }}
              >
                <span className="absolute left-0.5 top-0 text-[9px] leading-3 text-muted-foreground">{i + 1}</span>
                <span className="mt-2 font-medium">{formatDmxValue(value, format)}</span>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}
