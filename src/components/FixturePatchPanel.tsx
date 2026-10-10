import { useEffect, useRef, useState } from "react";
import { Trash2, X } from "lucide-react";
import * as engineApi from "../lib/engine";
import { useDmxStore } from "../store/useDmxStore";
import { usePatchStore, type PatchedFixture } from "../store/usePatchStore";
import { useAllFixtures, useFixtureLibraryStore } from "../store/useFixtureLibraryStore";
import { EMPTY_UNIVERSES, useShowStore } from "../store/useShowStore";
import { useUniverseChannelReader, useUniverseChannelsStore } from "../store/useUniverseChannelsStore";
import type { ChannelType, FixtureDefinition, FixtureMode } from "../lib/fixtures/types";
import { validateFixtureDefinition } from "../lib/fixtures/validate";
import { importOfl } from "../lib/fixtures/importers/ofl";
import { importQlcPlus } from "../lib/fixtures/importers/qlcplus";
import { FixtureEditor } from "./FixtureEditor";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Select } from "./ui/input";
import { cn } from "../lib/utils";
import { ValueSlider } from "./ui/value-slider";

const COLOR_PRESETS: Record<string, ChannelType[]> = {
  White: ["dimmer", "white"],
  Red: ["dimmer", "red"],
  Green: ["dimmer", "green"],
  Blue: ["dimmer", "blue"],
  UV: ["dimmer", "uv"],
  Off: [],
};

interface ResolvedFixture {
  instance: PatchedFixture;
  def: FixtureDefinition;
  mode: FixtureMode;
}

function findFixture(all: FixtureDefinition[], id: string) {
  return all.find((f) => f.id === id);
}

function resolve(instance: PatchedFixture, all: FixtureDefinition[]): ResolvedFixture | null {
  const def = findFixture(all, instance.fixtureId);
  const mode = def?.modes[instance.modeIndex];
  return def && mode ? { instance, def, mode } : null;
}

function addressSpan(fixtures: PatchedFixture[], allFixtures: FixtureDefinition[], universe: number) {
  return fixtures
    .filter((f) => f.universe === universe)
    .map((f) => {
      const mode = findFixture(allFixtures, f.fixtureId)?.modes[f.modeIndex];
      const count = mode?.channelCount ?? 1;
      return { fixture: f, start: f.address, end: f.address + count - 1 };
    });
}

/** Fixtures in `universe` whose address range overlaps [address, address+channelCount-1]. */
function findCollisions(
  fixtures: PatchedFixture[],
  allFixtures: FixtureDefinition[],
  universe: number,
  address: number,
  channelCount: number,
  excludeId?: string,
): PatchedFixture[] {
  const end = address + channelCount - 1;
  return addressSpan(fixtures, allFixtures, universe)
    .filter((s) => s.fixture.id !== excludeId && address <= s.end && s.start <= end)
    .map((s) => s.fixture);
}

/** Suggests the first free address in `universe` that fits `channelCount`
 * channels without overlapping anything already patched there. */
function nextFreeAddress(fixtures: PatchedFixture[], allFixtures: FixtureDefinition[], universe: number, channelCount: number): number {
  const spans = addressSpan(fixtures, allFixtures, universe).sort((a, b) => a.start - b.start);
  let candidate = 1;
  for (const span of spans) {
    if (candidate + channelCount - 1 < span.start) break;
    if (candidate <= span.end) candidate = span.end + 1;
  }
  return Math.min(candidate, 512);
}

/** Reads a patched fixture's live channel values from its own universe's
 * engine frame -- the single source of truth -- instead of a separate copy of
 * state, so the grid tile's color and the detail sliders can never drift from
 * what's actually being sent to the fixture. */
function useFixtureChannelValues(instance: PatchedFixture, mode: FixtureMode | undefined) {
  const channels = useUniverseChannelsStore((s) => s.channels[instance.universe]);
  const values: Record<number, number> = {};
  if (!mode) return values;
  for (const ch of mode.channels) {
    // address and offset are both 1-based, so this is the 0-based frame index.
    const idx = instance.address + ch.offset - 2;
    values[ch.offset] = idx >= 0 && channels && idx < channels.length ? channels[idx] : 0;
  }
  return values;
}

/** Approximates what the fixture is actually outputting right now, like a lighting
 * console's patch/fixture grid does, so the grid reads as a live rig overview. */
