import { useEffect, useMemo, useRef, useState } from "react";
import { Eraser, ImageUp, Play, Plus, Square, Trash2, X } from "lucide-react";
import * as engineApi from "../lib/engine";
import type { Generator, MapDirection, PixelCell, PixelMap, PixelMapStatus } from "../lib/engine";
import { useAllFixtures } from "../store/useFixtureLibraryStore";
import { usePatchStore } from "../store/usePatchStore";
import { Button } from "./ui/button";
import { Input, Select } from "./ui/input";
import { ValueSlider } from "./ui/value-slider";
import { cn } from "../lib/utils";

const GENERATORS: { id: Generator; label: string }[] = [
  { id: "rainbow", label: "Rainbow" },
  { id: "colourChase", label: "Colour chase" },
  { id: "wave", label: "Wave" },
  { id: "plasma", label: "Plasma" },
  { id: "textScroll", label: "Text" },
  { id: "fire", label: "Fire" },
  { id: "noise", label: "Noise" },
  { id: "media", label: "Image / GIF" },
];
const DIRECTIONS: MapDirection[] = ["right", "left", "up", "down"];
const USES_COLORS = new Set<Generator>(["colourChase", "wave", "textScroll", "noise"]);
const BEAT_OPTIONS = [1, 2, 4, 8, 16, 32];
const PREVIEW_MS = 50;
const STATUS_MS = 1000;
const MAX_MEDIA_BYTES = 8 * 1024 * 1024;

/** Something a grid cell can be mapped to: a whole fixture (cell 0) or one
 * cell of a multi-cell fixture. */
interface Source {
  fixtureNumber: number;
  cell: number;
  label: string;
}

function useSources(): Source[] {
  const fixtures = usePatchStore((s) => s.fixtures);
  const allFixtures = useAllFixtures();
  return useMemo(() => {
    const sources: Source[] = [];
    for (const f of [...fixtures].sort((a, b) => a.fixtureNumber - b.fixtureNumber)) {
      const mode = allFixtures.find((d) => d.id === f.fixtureId)?.modes[f.modeIndex];
      const cells = [...new Set((mode?.channels ?? []).map((c) => c.cell ?? 0).filter((c) => c > 0))].sort((a, b) => a - b);
      if (cells.length === 0) {
        sources.push({ fixtureNumber: f.fixtureNumber, cell: 0, label: `#${f.fixtureNumber} ${f.name}` });
      } else {
        cells.forEach((_, i) =>
          sources.push({ fixtureNumber: f.fixtureNumber, cell: i + 1, label: `#${f.fixtureNumber} ${f.name} · cell ${i + 1}` }),
        );
      }
    }
    return sources;
  }, [fixtures, allFixtures]);
}

const sourceKey = (s: { fixtureNumber: number; cell: number }) => `${s.fixtureNumber}:${s.cell}`;

function NewMapForm({ onCreated }: { onCreated: (id: number) => void }) {
  const [name, setName] = useState("");
  const [width, setWidth] = useState(10);
  const [height, setHeight] = useState(10);
  const create = () =>
    void engineApi.createPixelMap(name.trim() || "Pixel map", width, height).then((id) => {
      setName("");
      onCreated(id);
    });
  return (
    <div className="flex flex-col gap-2 rounded-md border border-border p-3">
      <h3 className="text-xs font-semibold uppercase text-muted-foreground">New pixel map</h3>
      <Input className="h-8 text-xs" placeholder="Name" value={name} onChange={(e) => setName(e.target.value)} />
      <div className="flex items-center gap-1 text-xs text-muted-foreground">
        <Input type="number" className="h-8 w-16 text-xs" min={1} max={256} value={width} onChange={(e) => setWidth(Number(e.target.value) || 1)} aria-label="Width" />
        ×
        <Input type="number" className="h-8 w-16 text-xs" min={1} max={256} value={height} onChange={(e) => setHeight(Number(e.target.value) || 1)} aria-label="Height" />
        <Button size="sm" className="ml-auto" onClick={create}>
          <Plus />
          Create
        </Button>
      </div>
    </div>
  );
}

