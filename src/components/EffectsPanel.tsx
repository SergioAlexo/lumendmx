import { useEffect, useMemo, useState } from "react";
import { Pencil, Play, Square, Trash2 } from "lucide-react";
import * as engineApi from "../lib/engine";
import type {
  EffectDef,
  EffectDirection,
  EffectInstance,
  EffectMode,
  Group,
  PositionShape,
  Preset,
  Waveform,
} from "../lib/engine";
import { useAllFixtures } from "../store/useFixtureLibraryStore";
import { usePatchStore } from "../store/usePatchStore";
import { Button } from "./ui/button";
import { Input, Select } from "./ui/input";
import { ValueSlider } from "./ui/value-slider";
import { cn } from "../lib/utils";

const WAVEFORMS: Waveform[] = ["sine", "square", "saw", "ramp", "random", "step", "pulse"];
const SHAPES: PositionShape[] = ["circle", "figure8", "ballyhoo", "panSaw", "tiltSaw"];
const SHAPE_LABELS: Record<PositionShape, string> = {
  circle: "Circle",
  figure8: "Figure 8",
  ballyhoo: "Ballyhoo",
  panSaw: "Pan saw",
  tiltSaw: "Tilt saw",
};
const DIRECTIONS: EffectDirection[] = ["forward", "backward", "bounce"];
/** Beats per cycle offered for a BPM-synced effect. */
const BEAT_OPTIONS = [0.25, 0.5, 1, 2, 4, 8, 16];
/** Attributes that don't make sense to run a waveform on. */
const NON_EFFECT_ATTRIBUTES = new Set(["control", "reset", "lamp", "nofunction", "function", "macro", "generic"]);

const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

function defaultDef(attribute = "dimmer"): EffectDef {
  return {
    target: { kind: "attribute", attribute, waveform: "sine" },
    mode: "absolute",
    size: 255,
    center: 128,
    rate: { kind: "beats", beats: 1 },
    spread: 0,
    phase: 0,
    width: 50,
    direction: "forward",
    groups: 0,
    blocks: 0,
    wings: 0,
  };
}

/** A one-line summary for lists. */
export function describeEffect(def: EffectDef): string {
  const what =
    def.target.kind === "attribute"
      ? `${capitalize(def.target.waveform)} on ${def.target.attribute}`
      : SHAPE_LABELS[def.target.shape];
  const rate = def.rate.kind === "beats" ? `${def.rate.beats} beat${def.rate.beats === 1 ? "" : "s"}` : `${def.rate.hz} Hz`;
  return `${what} · ${rate}${def.spread ? ` · fan ${def.spread}°` : ""}`;
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center gap-2 text-xs text-muted-foreground">
      <span className="w-16 shrink-0">{label}</span>
      <div className="flex min-w-0 flex-1 items-center gap-2">{children}</div>
    </div>
  );
}

function SmallNumber({ value, onChange, min = 0, max = 64, title }: { value: number; onChange: (v: number) => void; min?: number; max?: number; title: string }) {
  return (
    <Input
      type="number"
      className="h-7 w-14 px-1 text-xs"
      min={min}
      max={max}
      value={value}
      title={title}
      aria-label={title}
      onChange={(e) => {
        const n = Number(e.target.value);
        if (Number.isFinite(n)) onChange(Math.min(max, Math.max(min, Math.round(n))));
      }}
    />
  );
}