function swatchColor(mode: FixtureMode, values: Record<number, number>): string {
  const offsetFor = (t: ChannelType) => mode.channels.find((c) => c.type === t)?.offset;
  const valueOf = (t: ChannelType) => {
    const offset = offsetFor(t);
    return offset === undefined ? 0 : (values[offset] ?? 0);
  };

  const hasDimmer = offsetFor("dimmer") !== undefined;
  const dimmer = hasDimmer ? valueOf("dimmer") / 255 : 1;

  let r = valueOf("red");
  let g = valueOf("green");
  let b = valueOf("blue");
  // Fold the extra emitters into RGB (approximate hues; UV is shown as the
  // violet it visibly glows) so e.g. an amber-only mix reads as amber instead
  // of tripping the dimmer-only white fallback below.
  const emitters: [ChannelType, [number, number, number]][] = [
    ["white", [255, 255, 255]],
    ["amber", [255, 150, 0]],
    ["uv", [110, 0, 255]],
  ];
  for (const [type, [er, eg, eb]] of emitters) {
    const level = valueOf(type) / 255;
    r = Math.max(r, er * level);
    g = Math.max(g, eg * level);
    b = Math.max(b, eb * level);
  }

  if (r === 0 && g === 0 && b === 0 && hasDimmer) {
    // No color channels at all (a dimmer-only fixture) -- show a plain warm glow
    // scaled by the dimmer instead of always rendering black.
    r = g = b = 255;
  }

  r = Math.round(r * dimmer);
  g = Math.round(g * dimmer);
  b = Math.round(b * dimmer);
  return `rgb(${r}, ${g}, ${b})`;
}