/** The grid: live preview colours, mapped-fixture labels, click to map. */
function MapGrid({
  map,
  preview,
  onCellClick,
}: {
  map: PixelMap;
  preview: number[];
  onCellClick: (x: number, y: number) => void;
}) {
  const size = Math.max(10, Math.min(44, Math.floor(520 / Math.max(map.width, map.height))));
  const byPosition = new Map(map.cells.map((c) => [`${c.x},${c.y}`, c]));
  return (
    <div
      className="grid w-fit gap-px rounded border border-border bg-border"
      style={{ gridTemplateColumns: `repeat(${map.width}, ${size}px)` }}
    >
      {Array.from({ length: map.width * map.height }, (_, i) => {
        const x = i % map.width;
        const y = Math.floor(i / map.width);
        const cell = byPosition.get(`${x},${y}`);
        const [r, g, b] = [preview[i * 3] ?? 0, preview[i * 3 + 1] ?? 0, preview[i * 3 + 2] ?? 0];
        const light = r * 0.299 + g * 0.587 + b * 0.114 > 140;
        return (
          <button
            key={i}
            className={cn(
              "flex items-center justify-center text-[9px] leading-none tabular-nums",
              cell ? "outline outline-1 -outline-offset-2 outline-primary" : "",
              light ? "text-black/70" : "text-white/70",
            )}
            style={{ width: size, height: size, backgroundColor: `rgb(${r},${g},${b})` }}
            onClick={() => onCellClick(x, y)}
            title={cell ? `Fixture #${cell.fixtureNumber}${cell.cell ? ` cell ${cell.cell}` : ""} (${x},${y})` : `(${x},${y})`}
          >
            {cell && size >= 18 ? `${cell.fixtureNumber}${cell.cell ? `.${cell.cell}` : ""}` : ""}
          </button>
        );
      })}
    </div>
  );
}

