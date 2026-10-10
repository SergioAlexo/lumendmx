import { Fragment, useEffect, useState } from "react";
import { Trash2, Unlink } from "lucide-react";
import * as engineApi from "../lib/engine";
import type { ChaseDirection, CuelistDto, CuelistKind, Preset, PresetFamily } from "../lib/engine";
import { usePatchStore } from "../store/usePatchStore";
import { useAllFixtures } from "../store/useFixtureLibraryStore";
import type { ChannelType } from "../lib/fixtures/types";
import { Button } from "./ui/button";
import { describeEffect } from "./EffectsPanel";
import { Input, Select } from "./ui/input";
import { cn } from "../lib/utils";

const KIND_LABELS: Record<CuelistKind["kind"], string> = {
  standard: "Standard",
  chase: "Chase",
  override: "Override",
  submaster: "Submaster",
  timecode: "Timecode",
};

const ATTRIBUTES: ChannelType[] = [
  "dimmer",
  "red",
  "green",
  "blue",
  "white",
  "amber",
  "uv",
  "pan",
  "tilt",
  "colorwheel",
  "gobo",
  "zoom",
  "focus",
  "iris",
  "prism",
];

const FAMILIES: PresetFamily[] = ["intensity", "colour", "position", "beam", "beamFx", "framing", "effect"];

function NewCuelistForm({ onCreated }: { onCreated: (id: number) => void }) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<CuelistKind["kind"]>("standard");
  const [bpm, setBpm] = useState(120);
  const [direction, setDirection] = useState<ChaseDirection>("forward");

  const create = () => {
    if (!name.trim()) return;
    const kindPayload: CuelistKind =
      kind === "chase" ? { kind: "chase", bpm, direction } : ({ kind } as CuelistKind);
    void engineApi.createCuelist(name.trim(), kindPayload, true).then((id) => {
      setName("");
      onCreated(id);
    });
  };

  return (
    <div className="flex flex-col gap-2 rounded-md border border-border p-3">
      <h3 className="text-xs font-semibold uppercase text-muted-foreground">New Cuelist</h3>
      <Input placeholder="Name" value={name} onChange={(e) => setName(e.target.value)} />
      <Select value={kind} onChange={(e) => setKind(e.target.value as CuelistKind["kind"])}>
        {(Object.keys(KIND_LABELS) as CuelistKind["kind"][]).map((k) => (
          <option key={k} value={k}>
            {KIND_LABELS[k]}
          </option>
        ))}
      </Select>
      {kind === "chase" && (
        <div className="flex gap-2">
          <Input
            type="number"
            min={1}
            className="w-24"
            value={bpm}
            onChange={(e) => setBpm(Number(e.target.value))}
            title="BPM"
          />
          <Select value={direction} onChange={(e) => setDirection(e.target.value as ChaseDirection)}>
            <option value="forward">Forward</option>
            <option value="bounce">Bounce</option>
            <option value="random">Random</option>
          </Select>
        </div>
      )}
      <Button size="sm" disabled={!name.trim()} onClick={create}>
        Create
      </Button>
    </div>
  );
}

function CuelistDirectory({
  cuelists,
  selectedId,
  onSelect,
  refresh,
}: {
  cuelists: CuelistDto[];
  selectedId: number | null;
  onSelect: (id: number) => void;
  refresh: () => void;
}) {
  return (
    <div className="flex flex-col gap-1 overflow-y-auto rounded-md border border-border p-3">
      <h3 className="mb-1 text-xs font-semibold uppercase text-muted-foreground">Cuelists</h3>
      {cuelists.map((list) => (
        <div
          key={list.id}
          className={cn(
            "flex items-center gap-2 rounded px-2 py-1 text-xs",
            selectedId === list.id ? "bg-accent" : "hover:bg-accent/50",
          )}
        >
          <button className="flex-1 truncate text-left" onClick={() => onSelect(list.id)}>
            {list.name}
            <span className="ml-1 text-muted-foreground">
              ({KIND_LABELS[list.kind.kind]}, {list.cues.length})
            </span>
          </button>
          <Button
            size="iconSm"
            variant="ghost"
            onClick={() => void engineApi.deleteCuelist(list.id).then(refresh)}
            title="Delete cuelist"
          >
            <Trash2 />
          </Button>
        </div>
      ))}
      {cuelists.length === 0 && <p className="text-xs text-muted-foreground">No cuelists yet.</p>}
    </div>
  );
}