export function FixturePatchPanel() {
  const connected = useDmxStore((s) => s.status.connected);

  const { fixtures, addFixture, removeFixture, repatch, nextFixtureNumber } = usePatchStore();
  const addCustomFixture = useFixtureLibraryStore((s) => s.addCustomFixture);
  const customFixtures = useFixtureLibraryStore((s) => s.customFixtures);
  const allFixtures = useAllFixtures();
  const universes = useShowStore((s) => s.meta?.universes ?? EMPTY_UNIVERSES);
  const [editorOpen, setEditorOpen] = useState(false);

  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const selected = fixtures.filter((f) => selectedIds.includes(f.id));
  const resolvedSelected = selected
    .map((instance) => resolve(instance, allFixtures))
    .filter((r): r is ResolvedFixture => r !== null);

  const fileInputRef = useRef<HTMLInputElement>(null);
  const oflInputRef = useRef<HTMLInputElement>(null);
  const qlcInputRef = useRef<HTMLInputElement>(null);
  const [importError, setImportError] = useState<string | null>(null);

  const [newFixtureId, setNewFixtureId] = useState(allFixtures[0]?.id ?? "");
  const [newModeIndex, setNewModeIndex] = useState(0);
  const [newUniverse, setNewUniverse] = useState(universes[0]?.id ?? 1);
  const [newAddress, setNewAddress] = useState(1);
  const [addressTouched, setAddressTouched] = useState(false);
  const [newFixtureNumber, setNewFixtureNumber] = useState(nextFixtureNumber());
  const [newName, setNewName] = useState("");

  const newFixtureDef = findFixture(allFixtures, newFixtureId);
  const newMode = newFixtureDef?.modes[newModeIndex];

  // Keep the suggested address/fixture number fresh as the form's other
  // fields change, unless the user has already typed their own address.
  useEffect(() => {
    if (!newMode || addressTouched) return;
    setNewAddress(nextFreeAddress(fixtures, allFixtures, newUniverse, newMode.channelCount));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [newFixtureId, newModeIndex, newUniverse, fixtures.length]);

  const collisions =
    newMode && newFixtureDef
      ? findCollisions(fixtures, allFixtures, newUniverse, newAddress, newMode.channelCount)
      : [];

  const handleImport = async (file: File) => {
    setImportError(null);
    try {
      const json = JSON.parse(await file.text());
      const errors = validateFixtureDefinition(json);
      if (errors.length > 0) {
        setImportError(errors.join("; "));
        return;
      }
      addCustomFixture(json as FixtureDefinition);
    } catch (err) {
      setImportError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleFormatImport = async (file: File, importer: (input: string) => FixtureDefinition) => {
    setImportError(null);
    try {
      const def = importer(await file.text());
      const errors = validateFixtureDefinition(def);
      if (errors.length > 0) {
        setImportError(errors.join("; "));
        return;
      }
      addCustomFixture(def);
      setNewFixtureId(def.id);
      setNewModeIndex(0);
    } catch (err) {
      setImportError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleAdd = () => {
    const def = findFixture(allFixtures, newFixtureId);
    const mode = def?.modes[newModeIndex];
    if (!def || !mode) return;

    const fixture = addFixture({
      fixtureId: def.id,
      modeIndex: newModeIndex,
      address: newAddress,
      universe: newUniverse,
      fixtureNumber: newFixtureNumber,
      name: newName.trim() || `${def.model} @${newAddress}`,
      invertPan: false,
      invertTilt: false,
      swapPanTilt: false,
    });

    // Pin any function/mode-select channel to its safe "obey DMX" value right
    // away, so the fixture doesn't start out running a built-in auto-program.
    const defaults: Record<number, number> = {};
    for (const ch of mode.channels) {
      if (ch.defaultValue !== undefined) defaults[newAddress + ch.offset - 1] = ch.defaultValue;
    }
    if (Object.keys(defaults).length > 0) {
      void engineApi.setChannels(newUniverse, defaults);
    }

    setNewName("");
    setAddressTouched(false);
    setNewFixtureNumber(fixture.fixtureNumber + 1);
    setSelectedIds([fixture.id]);
  };

  const handleTileClick = (id: string, e: React.MouseEvent) => {
    setSelectedIds((prev) => {
      if (e.shiftKey || e.ctrlKey || e.metaKey) {
        return prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id];
      }
      return [id];
    });
  };

  return (
    <div className="flex h-full flex-col gap-4 overflow-hidden p-4">
      {!connected && (
        <p className="shrink-0 text-xs text-destructive">
          Connect a DMX interface in the top bar first.
        </p>
      )}

      <div className="flex shrink-0 flex-col gap-3 rounded-md border border-border p-3">
        <div className="grid grid-cols-2 gap-3 text-xs sm:grid-cols-6">
          <label className="flex flex-col gap-1">
            Fixture
            <Select
              value={newFixtureId}
              onChange={(e) => {
                setNewFixtureId(e.target.value);
                setNewModeIndex(0);
                setAddressTouched(false);
              }}
            >
              {allFixtures.map((f) => (
                <option key={f.id} value={f.id}>
                  {f.manufacturer} {f.model}
                </option>
              ))}
            </Select>
          </label>
          <label className="flex flex-col gap-1">
            Mode
            <Select
              value={newModeIndex}
              onChange={(e) => {
                setNewModeIndex(Number(e.target.value));
                setAddressTouched(false);
              }}
            >
              {newFixtureDef?.modes.map((m, i) => (
                <option key={m.name} value={i}>
                  {m.name} ({m.channelCount}ch)
                </option>
              ))}
            </Select>
          </label>
          <label className="flex flex-col gap-1">
            Universe
            <Select value={newUniverse} onChange={(e) => setNewUniverse(Number(e.target.value))}>
              {universes.length === 0 && <option value={1}>Universe 1</option>}
              {universes.map((u) => (
                <option key={u.id} value={u.id}>
                  {u.name}
                </option>
              ))}
            </Select>
          </label>
          <label className="flex flex-col gap-1">
            Address
            <Input
              type="number"
              min={1}
              max={512}
              value={newAddress}
              onChange={(e) => {
                setAddressTouched(true);
                setNewAddress(Number(e.target.value) || 1);
              }}
            />
          </label>
          <label className="flex flex-col gap-1">
            Fixture #
            <Input
              type="number"
              min={1}
              value={newFixtureNumber}
              onChange={(e) => setNewFixtureNumber(Number(e.target.value) || 1)}
            />
          </label>
          <label className="flex flex-col gap-1">
            Name
            <Input
              placeholder="optional"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
            />
          </label>
        </div>
        {collisions.length > 0 && (
          <p className="text-xs text-destructive">
            Address conflict with {collisions.map((c) => c.name).join(", ")} — pick a different address.
          </p>
        )}
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" onClick={handleAdd} disabled={!newFixtureDef}>
            Add to patch
          </Button>
          <Button size="sm" variant="secondary" onClick={() => fileInputRef.current?.click()}>
            Import fixture JSON
          </Button>
          <Button size="sm" variant="secondary" onClick={() => oflInputRef.current?.click()}>
            Import OFL
          </Button>
          <Button size="sm" variant="secondary" onClick={() => qlcInputRef.current?.click()}>
            Import QLC+ (.qxf)
          </Button>
          <Button size="sm" variant="secondary" onClick={() => setEditorOpen(true)}>
            {newFixtureDef && customFixtures.some((f) => f.id === newFixtureDef.id) ? "Edit fixture" : "New fixture"}
          </Button>
          <input
            ref={fileInputRef}
            type="file"
            accept="application/json"
            className="hidden"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) void handleImport(file);
              e.target.value = "";
            }}
          />
          <input
            ref={oflInputRef}
            type="file"
            accept="application/json"
            className="hidden"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) void handleFormatImport(file, importOfl);
              e.target.value = "";
            }}
          />
          <input
            ref={qlcInputRef}
            type="file"
            accept=".qxf,application/xml,text/xml"
            className="hidden"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) void handleFormatImport(file, importQlcPlus);
              e.target.value = "";
            }}
          />
        </div>
        {importError && <p className="text-xs text-destructive">{importError}</p>}
      </div>

      {fixtures.length > 0 && (
        <div className="flex shrink-0 items-center gap-2 text-xs text-muted-foreground">
          <span>
            {selectedIds.length > 0 ? `${selectedIds.length} selected` : `${fixtures.length} patched`}
          </span>
          <span className="text-muted-foreground/50">·</span>
          <button
            className="underline-offset-2 hover:text-foreground hover:underline"
            onClick={() => setSelectedIds(fixtures.map((f) => f.id))}
          >
            Select all
          </button>
          {selectedIds.length > 0 && (
            <button
              className="underline-offset-2 hover:text-foreground hover:underline"
              onClick={() => setSelectedIds([])}
            >
              Clear
            </button>
          )}
          <span className="text-muted-foreground/50">(shift/ctrl-click to multi-select)</span>
        </div>
      )}

      <div className="flex flex-1 gap-4 overflow-hidden">
        {fixtures.length === 0 ? (
          <div className="flex flex-1 items-center justify-center text-sm text-muted-foreground">
            No fixtures patched yet -- add one above.
          </div>
        ) : (
          <div className="grid flex-1 content-start gap-3 overflow-y-auto grid-cols-[repeat(auto-fill,minmax(110px,1fr))]">
            {fixtures.map((instance) => (
              <FixtureTile
                key={instance.id}
                instance={instance}
                def={findFixture(allFixtures, instance.fixtureId)}
                selected={selectedIds.includes(instance.id)}
                onClick={(e) => handleTileClick(instance.id, e)}
              />
            ))}
          </div>
        )}

        {resolvedSelected.length > 0 && (
          <FixtureDetailPanel
            resolved={resolvedSelected}
            allFixtures={allFixtures}
            fixtures={fixtures}
            universes={universes}
            onClose={() => setSelectedIds([])}
            onRepatch={repatch}
            onRemove={
              resolvedSelected.length === 1
                ? () => {
                    removeFixture(resolvedSelected[0].instance.id);
                    setSelectedIds([]);
                  }
                : undefined
            }
          />
        )}
      </div>

      {editorOpen && (
        <FixtureEditor
          initial={newFixtureDef && customFixtures.some((f) => f.id === newFixtureDef.id) ? newFixtureDef : undefined}
          onClose={() => setEditorOpen(false)}
        />
      )}
    </div>
  );
}