function MapSettings({
  map,
  onChange,
  running,
  onRunningChanged,
}: {
  map: PixelMap;
  onChange: (map: PixelMap) => void;
  running: PixelMapStatus | undefined;
  onRunningChanged: () => void;
}) {
  const set = (patch: Partial<PixelMap>) => onChange({ ...map, ...patch });
  const fileRef = useRef<HTMLInputElement>(null);
  const [mediaError, setMediaError] = useState<string | null>(null);

  const importFile = (file: File) => {
    if (file.size > MAX_MEDIA_BYTES) {
      setMediaError("File is larger than 8 MB.");
      return;
    }
    setMediaError(null);
    void engineApi
      .importPixelMapMedia(map.id, file)
      .then(() => engineApi.listPixelMaps())
      .then((maps) => {
        const updated = maps.find((m) => m.id === map.id);
        if (updated) onChange(updated);
      })
      .catch((e) => setMediaError(String(e)));
  };

  return (
    <div className="flex flex-col gap-3 text-xs">
      <div className="flex items-center gap-2">
        {running ? (
          <Button size="sm" variant="destructive" onClick={() => void engineApi.stopPixelMap(map.id).then(onRunningChanged)}>
            <Square />
            Stop
          </Button>
        ) : (
          <Button size="sm" onClick={() => void engineApi.startPixelMap(map.id).then(onRunningChanged)}>
            <Play />
            Run
          </Button>
        )}
        {running && (
          <ValueSlider
            className="flex-1"
            max={100}
            value={Math.round(running.intensity * 100)}
            onChange={(v) => void engineApi.setPixelMapIntensity(map.id, v).then(onRunningChanged)}
            title="Master"
          />
        )}
      </div>

      <label className="flex items-center gap-2 text-muted-foreground">
        <span className="w-16">Name</span>
        <Input className="h-8 text-xs" value={map.name} onChange={(e) => set({ name: e.target.value })} />
      </label>
      <div className="flex items-center gap-2 text-muted-foreground">
        <span className="w-16">Size</span>
        <Input type="number" className="h-8 w-16 text-xs" min={1} max={256} value={map.width} onChange={(e) => set({ width: Math.max(1, Number(e.target.value) || 1) })} aria-label="Width" />
        ×
        <Input type="number" className="h-8 w-16 text-xs" min={1} max={256} value={map.height} onChange={(e) => set({ height: Math.max(1, Number(e.target.value) || 1) })} aria-label="Height" />
      </div>

      <div className="flex flex-wrap gap-1">
        {GENERATORS.map((g) => (
          <Button
            key={g.id}
            size="sm"
            variant={map.generator === g.id ? "default" : "secondary"}
            className="h-7 px-2 text-xs"
            onClick={() => (g.id === "media" && !map.media ? fileRef.current?.click() : set({ generator: g.id }))}
          >
            {g.label}
          </Button>
        ))}
      </div>

      <div className="flex items-center gap-2 text-muted-foreground">
        <span className="w-16">Rate</span>
        <Select
          className="h-8 w-24 text-xs"
          value={map.rate.kind}
          onChange={(e) => set({ rate: e.target.value === "beats" ? { kind: "beats", beats: 4 } : { kind: "hz", hz: 0.25 } })}
        >
          <option value="beats">BPM sync</option>
          <option value="hz">Hz</option>
        </Select>
        {map.rate.kind === "beats" ? (
          <Select className="h-8 text-xs" value={map.rate.beats} onChange={(e) => set({ rate: { kind: "beats", beats: Number(e.target.value) } })}>
            {[...new Set([map.rate.beats, ...BEAT_OPTIONS])].sort((a, b) => a - b).map((b) => (
              <option key={b} value={b}>
                {b} beat{b === 1 ? "" : "s"} / cycle
              </option>
            ))}
          </Select>
        ) : (
          <Input
            type="number"
            className="h-8 w-20 text-xs"
            min={0}
            step={0.05}
            value={Number(map.rate.hz.toFixed(3))}
            aria-label="Hz"
            onChange={(e) => {
              const hz = Number(e.target.value);
              if (Number.isFinite(hz) && hz >= 0) set({ rate: { kind: "hz", hz } });
            }}
          />
        )}
      </div>

      <div className="flex items-center gap-2 text-muted-foreground">
        <span className="w-16">Direction</span>
        {DIRECTIONS.map((d) => (
          <Button key={d} size="sm" variant={map.direction === d ? "default" : "secondary"} className="h-7 px-2 text-xs" onClick={() => set({ direction: d })}>
            {d.charAt(0).toUpperCase() + d.slice(1)}
          </Button>
        ))}
      </div>

      {USES_COLORS.has(map.generator) && (
        <div className="flex flex-wrap items-center gap-1.5 text-muted-foreground">
          <span className="w-16">Colours</span>
          {map.colors.map((color, i) => (
            <span key={i} className="flex items-center">
              <input
                type="color"
                value={color}
                className="h-7 w-8 cursor-pointer rounded border border-border bg-transparent"
                onChange={(e) => set({ colors: map.colors.map((c, j) => (j === i ? e.target.value : c)) })}
                aria-label={`Colour ${i + 1}`}
              />
              {map.colors.length > 1 && (
                <button className="hover:text-destructive" onClick={() => set({ colors: map.colors.filter((_, j) => j !== i) })} title="Remove colour">
                  <X className="h-3 w-3" />
                </button>
              )}
            </span>
          ))}
          <Button size="iconSm" variant="ghost" onClick={() => set({ colors: [...map.colors, "#ffffff"] })} title="Add colour">
            <Plus />
          </Button>
        </div>
      )}

      {map.generator === "textScroll" && (
        <label className="flex items-center gap-2 text-muted-foreground">
          <span className="w-16">Text</span>
          <Input className="h-8 text-xs" value={map.text} onChange={(e) => set({ text: e.target.value })} />
        </label>
      )}

      <div className="flex items-center gap-2 text-muted-foreground">
        <span className="w-16">Media</span>
        <Button size="sm" variant="secondary" onClick={() => fileRef.current?.click()}>
          <ImageUp />
          {map.media ? "Replace image…" : "Import image / GIF…"}
        </Button>
        {map.media && <span>{map.media.frames.length} frame{map.media.frames.length === 1 ? "" : "s"}</span>}
        <input
          ref={fileRef}
          type="file"
          accept="image/png,image/jpeg,image/gif"
          className="hidden"
          onChange={(e) => {
            const file = e.target.files?.[0];
            if (file) importFile(file);
            e.target.value = "";
          }}
        />
      </div>
      {mediaError && <p className="text-destructive">{mediaError}</p>}
    </div>
  );
}

/** Pixel maps (BUILD_PLAN Phase 7). A map renders even with nothing mapped
 * (a virtual matrix), so the grid doubles as its preview. */
