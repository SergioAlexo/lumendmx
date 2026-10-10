import { useEffect, useState } from "react";
import { Trash2 } from "lucide-react";
import * as engineApi from "../lib/engine";
import type { Group, Preset, PresetFamily } from "../lib/engine";
import { useAllFixtures } from "../store/useFixtureLibraryStore";
import { usePatchStore } from "../store/usePatchStore";
import { useUniverseChannelReader } from "../store/useUniverseChannelsStore";
import type { ChannelType } from "../lib/fixtures/types";
import { rgbToHex } from "../lib/color";
import { ColorPicker } from "./ColorPicker";
import { PanTiltPad } from "./PanTiltPad";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { ValueSlider } from "./ui/value-slider";
import { cn } from "../lib/utils";

const FAMILIES: PresetFamily[] = ["intensity", "colour", "position", "beam", "beamFx", "framing", "effect"];
const FAMILY_LABELS: Record<PresetFamily, string> = {
  intensity: "Intensity",
  colour: "Colour",
  position: "Position",
  beam: "Beam",
  beamFx: "Beam FX",
  framing: "Framing",
  effect: "Effect",
};

/** Reads/writes an attribute across every fixture in `fixtureNumbers` at
 * once (a preset can span several fixtures), resolving each one's channel
 * for that attribute from the patch + fixture library — the same pattern
 * FixturePatchPanel's detail panel uses, keyed by fixture number instead of
 * patch instance id since a Palette selection isn't tied to the patch
 * panel's own selection state. */
function useAttributeIO(fixtureNumbers: number[]) {
  const fixtures = usePatchStore((s) => s.fixtures);
  const allFixtures = useAllFixtures();
  const getUniverseChannel = useUniverseChannelReader();
  const instances = fixtures.filter((f) => fixtureNumbers.includes(f.fixtureNumber));

  const getValue = (type: ChannelType): number => {
    for (const instance of instances) {
      const mode = allFixtures.find((f) => f.id === instance.fixtureId)?.modes[instance.modeIndex];
      const ch = mode?.channels.find((c) => c.type === type);
      if (ch) return getUniverseChannel(instance.universe, instance.address + ch.offset - 1);
    }
    return 0;
  };

  const setValues = (values: Partial<Record<ChannelType, number>>) => {
    const byUniverse = new Map<number, Record<number, number>>();
    for (const instance of instances) {
      const mode = allFixtures.find((f) => f.id === instance.fixtureId)?.modes[instance.modeIndex];
      if (!mode) continue;
      const bucket = byUniverse.get(instance.universe) ?? {};
      for (const [type, value] of Object.entries(values) as [ChannelType, number][]) {
        const ch = mode.channels.find((c) => c.type === type);
        if (ch) bucket[instance.address + ch.offset - 1] = value;
      }
      byUniverse.set(instance.universe, bucket);
    }
    for (const [universe, values] of byUniverse) void engineApi.setChannels(universe, values);
  };

  return { getValue, setValues };
}

/** The engine doesn't report a group master's level back, so the slider
 * keeps its own (starts at 100, the engine's default). */
function GroupMaster({ groupId }: { groupId: number }) {
  const [level, setLevel] = useState(100);
  return (
    <ValueSlider
      className="w-36"
      max={100}
      value={level}
      onChange={(v) => {
        setLevel(v);
        void engineApi.applyGroupMaster(groupId, v);
      }}
      title="Group master"
    />
  );
}

function GroupsSection({ picked, onSelectGroup }: { picked: Set<number>; onSelectGroup: (s: Set<number>) => void }) {
  const [groups, setGroups] = useState<Group[]>([]);
  // Inline name field rather than window.prompt(): macOS's WKWebView has no
  // prompt() UI under Tauri, so it returns null and the record silently no-ops.
  const [name, setName] = useState("");
  const refresh = () => void engineApi.listGroups().then(setGroups).catch(() => undefined);
  useEffect(() => {
    refresh();
  }, []);

  const record = () => {
    if (!name.trim() || picked.size === 0) return;
    void engineApi.recordGroup(name.trim(), [...picked]).then(() => {
      setName("");
      refresh();
    });
  };

  return (
    <div className="rounded-md border border-border p-3">
      <div className="mb-2 flex items-center justify-between">
        <h3 className="text-xs font-semibold uppercase text-muted-foreground">Groups</h3>
        <div className="flex shrink-0 items-center gap-1">
          <Input className="h-8 w-40 text-xs" placeholder="Group name" value={name} onChange={(e) => setName(e.target.value)} />
          <Button size="sm" className="shrink-0" disabled={picked.size === 0 || !name.trim()} onClick={record}>
            Record group from selection
          </Button>
        </div>
      </div>
      <div className="flex flex-col gap-2">
        {groups.map((g) => (
          <div key={g.id} className="flex items-center gap-2 text-xs">
            <button
              className="flex-1 truncate text-left hover:text-primary"
              onClick={() => onSelectGroup(new Set(g.fixtureNumbers))}
              title="Select this group's fixtures"
            >
              {g.name} ({g.fixtureNumbers.length})
            </button>
            <GroupMaster groupId={g.id} />
            <Button size="iconSm" variant="ghost" onClick={() => void engineApi.deleteGroup(g.id).then(refresh)}>
              <Trash2 />
            </Button>
          </div>
        ))}
        {groups.length === 0 && <p className="text-xs text-muted-foreground">No groups yet.</p>}
      </div>
    </div>
  );
}