function FixtureTile({
  instance,
  def,
  selected,
  onClick,
}: {
  instance: PatchedFixture;
  def: FixtureDefinition | undefined;
  selected: boolean;
  onClick: (e: React.MouseEvent) => void;
}) {
  const mode = def?.modes[instance.modeIndex];
  const values = useFixtureChannelValues(instance, mode);

  if (!def || !mode) {
    return (
      <button
        onClick={onClick}
        className="flex aspect-square flex-col items-center justify-center rounded-md border border-destructive/50 p-2 text-center text-[10px] text-destructive"
      >
        unknown
        <br />
        addr {instance.address}
      </button>
    );
  }

  return (
    <button
      onClick={onClick}
      style={{ backgroundColor: swatchColor(mode, values) }}
      className={cn(
        "flex aspect-square flex-col justify-between rounded-md border p-2 text-left transition-all",
        selected ? "border-primary ring-2 ring-primary" : "border-border hover:border-primary/50",
      )}
    >
      <span className="w-fit rounded bg-black/50 px-1 text-[10px] font-semibold text-white">
        #{instance.fixtureNumber} · {instance.address}
      </span>
      <span className="truncate rounded bg-black/50 px-1 text-[10px] text-white">
        {instance.name}
      </span>
    </button>
  );
}