export function PixelMapPanel() {
  const sources = useSources();
  const [maps, setMaps] = useState<PixelMap[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [preview, setPreview] = useState<number[]>([]);
  const [running, setRunning] = useState<PixelMapStatus[]>([]);
  const [sourceIndex, setSourceIndex] = useState(0);
  const [erasing, setErasing] = useState(false);

  const refresh = () => void engineApi.listPixelMaps().then(setMaps).catch(() => undefined);
  const pollRunning = () => void engineApi.runningPixelMaps().then(setRunning).catch(() => undefined);
  useEffect(() => {
    refresh();
    pollRunning();
    const id = setInterval(pollRunning, STATUS_MS);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    if (selectedId === null) return;
    const poll = () => void engineApi.pixelMapPreview(selectedId).then(setPreview).catch(() => undefined);
    poll();
    const id = setInterval(poll, PREVIEW_MS);
    return () => clearInterval(id);
  }, [selectedId]);

  const map = maps.find((m) => m.id === selectedId) ?? null;

  const save = (next: PixelMap) => {
    setMaps((all) => all.map((m) => (m.id === next.id ? next : m)));
    void engineApi.updatePixelMap(next).catch(() => undefined);
  };

  const clickCell = (x: number, y: number) => {
    if (!map) return;
    const others = map.cells.filter((c) => c.x !== x || c.y !== y);
    if (erasing) {
      save({ ...map, cells: others });
      return;
    }
    const source = sources[sourceIndex];
    if (!source) return;
    // A source is mapped to one position: placing it again moves it.
    const cell: PixelCell = { x, y, fixtureNumber: source.fixtureNumber, cell: source.cell };
    save({ ...map, cells: [...others.filter((c) => sourceKey(c) !== sourceKey(source)), cell] });
    setSourceIndex((i) => Math.min(i + 1, sources.length - 1));
  };

  /** Lays every source out row by row, left to right, from the top-left. */
  const fillRows = () => {
    if (!map) return;
    const cells: PixelCell[] = sources.slice(0, map.width * map.height).map((s, i) => ({
      x: i % map.width,
      y: Math.floor(i / map.width),
      fixtureNumber: s.fixtureNumber,
      cell: s.cell,
    }));
    save({ ...map, cells });
  };

  return (
    <div className="flex h-full gap-4 overflow-hidden p-4">
      <div className="flex w-56 shrink-0 flex-col gap-3 overflow-y-auto">
        <NewMapForm
          onCreated={(id) => {
            refresh();
            setSelectedId(id);
          }}
        />
        <div className="flex flex-col gap-1 rounded-md border border-border p-3">
          <h3 className="mb-1 text-xs font-semibold uppercase text-muted-foreground">Pixel maps</h3>
          {maps.map((m) => {
            const isRunning = running.some((r) => r.mapId === m.id);
            return (
              <div key={m.id} className={cn("flex items-center gap-1 rounded px-1 text-xs", m.id === selectedId && "bg-primary/15")}>
                <span className={cn("h-2 w-2 shrink-0 rounded-full", isRunning ? "bg-success" : "bg-muted-foreground/30")} />
                <button className="min-w-0 flex-1 truncate py-1 text-left" onClick={() => setSelectedId(m.id)}>
                  {m.name} <span className="text-muted-foreground">{m.width}×{m.height}</span>
                </button>
                <Button
                  size="iconSm"
                  variant="ghost"
                  title="Delete"
                  onClick={() =>
                    void engineApi.deletePixelMap(m.id).then(() => {
                      if (selectedId === m.id) setSelectedId(null);
                      refresh();
                    })
                  }
                >
                  <Trash2 />
                </Button>
              </div>
            );
          })}
          {maps.length === 0 && <p className="text-xs text-muted-foreground">No pixel maps yet.</p>}
        </div>
      </div>

      {map ? (
        <>
          <div className="flex min-w-0 flex-1 flex-col gap-3 overflow-auto">
            <div className="flex flex-wrap items-center gap-2 text-xs">
              <span className="text-muted-foreground">Click a cell to map</span>
              <Select
                className="h-8 w-56 text-xs"
                value={sourceIndex}
                disabled={erasing || sources.length === 0}
                onChange={(e) => setSourceIndex(Number(e.target.value))}
              >
                {sources.map((s, i) => (
                  <option key={sourceKey(s)} value={i}>
                    {s.label}
                  </option>
                ))}
                {sources.length === 0 && <option>No fixtures patched</option>}
              </Select>
              <Button size="sm" variant={erasing ? "default" : "secondary"} onClick={() => setErasing((e) => !e)} title="Click cells to unmap them">
                <Eraser />
                Erase
              </Button>
              <Button size="sm" variant="secondary" disabled={sources.length === 0} onClick={fillRows} title="Map every fixture (and cell) row by row">
                Fill rows
              </Button>
              <Button size="sm" variant="ghost" onClick={() => save({ ...map, cells: [] })}>
                Clear
              </Button>
              <span className="ml-auto text-muted-foreground">{map.cells.length} mapped</span>
            </div>
            <MapGrid map={map} preview={preview} onCellClick={clickCell} />
          </div>
          <div className="w-80 shrink-0 overflow-y-auto rounded-md border border-border p-3">
            <MapSettings
              map={map}
              onChange={save}
              running={running.find((r) => r.mapId === map.id)}
              onRunningChanged={pollRunning}
            />
          </div>
        </>
      ) : (
        <p className="flex-1 text-center text-sm text-muted-foreground">Select or create a pixel map.</p>
      )}
    </div>
  );
}