function ChaseTempo({ cuelist, refresh }: { cuelist: CuelistDto; refresh: () => void }) {
  const [tapTimes, setTapTimes] = useState<number[]>([]);
  if (cuelist.kind.kind !== "chase") return null;
  const { bpm } = cuelist.kind;

  const tap = () => {
    const now = performance.now();
    const recent = [...tapTimes, now].filter((t) => now - t < 3000).slice(-6);
    setTapTimes(recent);
    if (recent.length < 2) return;
    const intervals = recent.slice(1).map((t, i) => t - recent[i]);
    const avgMs = intervals.reduce((a, b) => a + b, 0) / intervals.length;
    void engineApi.setChaseTempo(cuelist.id, Math.round(60000 / avgMs)).then(refresh);
  };

  return (
    <div className="flex items-center gap-2 text-xs">
      <span className="text-muted-foreground">BPM</span>
      <Input
        type="number"
        min={1}
        className="w-20"
        value={bpm}
        onChange={(e) => void engineApi.setChaseTempo(cuelist.id, Number(e.target.value)).then(refresh)}
      />
      <Button size="sm" variant="secondary" onClick={tap}>
        Tap Tempo
      </Button>
    </div>
  );
}

function CueSheet({
  cuelist,
  picked,
  refresh,
}: {
  cuelist: CuelistDto;
  picked: number[];
  refresh: () => void;
}) {
  const [number, setNumber] = useState(cuelist.cues.length > 0 ? cuelist.cues[cuelist.cues.length - 1].number + 1 : 1);
  const [name, setName] = useState("");
  const [fadeInMs, setFadeInMs] = useState(1000);
  const [fadeOutMs, setFadeOutMs] = useState(1000);

  const [linkCue, setLinkCue] = useState<number | null>(null);
  const [linkFixture, setLinkFixture] = useState<number | null>(null);
  const [linkAttribute, setLinkAttribute] = useState<ChannelType>("dimmer");
  const [linkFamily, setLinkFamily] = useState<PresetFamily>("colour");
  const [linkPresets, setLinkPresets] = useState<Preset[]>([]);
  const [linkStatus, setLinkStatus] = useState<{ ok: boolean; text: string } | null>(null);
  const [expandedCue, setExpandedCue] = useState<number | null>(null);
  const [allPresets, setAllPresets] = useState<Preset[]>([]);
  const patched = usePatchStore((s) => s.fixtures);
  const allFixtures = useAllFixtures();

  // Re-fetch on every cuelist refresh so a just-linked preset has a name.
  useEffect(() => {
    void engineApi.listPresets().then(setAllPresets).catch(() => undefined);
  }, [cuelist]);

  /** Fixture number owning a (universe, 1-based channel), for display. */
  const fixtureAt = (universe: number, channel: number) =>
    patched.find((f) => {
      const size = allFixtures.find((d) => d.id === f.fixtureId)?.modes[f.modeIndex]?.channels.length ?? 0;
      return f.universe === universe && channel >= f.address && channel < f.address + size;
    })?.fixtureNumber;

  useEffect(() => {
    void engineApi.listPresets(linkFamily).then(setLinkPresets).catch(() => undefined);
  }, [linkFamily]);

  const record = () => {
    if (picked.length === 0 || !name.trim()) return;
    void engineApi
      .recordCue(cuelist.id, number, name.trim(), picked, fadeInMs, fadeOutMs)
      .then(() => {
        setName("");
        setNumber(number + 1);
        refresh();
      });
  };

  return (
    <div className="flex flex-1 flex-col gap-3 overflow-hidden">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold">{cuelist.name}</h3>
        <label className="flex items-center gap-1 text-xs text-muted-foreground">
          <input
            type="checkbox"
            checked={cuelist.tracking}
            onChange={(e) => void engineApi.setCuelistTracking(cuelist.id, e.target.checked).then(refresh)}
          />
          Tracking
        </label>
      </div>

      <ChaseTempo cuelist={cuelist} refresh={refresh} />

      <div className="flex flex-wrap items-end gap-2 rounded-md border border-border p-3">
        <div>
          <label className="block text-xs text-muted-foreground">Cue #</label>
          <Input
            type="number"
            step={0.1}
            min={0}
            className="w-20"
            value={number}
            onChange={(e) => setNumber(Math.max(0, Number(e.target.value)))}
          />
        </div>
        <div>
          <label className="block text-xs text-muted-foreground">Name</label>
          <Input className="w-40" value={name} onChange={(e) => setName(e.target.value)} />
        </div>
        <div>
          <label className="block text-xs text-muted-foreground">Fade in (ms)</label>
          <Input
            type="number"
            className="w-24"
            min={0}
            value={fadeInMs}
            onChange={(e) => setFadeInMs(Math.max(0, Number(e.target.value)))}
          />
        </div>
        <div>
          <label className="block text-xs text-muted-foreground">Fade out (ms)</label>
          <Input
            type="number"
            className="w-24"
            min={0}
            value={fadeOutMs}
            onChange={(e) => setFadeOutMs(Math.max(0, Number(e.target.value)))}
          />
        </div>
        <Button size="sm" disabled={picked.length === 0 || !name.trim()} onClick={record}>
          Record from selection
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto rounded-md border border-border">
        <table className="w-full text-left text-xs">
          <thead className="sticky top-0 bg-card">
            <tr className="border-b border-border text-muted-foreground">
              <th className="px-2 py-1">#</th>
              <th className="px-2 py-1">Name</th>
              <th className="px-2 py-1">Fade In</th>
              <th className="px-2 py-1">Fade Out</th>
              <th className="px-2 py-1">Values</th>
              <th className="px-2 py-1" />
            </tr>
          </thead>
          <tbody>
            {cuelist.cues.map((cue) => {
              const links = cue.values.flatMap((v) =>
                v.value.kind === "preset" ? [{ ...v, presetId: v.value.presetId, attribute: v.value.attribute }] : [],
              );
              const expanded = expandedCue === cue.number;
              return (
                <Fragment key={cue.number}>
                  <tr className="border-b border-border/50">
                    <td className="px-2 py-1">{cue.number}</td>
                    <td className="px-2 py-1">
                      {cue.name}
                      {cue.effects.length > 0 && (
                        <span
                          className="ml-1.5 rounded bg-primary/15 px-1 text-[10px] text-primary"
                          title={cue.effects.map((e) => describeEffect(e.def)).join("\n")}
                        >
                          FX {cue.effects.length}
                        </span>
                      )}
                    </td>
                    <td className="px-2 py-1">{cue.fadeInMs}ms</td>
                    <td className="px-2 py-1">{cue.fadeOutMs}ms</td>
                    <td className="px-2 py-1">
                      {cue.values.length}
                      {links.length > 0 && (
                        <button
                          className="ml-1 text-primary hover:underline"
                          onClick={() => setExpandedCue(expanded ? null : cue.number)}
                          title={expanded ? "Hide linked presets" : "Show linked presets"}
                        >
                          ({links.length} linked {expanded ? "▾" : "▸"})
                        </button>
                      )}
                    </td>
                    <td className="px-2 py-1 text-right">
                      <Button
                        size="iconSm"
                        variant="ghost"
                        onClick={() => void engineApi.deleteCue(cuelist.id, cue.number).then(refresh)}
                      >
                        <Trash2 />
                      </Button>
                    </td>
                  </tr>
                  {expanded && (
                    <tr className="border-b border-border/50 bg-muted/30">
                      <td colSpan={6} className="px-4 py-2">
                        <ul className="flex flex-col gap-0.5">
                          {links.map((l) => {
                            const preset = allPresets.find((p) => p.id === l.presetId);
                            const fixture = fixtureAt(l.universe, l.channel);
                            return (
                              <li key={`${l.universe}.${l.channel}`} className="flex items-center gap-2">
                                {preset?.color && (
                                  <span className="h-3 w-3 shrink-0 rounded-sm border border-border" style={{ backgroundColor: preset.color }} />
                                )}
                                <span>
                                  {fixture !== undefined ? `Fixture ${fixture}` : `U${l.universe}.${l.channel}`} · {l.attribute} →{" "}
                                  <span className="font-medium">{preset ? preset.name : `missing preset #${l.presetId}`}</span>
                                  {preset && <span className="text-muted-foreground"> ({preset.family})</span>}
                                </span>
                                <Button
                                  size="iconSm"
                                  variant="ghost"
                                  title="Unlink (keep the preset's current value as a fixed value)"
                                  onClick={() =>
                                    engineApi
                                      .unlinkCuePreset(cuelist.id, cue.number, l.universe, l.channel)
                                      .then(refresh)
                                      .catch((err) => setLinkStatus({ ok: false, text: String(err) }))
                                  }
                                >
                                  <Unlink />
                                </Button>
                              </li>
                            );
                          })}
                        </ul>
                      </td>
                    </tr>
                  )}
                </Fragment>
              );
            })}
            {cuelist.cues.length === 0 && (
              <tr>
                <td colSpan={6} className="px-2 py-3 text-center text-muted-foreground">
                  No cues recorded yet.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <div className="flex flex-wrap items-end gap-2 rounded-md border border-border p-3">
        <h4 className="w-full text-xs font-semibold uppercase text-muted-foreground">
          Link a cue's channel to a preset
        </h4>
        <div>
          <label className="block text-xs text-muted-foreground">Cue #</label>
          <Select value={linkCue ?? ""} onChange={(e) => setLinkCue(Number(e.target.value))} className="w-20">
            <option value="" disabled>
              --
            </option>
            {cuelist.cues.map((c) => (
              <option key={c.number} value={c.number}>
                {c.number}
              </option>
            ))}
          </Select>
        </div>
        <div>
          <label className="block text-xs text-muted-foreground">Fixture #</label>
          <Input
            type="number"
            className="w-20"
            min={1}
            value={linkFixture ?? ""}
            onChange={(e) => setLinkFixture(e.target.value === "" ? null : Math.max(1, Math.round(Number(e.target.value))))}
          />
        </div>
        <div>
          <label className="block text-xs text-muted-foreground">Attribute</label>
          <Select value={linkAttribute} onChange={(e) => setLinkAttribute(e.target.value as ChannelType)}>
            {ATTRIBUTES.map((a) => (
              <option key={a} value={a}>
                {a}
              </option>
            ))}
          </Select>
        </div>
        <div>
          <label className="block text-xs text-muted-foreground">Preset family</label>
          <Select value={linkFamily} onChange={(e) => setLinkFamily(e.target.value as PresetFamily)}>
            {FAMILIES.map((f) => (
              <option key={f} value={f}>
                {f}
              </option>
            ))}
          </Select>
        </div>
        <div>
          <label className="block text-xs text-muted-foreground">Preset</label>
          <Select id="preset-select" className="w-40">
            {linkPresets.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </Select>
        </div>
        <Button
          size="sm"
          disabled={linkCue === null || linkFixture === null || linkPresets.length === 0}
          onClick={() => {
            const select = document.getElementById("preset-select") as HTMLSelectElement | null;
            const presetId = Number(select?.value);
            if (linkCue === null || linkFixture === null || !presetId) return;
            const presetName = linkPresets.find((p) => p.id === presetId)?.name ?? `preset ${presetId}`;
            // Surface the result: a failure (unknown fixture, no such attribute
            // channel) used to be swallowed, and a success doesn't change the
            // cue's value count, so either way the button looked dead.
            engineApi
              .setCuePresetReference(cuelist.id, linkCue, linkFixture, linkAttribute, presetId)
              .then(() => {
                setLinkStatus({ ok: true, text: `Cue ${linkCue}: fixture ${linkFixture} ${linkAttribute} → ${presetName}` });
                refresh();
              })
              .catch((err) => setLinkStatus({ ok: false, text: String(err) }));
          }}
        >
          Link
        </Button>
        {linkStatus && (
          <p className={cn("w-full text-xs", linkStatus.ok ? "text-muted-foreground" : "text-destructive")}>
            {linkStatus.text}
          </p>
        )}
      </div>
    </div>
  );
}

/** Cuelists, cues and tracking (BUILD_PLAN Phase 6). Uses its own fixture
 * picker rather than sharing another panel's selection, the same scope
 * reduction PalettesPanel documents (no cross-tab selection state). Playback
 * (GO/Back/Release/fader) lives in `PlaybackBar`, docked separately so it
 * stays reachable while this panel is used purely for editing.
 *
 * Selecting a cuelist here also becomes the command line's "current
 * cuelist" (`engine_set_current_cuelist`), so `1 THRU 8 @ 50` to set a look
 * and then `RECORD CUE 1` from the command line targets whichever cuelist
 * is open in this tab. */
export function CuePanel() {
  const fixtures = usePatchStore((s) => s.fixtures);
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [cuelists, setCuelists] = useState<CuelistDto[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const select = (id: number) => {
    setSelectedId(id);
    void engineApi.setCurrentCuelist(id);
  };

  const refresh = () => void engineApi.listCuelists().then(setCuelists).catch(() => undefined);
  useEffect(() => {
    refresh();
  }, []);

  const selected = cuelists.find((l) => l.id === selectedId) ?? null;

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

      <div className="flex w-64 shrink-0 flex-col gap-4 overflow-y-auto">
        <NewCuelistForm onCreated={(id) => { refresh(); select(id); }} />
        <CuelistDirectory cuelists={cuelists} selectedId={selectedId} onSelect={select} refresh={refresh} />
      </div>

      {selected ? (
        <CueSheet cuelist={selected} picked={[...picked]} refresh={refresh} />
      ) : (
        <p className="flex-1 text-center text-sm text-muted-foreground">Select or create a cuelist.</p>
      )}
    </div>
  );
}