function PresetsSection({ picked }: { picked: number[] }) {
  const [family, setFamily] = useState<PresetFamily>("colour");
  const [presets, setPresets] = useState<Preset[]>([]);
  const [name, setName] = useState("");
  const io = useAttributeIO(picked);
  const refresh = (f: PresetFamily) => void engineApi.listPresets(f).then(setPresets).catch(() => undefined);

  useEffect(() => {
    refresh(family);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [family]);

  const record = () => {
    if (!name.trim() || picked.length === 0) return;
    const color = family === "colour" ? rgbToHex({ r: io.getValue("red"), g: io.getValue("green"), b: io.getValue("blue") }) : undefined;
    void engineApi.recordPreset(family, name.trim(), picked, color).then(() => {
      setName("");
      refresh(family);
    });
  };

  const rgb = { r: io.getValue("red"), g: io.getValue("green"), b: io.getValue("blue") };
  const pan = io.getValue("pan");
  const tilt = io.getValue("tilt");

  return (
    <div className="rounded-md border border-border p-3">
      <div className="mb-2 flex flex-wrap items-center gap-1">
        {FAMILIES.map((f) => (
          <Button key={f} size="sm" variant={f === family ? "default" : "secondary"} onClick={() => setFamily(f)}>
            {FAMILY_LABELS[f]}
          </Button>
        ))}
        <div className="flex-1" />
        <div className="flex shrink-0 items-center gap-1">
          <Input
            className={cn("h-8 w-40 text-xs", family === "effect" && "hidden")}
            placeholder={`${FAMILY_LABELS[family]} preset name`}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          {family === "effect" ? (
            <span className="text-xs text-muted-foreground">Record effect presets in the Effects tab</span>
          ) : (
            <Button size="sm" className="shrink-0" disabled={picked.length === 0 || !name.trim()} onClick={record}>
              Record from selection
            </Button>
          )}
        </div>
      </div>

      <div className="grid grid-cols-[repeat(auto-fill,minmax(90px,1fr))] gap-2">
        {presets.map((p) => (
          <div key={p.id} className="flex flex-col gap-1 rounded border border-border p-2 text-center text-xs">
            <button
              className={cn("aspect-square rounded border border-border/50", !p.color && "bg-muted")}
              style={p.color ? { backgroundColor: p.color } : undefined}
              onClick={() => picked.length > 0 && void engineApi.applyPreset(p.id, picked)}
              title={`Apply ${p.name}`}
              disabled={picked.length === 0}
            />
            <span className="truncate" title={p.name}>
              {p.name}
            </span>
            <div className="flex justify-center gap-2 text-muted-foreground">
              <button
                className={cn("hover:text-foreground", family === "effect" && "hidden")}
                title="Update from selection"
                onClick={() => void engineApi.updatePreset(p.id, picked).then(() => refresh(family))}
              >
                ↻
              </button>
              <button
                className="hover:text-destructive"
                title="Delete"
                onClick={() => void engineApi.deletePreset(p.id).then(() => refresh(family))}
              >
                ✕
              </button>
            </div>
          </div>
        ))}
        {presets.length === 0 && (
          <p className="col-span-full py-2 text-xs text-muted-foreground">
            No {FAMILY_LABELS[family].toLowerCase()} presets yet.
          </p>
        )}
      </div>

      {family === "colour" && picked.length > 0 && (
        <div className="mt-3 border-t border-border pt-3">
          <ColorPicker value={rgb} onChange={(next) => io.setValues({ red: next.r, green: next.g, blue: next.b })} />
        </div>
      )}
      {family === "position" && picked.length > 0 && (
        <div className="mt-3 border-t border-border pt-3">
          <PanTiltPad pan={pan} tilt={tilt} onChange={(p, t) => io.setValues({ pan: p, tilt: t })} />
        </div>
      )}
    </div>
  );
}

/** Groups + preset families (BUILD_PLAN Phase 5). Uses its own fixture
 * picker rather than sharing FixturePatchPanel's selection — there's no
 * cross-tab selection state yet, and building that just for this would be
 * more infrastructure than this phase needs. */
export function PalettesPanel() {
  const fixtures = usePatchStore((s) => s.fixtures);
  const [picked, setPicked] = useState<Set<number>>(new Set());

  return (
    <div className="flex h-full gap-4 overflow-hidden p-4">
      <div className="flex w-56 shrink-0 flex-col gap-2 overflow-y-auto rounded-md border border-border p-3">
        <div className="flex items-center justify-between text-xs font-semibold uppercase text-muted-foreground">
          <span>Fixtures</span>
          <button
            className="normal-case underline-offset-2 hover:underline"
            onClick={() => setPicked(new Set(fixtures.map((f) => f.fixtureNumber)))}
          >
            All
          </button>
        </div>
        {fixtures.map((f) => (
          <label key={f.id} className="flex items-center gap-2 text-xs">
            <input
              type="checkbox"
              checked={picked.has(f.fixtureNumber)}
              onChange={(e) => {
                const next = new Set(picked);
                if (e.target.checked) next.add(f.fixtureNumber);
                else next.delete(f.fixtureNumber);
                setPicked(next);
              }}
            />
            #{f.fixtureNumber} {f.name}
          </label>
        ))}
        {fixtures.length === 0 && <p className="text-xs text-muted-foreground">Patch some fixtures first.</p>}
      </div>

      <div className="flex flex-1 flex-col gap-4 overflow-y-auto">
        <GroupsSection picked={picked} onSelectGroup={setPicked} />
        <PresetsSection picked={[...picked]} />
      </div>
    </div>
  );
}