function EffectEditor({ def, onChange, attributes }: { def: EffectDef; onChange: (def: EffectDef) => void; attributes: string[] }) {
  const set = (patch: Partial<EffectDef>) => onChange({ ...def, ...patch });
  const isPosition = def.target.kind === "position";
  const usesWidth = def.target.kind === "attribute" && (def.target.waveform === "square" || def.target.waveform === "pulse");

  return (
    <div className="flex flex-col gap-2.5">
      <div className="flex gap-1">
        <Button
          size="sm"
          variant={!isPosition ? "default" : "secondary"}
          onClick={() => !(def.target.kind === "attribute") && set({ target: { kind: "attribute", attribute: attributes[0] ?? "dimmer", waveform: "sine" } })}
        >
          Waveform
        </Button>
        <Button
          size="sm"
          variant={isPosition ? "default" : "secondary"}
          onClick={() => !isPosition && set({ target: { kind: "position", shape: "circle" }, mode: "relative", size: 64 })}
        >
          Position shape
        </Button>
      </div>

      {def.target.kind === "attribute" ? (
        <>
          <Row label="Attribute">
            <Select
              className="h-8 text-xs"
              value={def.target.attribute}
              onChange={(e) => def.target.kind === "attribute" && set({ target: { ...def.target, attribute: e.target.value } })}
            >
              {[...new Set([def.target.attribute, ...attributes])].map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
            </Select>
          </Row>
          <Row label="Waveform">
            <div className="flex flex-wrap gap-1">
              {WAVEFORMS.map((w) => (
                <Button
                  key={w}
                  size="sm"
                  variant={def.target.kind === "attribute" && def.target.waveform === w ? "default" : "secondary"}
                  className="h-7 px-2 text-xs"
                  onClick={() => def.target.kind === "attribute" && set({ target: { ...def.target, waveform: w } })}
                >
                  {capitalize(w)}
                </Button>
              ))}
            </div>
          </Row>
        </>
      ) : (
        <Row label="Shape">
          <div className="flex flex-wrap gap-1">
            {SHAPES.map((shape) => (
              <Button
                key={shape}
                size="sm"
                variant={def.target.kind === "position" && def.target.shape === shape ? "default" : "secondary"}
                className="h-7 px-2 text-xs"
                onClick={() => set({ target: { kind: "position", shape } })}
              >
                {SHAPE_LABELS[shape]}
              </Button>
            ))}
          </div>
        </Row>
      )}

      <Row label="Mode">
        {(["absolute", "relative"] as EffectMode[]).map((mode) => (
          <Button
            key={mode}
            size="sm"
            variant={def.mode === mode ? "default" : "secondary"}
            className="h-7 px-2 text-xs"
            onClick={() => set({ mode })}
            title={mode === "relative" ? "Swing around the attribute's current value" : "Swing around a fixed centre"}
          >
            {capitalize(mode)}
          </Button>
        ))}
      </Row>
      <Row label="Size">
        <ValueSlider className="flex-1" max={255} value={Math.round(def.size)} onChange={(size) => set({ size })} title="Size" />
      </Row>
      {def.mode === "absolute" && (
        <Row label="Centre">
          <ValueSlider className="flex-1" max={255} value={Math.round(def.center)} onChange={(center) => set({ center })} title="Centre" />
        </Row>
      )}

      <Row label="Rate">
        <Select
          className="h-8 w-24 text-xs"
          value={def.rate.kind}
          onChange={(e) =>
            set({ rate: e.target.value === "beats" ? { kind: "beats", beats: 1 } : { kind: "hz", hz: 1 } })
          }
        >
          <option value="beats">BPM sync</option>
          <option value="hz">Hz</option>
        </Select>
        {def.rate.kind === "beats" ? (
          <Select
            className="h-8 text-xs"
            value={def.rate.beats}
            onChange={(e) => set({ rate: { kind: "beats", beats: Number(e.target.value) } })}
          >
            {[...new Set([def.rate.beats, ...BEAT_OPTIONS])].sort((a, b) => a - b).map((b) => (
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
            step={0.1}
            value={def.rate.hz}
            aria-label="Hz"
            onChange={(e) => {
              const hz = Number(e.target.value);
              if (Number.isFinite(hz) && hz >= 0) set({ rate: { kind: "hz", hz } });
            }}
          />
        )}
      </Row>

      <Row label="Fan">
        <ValueSlider className="flex-1" max={720} value={Math.round(def.spread)} onChange={(spread) => set({ spread })} title="Phase spread across the selection (degrees)" />
      </Row>
      <Row label="Phase">
        <ValueSlider className="flex-1" max={360} value={Math.round(def.phase)} onChange={(phase) => set({ phase })} title="Phase offset (degrees)" />
      </Row>
      {usesWidth && (
        <Row label="Width">
          <ValueSlider className="flex-1" min={1} max={100} value={Math.round(def.width)} onChange={(width) => set({ width })} title="Duty cycle (%)" />
        </Row>
      )}
      <Row label="Direction">
        {DIRECTIONS.map((direction) => (
          <Button
            key={direction}
            size="sm"
            variant={def.direction === direction ? "default" : "secondary"}
            className="h-7 px-2 text-xs"
            onClick={() => set({ direction })}
          >
            {capitalize(direction)}
          </Button>
        ))}
      </Row>
      <Row label="Grouping">
        <span>Groups</span>
        <SmallNumber value={def.groups} onChange={(groups) => set({ groups })} title="Groups: fixtures cycle through this many phase positions" />
        <span>Blocks</span>
        <SmallNumber value={def.blocks} onChange={(blocks) => set({ blocks })} title="Blocks: this many adjacent fixtures share a phase" />
        <span>Wings</span>
        <SmallNumber value={def.wings} onChange={(wings) => set({ wings })} title="Wings: mirror the fan across this many wings" />
      </Row>
    </div>
  );
}

/** Ordered fixture picker: the order you pick fixtures in is the order an
 * effect's fan runs in. */
function OrderedPicker({ picked, onChange }: { picked: number[]; onChange: (picked: number[]) => void }) {
  const fixtures = usePatchStore((s) => s.fixtures);
  const [groups, setGroups] = useState<Group[]>([]);
  useEffect(() => {
    void engineApi.listGroups().then(setGroups).catch(() => undefined);
  }, []);

  return (
    <div className="flex w-56 shrink-0 flex-col gap-2 overflow-y-auto rounded-md border border-border p-3">
      <div className="flex items-center justify-between text-xs font-semibold uppercase text-muted-foreground">
        <span>Fixtures</span>
        <span className="flex gap-2 normal-case">
          <button className="underline-offset-2 hover:underline" onClick={() => onChange(fixtures.map((f) => f.fixtureNumber))}>
            All
          </button>
          <button className="underline-offset-2 hover:underline" onClick={() => onChange([...picked].reverse())}>
            Reverse
          </button>
          <button className="underline-offset-2 hover:underline" onClick={() => onChange([])}>
            None
          </button>
        </span>
      </div>
      {fixtures.map((f) => {
        const position = picked.indexOf(f.fixtureNumber);
        return (
          <button
            key={f.id}
            className={cn(
              "flex items-center gap-2 rounded px-1 py-0.5 text-left text-xs",
              position >= 0 ? "bg-primary/15 text-foreground" : "text-muted-foreground hover:text-foreground",
            )}
            onClick={() =>
              onChange(position >= 0 ? picked.filter((n) => n !== f.fixtureNumber) : [...picked, f.fixtureNumber])
            }
          >
            <span className="w-5 text-right tabular-nums text-primary">{position >= 0 ? position + 1 : ""}</span>
            #{f.fixtureNumber} {f.name}
          </button>
        );
      })}
      {fixtures.length === 0 && <p className="text-xs text-muted-foreground">Patch some fixtures first.</p>}
      {groups.length > 0 && (
        <>
          <div className="mt-2 text-xs font-semibold uppercase text-muted-foreground">Groups</div>
          {groups.map((g) => (
            <button
              key={g.id}
              className="truncate text-left text-xs text-muted-foreground hover:text-primary"
              onClick={() => onChange(g.fixtureNumbers)}
              title="Select this group, in its recorded order"
            >
              {g.name} ({g.fixtureNumbers.length})
            </button>
          ))}
        </>
      )}
    </div>
  );
}

const LIST_POLL_MS = 1000;

/** Effects (BUILD_PLAN Phase 7): build an effect, run it on the selection
 * in the programmer, edit running effects live, and keep Effect presets.
 * A running programmer effect is captured by RECORD CUE like any other
 * programmer content. */
export function EffectsPanel() {
  const fixtures = usePatchStore((s) => s.fixtures);
  const allFixtures = useAllFixtures();
  const [picked, setPicked] = useState<number[]>([]);
  const [def, setDef] = useState<EffectDef>(defaultDef());
  const [running, setRunning] = useState<EffectInstance[]>([]);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [presets, setPresets] = useState<Preset[]>([]);
  const [presetName, setPresetName] = useState("");

  const refreshRunning = () => void engineApi.listEffects().then(setRunning).catch(() => undefined);
  const refreshPresets = () => void engineApi.listPresets("effect").then(setPresets).catch(() => undefined);
  useEffect(() => {
    refreshRunning();
    refreshPresets();
    // CLEAR/RELEASE on the command line can stop effects behind our back.
    const id = setInterval(refreshRunning, LIST_POLL_MS);
    return () => clearInterval(id);
  }, []);

  // Effect-able attributes on the picked fixtures (or the whole patch).
  const attributes = useMemo(() => {
    const source = picked.length > 0 ? fixtures.filter((f) => picked.includes(f.fixtureNumber)) : fixtures;
    const set = new Set<string>();
    for (const instance of source) {
      const mode = allFixtures.find((f) => f.id === instance.fixtureId)?.modes[instance.modeIndex];
      for (const ch of mode?.channels ?? []) {
        if (!NON_EFFECT_ATTRIBUTES.has(ch.type) && ch.type !== "pan" && ch.type !== "tilt") set.add(ch.type);
      }
    }
    const list = [...set].sort();
    return list.includes("dimmer") ? ["dimmer", ...list.filter((a) => a !== "dimmer")] : list;
  }, [picked, fixtures, allFixtures]);

  const editing = running.find((e) => e.id === editingId) ?? null;
  // A running effect that stopped (e.g. CLEAR) can't be edited any more.
  useEffect(() => {
    if (editingId !== null && !running.some((e) => e.id === editingId)) setEditingId(null);
  }, [running, editingId]);

  const changeDef = (next: EffectDef) => {
    setDef(next);
    if (editingId !== null) void engineApi.updateEffect(editingId, next).then(refreshRunning);
  };

  const run = () => {
    if (picked.length === 0) return;
    void engineApi.startEffect(def, picked).then((id) => {
      setEditingId(id);
      refreshRunning();
    });
  };

  const recordPreset = () => {
    if (!presetName.trim()) return;
    void engineApi.recordEffectPreset(presetName.trim(), def).then(() => {
      setPresetName("");
      refreshPresets();
    });
  };

  return (
    <div className="flex h-full gap-4 overflow-hidden p-4">
      <OrderedPicker picked={picked} onChange={setPicked} />

      <div className="flex w-[26rem] shrink-0 flex-col gap-3 overflow-y-auto rounded-md border border-border p-3">
        <div className="flex items-center justify-between">
          <h3 className="text-xs font-semibold uppercase text-muted-foreground">
            {editing ? `Editing running effect (fixtures ${editing.fixtures.join(", ")})` : "New effect"}
          </h3>
          {editing && (
            <button className="text-xs text-muted-foreground underline-offset-2 hover:underline" onClick={() => setEditingId(null)}>
              Done
            </button>
          )}
        </div>
        <EffectEditor def={def} onChange={changeDef} attributes={attributes} />
        <div className="flex flex-wrap items-center gap-2 border-t border-border pt-3">
          <Button size="sm" disabled={picked.length === 0} onClick={run} title="Run this effect on the selected fixtures, in pick order">
            <Play />
            Run on {picked.length} fixture{picked.length === 1 ? "" : "s"}
          </Button>
          <Button size="sm" variant="ghost" onClick={() => changeDef(defaultDef(attributes[0]))}>
            Reset
          </Button>
        </div>
        <div className="flex items-center gap-1">
          <Input className="h-8 text-xs" placeholder="Effect preset name" value={presetName} onChange={(e) => setPresetName(e.target.value)} />
          <Button size="sm" className="shrink-0" disabled={!presetName.trim()} onClick={recordPreset}>
            Record preset
          </Button>
        </div>
      </div>

      <div className="flex min-w-0 flex-1 flex-col gap-4 overflow-y-auto">
        <div className="rounded-md border border-border p-3">
          <h3 className="mb-2 text-xs font-semibold uppercase text-muted-foreground">Running in the programmer</h3>
          <div className="flex flex-col gap-1.5">
            {running.map((e) => (
              <div
                key={e.id}
                className={cn("flex items-center gap-2 rounded px-1 text-xs", e.id === editingId && "bg-primary/15")}
              >
                <span className="min-w-0 flex-1 truncate" title={describeEffect(e.def)}>
                  {describeEffect(e.def)}
                  <span className="text-muted-foreground"> · #{e.fixtures.join(", #")}</span>
                  {e.presetId !== null && <span className="text-primary"> · preset</span>}
                </span>
                <Button
                  size="iconSm"
                  variant="ghost"
                  title="Edit live"
                  onClick={() => {
                    setEditingId(e.id);
                    setDef(e.def);
                  }}
                >
                  <Pencil />
                </Button>
                <Button size="iconSm" variant="ghost" title="Stop" onClick={() => void engineApi.stopEffect(e.id).then(refreshRunning)}>
                  <Square />
                </Button>
              </div>
            ))}
            {running.length === 0 && (
              <p className="text-xs text-muted-foreground">
                Nothing running. RECORD CUE captures whatever runs here for the selected fixtures.
              </p>
            )}
          </div>
        </div>

        <div className="rounded-md border border-border p-3">
          <h3 className="mb-2 text-xs font-semibold uppercase text-muted-foreground">Effect presets</h3>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(140px,1fr))] gap-2">
            {presets.map((p) => (
              <div key={p.id} className="flex flex-col gap-1 rounded border border-border p-2 text-xs">
                <button
                  className="truncate text-left font-medium hover:text-primary disabled:hover:text-foreground"
                  disabled={picked.length === 0}
                  title={picked.length === 0 ? "Pick fixtures to apply" : `Run ${p.name} on the selection (linked to the preset)`}
                  onClick={() => void engineApi.applyPreset(p.id, picked).then(refreshRunning)}
                >
                  {p.name}
                </button>
                <span className="truncate text-muted-foreground">{p.effect ? describeEffect(p.effect) : "—"}</span>
                <div className="flex gap-2 text-muted-foreground">
                  <button className="hover:text-foreground" title="Load into the editor" onClick={() => p.effect && setDef(p.effect)}>
                    Load
                  </button>
                  <button
                    className="hover:text-foreground"
                    title="Update this preset from the editor (every cue using it follows)"
                    onClick={() => void engineApi.updateEffectPreset(p.id, def).then(refreshPresets)}
                  >
                    ↻
                  </button>
                  <button
                    className="ml-auto hover:text-destructive"
                    title="Delete"
                    onClick={() => void engineApi.deletePreset(p.id).then(refreshPresets)}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
            ))}
            {presets.length === 0 && <p className="col-span-full text-xs text-muted-foreground">No effect presets yet.</p>}
          </div>
        </div>
      </div>
    </div>
  );
}