/** Union of channel info across every selected fixture, keyed by channel type and
 * built in first-seen order -- lets one slider drive "Red" (say) on every selected
 * fixture at once even if they're different models with different offsets. */
function unionChannels(resolved: ResolvedFixture[]) {
  const byType = new Map<ChannelType, { label?: string; notes?: string }>();
  for (const { mode } of resolved) {
    for (const ch of mode.channels) {
      if (!byType.has(ch.type)) byType.set(ch.type, { label: ch.label, notes: ch.notes });
    }
  }
  return [...byType.entries()].map(([type, info]) => ({ type, ...info }));
}

function FixtureDetailPanel({
  resolved,
  allFixtures,
  fixtures,
  universes,
  onClose,
  onRemove,
  onRepatch,
}: {
  resolved: ResolvedFixture[];
  allFixtures: FixtureDefinition[];
  fixtures: PatchedFixture[];
  universes: { id: number; name: string }[];
  onClose: () => void;
  onRemove?: () => void;
  onRepatch: (id: string, changes: Partial<Pick<PatchedFixture, "address" | "universe" | "invertPan" | "invertTilt" | "swapPanTilt">>) => void;
}) {
  const getUniverseChannel = useUniverseChannelReader();
  const isGroup = resolved.length > 1;
  const channels = unionChannels(resolved);
  const single = !isGroup ? resolved[0] : null;
  const hasPan = resolved.some(({ mode }) => mode.channels.some((c) => c.type === "pan"));
  const hasTilt = resolved.some(({ mode }) => mode.channels.some((c) => c.type === "tilt"));

  const valueOf = (type: ChannelType): number => {
    for (const { instance, mode } of resolved) {
      const ch = mode.channels.find((c) => c.type === type);
      if (ch) return getUniverseChannel(instance.universe, instance.address + ch.offset - 1);
    }
    return 0;
  };

  /** Builds a per-fixture channel patch from `pick`, groups it by universe
   * (a multi-select can span universes), and sends each group to the engine. */
  const applyAcrossUniverses = (pick: (instance: PatchedFixture, mode: FixtureMode) => Record<number, number>) => {
    const byUniverse = new Map<number, Record<number, number>>();
    for (const { instance, mode } of resolved) {
      const patch = pick(instance, mode);
      if (Object.keys(patch).length === 0) continue;
      const bucket = byUniverse.get(instance.universe) ?? {};
      Object.assign(bucket, patch);
      byUniverse.set(instance.universe, bucket);
    }
    for (const [universe, patch] of byUniverse) void engineApi.setChannels(universe, patch);
  };

  const setValue = (type: ChannelType, value: number) => {
    applyAcrossUniverses((instance, mode) => {
      const ch = mode.channels.find((c) => c.type === type);
      return ch ? { [instance.address + ch.offset - 1]: value } : {};
    });
  };

  const applyPreset = (types: ChannelType[]) => {
    applyAcrossUniverses((instance, mode) => {
      const patch: Record<number, number> = {};
      for (const ch of mode.channels) {
        patch[instance.address + ch.offset - 1] =
          ch.type === "function" ? (ch.defaultValue ?? 0) : types.includes(ch.type) ? 255 : 0;
      }
      return patch;
    });
  };

  return (
    <div className="flex w-72 shrink-0 flex-col gap-3 overflow-y-auto rounded-md border border-border bg-card p-3">
      <div className="flex items-start justify-between">
        <div>
          {isGroup ? (
            <div className="text-sm font-medium">{resolved.length} fixtures selected</div>
          ) : (
            <>
              <div className="text-sm font-medium">
                #{single?.instance.fixtureNumber} {single?.instance.name}
              </div>
              <div className="text-xs text-muted-foreground">
                {single?.def.manufacturer} {single?.def.model} · {single?.mode.name} · U{single?.instance.universe} ·
                addr {single?.instance.address}
              </div>
            </>
          )}
        </div>
        <div className="flex shrink-0 gap-1">
          {onRemove && (
            <Button size="iconSm" variant="ghost" onClick={onRemove} title="Unpatch">
              <Trash2 />
            </Button>
          )}
          <Button size="iconSm" variant="ghost" onClick={onClose} title="Close">
            <X />
          </Button>
        </div>
      </div>

      {single && (
        <RepatchForm
          instance={single.instance}
          channelCount={single.mode.channelCount}
          fixtures={fixtures}
          allFixtures={allFixtures}
          universes={universes}
          onRepatch={onRepatch}
        />
      )}

      {(hasPan || hasTilt) && single && (
        <div className="flex flex-wrap gap-3 text-xs text-muted-foreground">
          {hasPan && (
            <label className="flex items-center gap-1.5">
              <input
                type="checkbox"
                checked={single.instance.invertPan}
                onChange={(e) => onRepatch(single.instance.id, { invertPan: e.target.checked })}
              />
              Invert pan
            </label>
          )}
          {hasTilt && (
            <label className="flex items-center gap-1.5">
              <input
                type="checkbox"
                checked={single.instance.invertTilt}
                onChange={(e) => onRepatch(single.instance.id, { invertTilt: e.target.checked })}
              />
              Invert tilt
            </label>
          )}
          {hasPan && hasTilt && (
            <label className="flex items-center gap-1.5">
              <input
                type="checkbox"
                checked={single.instance.swapPanTilt}
                onChange={(e) => onRepatch(single.instance.id, { swapPanTilt: e.target.checked })}
              />
              Swap pan/tilt
            </label>
          )}
        </div>
      )}

      <div className="flex flex-col gap-2">
        {channels.map((ch) => (
          <div key={ch.type} className="flex items-center gap-3">
            <span className="w-20 shrink-0 truncate text-xs text-muted-foreground" title={ch.notes}>
              {ch.label ?? ch.type}
            </span>
            <ValueSlider
              className="flex-1"
              max={255}
              value={valueOf(ch.type)}
              onChange={(v) => setValue(ch.type, v)}
              title={ch.label ?? ch.type}
            />
          </div>
        ))}
      </div>

      <div className="flex flex-wrap gap-1.5">
        {Object.entries(COLOR_PRESETS).map(([name, types]) => (
          <Button key={name} size="sm" variant="secondary" onClick={() => applyPreset(types)}>
            {name}
          </Button>
        ))}
      </div>
    </div>
  );
}

function RepatchForm({
  instance,
  channelCount,
  fixtures,
  allFixtures,
  universes,
  onRepatch,
}: {
  instance: PatchedFixture;
  channelCount: number;
  fixtures: PatchedFixture[];
  allFixtures: FixtureDefinition[];
  universes: { id: number; name: string }[];
  onRepatch: (id: string, changes: Partial<Pick<PatchedFixture, "address" | "universe">>) => void;
}) {
  const [universe, setUniverse] = useState(instance.universe);
  const [address, setAddress] = useState(instance.address);

  useEffect(() => {
    setUniverse(instance.universe);
    setAddress(instance.address);
  }, [instance.id, instance.universe, instance.address]);

  const collisions = findCollisions(fixtures, allFixtures, universe, address, channelCount, instance.id);
  const changed = universe !== instance.universe || address !== instance.address;

  return (
    <div className="flex flex-col gap-1.5 rounded border border-border p-2">
      <div className="flex items-center gap-2 text-xs">
        <label className="flex flex-1 flex-col gap-1">
          Universe
          <Select value={universe} onChange={(e) => setUniverse(Number(e.target.value))}>
            {universes.length === 0 && <option value={instance.universe}>Universe {instance.universe}</option>}
            {universes.map((u) => (
              <option key={u.id} value={u.id}>
                {u.name}
              </option>
            ))}
          </Select>
        </label>
        <label className="flex flex-1 flex-col gap-1">
          Address
          <Input type="number" min={1} max={512} value={address} onChange={(e) => setAddress(Number(e.target.value) || 1)} />
        </label>
      </div>
      {collisions.length > 0 && (
        <p className="text-xs text-destructive">Conflicts with {collisions.map((c) => c.name).join(", ")}</p>
      )}
      <Button size="sm" variant="secondary" disabled={!changed} onClick={() => onRepatch(instance.id, { universe, address })}>
        Re-patch
      </Button>
    </div>
  );
}
