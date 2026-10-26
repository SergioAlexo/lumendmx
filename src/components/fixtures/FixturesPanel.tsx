import { useEffect, useMemo, useState } from "react";
import { PanelRightClose, PanelRightOpen, Plus, Trash2 } from "lucide-react";
import { controllerLabel } from "../../lib/dmx";
import * as showApi from "../../lib/show";
import type { FixtureDefinition, FixtureMode } from "../../lib/fixtures/types";
import type { PatchedFixture, UniverseConfig } from "../../lib/showfile/generated";
import { cn } from "../../lib/utils";
import { useAllFixtures } from "../../store/useFixtureLibraryStore";
import { EMPTY_UNIVERSES, useShowStore } from "../../store/useShowStore";
import { useUniverseChannelsStore } from "../../store/useUniverseChannelsStore";
import { useUniverseStatusesStore } from "../../store/useUniverseStatusesStore";
import { Button } from "../ui/button";
import { Input, Select } from "../ui/input";
import { AddFixturesDialog } from "./AddFixturesDialog";
import { GroupsPanel } from "./GroupsPanel";
import { channelValues, findMode, hasAttribute, strobeHz, swatchColor } from "./fixtureLook";

type View = "patch" | "values" | "dmx";

const GROUPS_HIDDEN_KEY = "lumendmx-groups-hidden";

/** Whether the groups sidebar is hidden, remembered across launches (and
 * shared with popped-out windows). */
function useGroupsHidden(): [boolean, (hidden: boolean) => void] {
  const [hidden, setHidden] = useState(() => {
    try {
      return localStorage.getItem(GROUPS_HIDDEN_KEY) === "1";
    } catch {
      return false;
    }
  });
  const set = (next: boolean) => {
    setHidden(next);
    try {
      localStorage.setItem(GROUPS_HIDDEN_KEY, next ? "1" : "0");
    } catch {
      // Not remembered; still toggles for this session.
    }
  };
  return [hidden, set];
}

const VIEW_LABELS: Record<View, string> = { patch: "Patch", values: "Values", dmx: "Raw DMX" };

function isEditable(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName));
}

/** Fixtures in `universe`, in address order. */
function fixturesIn(patch: PatchedFixture[], universe: number): PatchedFixture[] {
  return patch.filter((f) => f.universe === universe).sort((a, b) => a.address - b.address);
}

/** The Fixtures tab. Fixtures are listed per universe, i.e. per controller.
 * Patch view edits them, Values shows what each is outputting, Raw DMX the
 * universes' channel values. Groups sit on the right. Selection is shared by
 * all three views and the groups panel; its order is click order, so a group
 * made from it keeps the order fixtures were picked in. */
export function FixturesPanel() {
  const show = useShowStore((s) => s.show);
  const run = useShowStore((s) => s.run);
  const universes = show?.universes ?? EMPTY_UNIVERSES;
  const patch = useMemo(() => show?.patch ?? [], [show?.patch]);
  const library = useAllFixtures();

  const [view, setView] = useState<View>("patch");
  const [selected, setSelected] = useState<number[]>([]);
  const [anchor, setAnchor] = useState<number | null>(null);
  const [adding, setAdding] = useState<{ universe?: number } | null>(null);
  const [groupsHidden, setGroupsHidden] = useGroupsHidden();

  // Display order, for shift-click ranges.
  const ordered = useMemo(() => universes.flatMap((u) => fixturesIn(patch, u.id)), [universes, patch]);

  // Drop selected fixtures that no longer exist (removed, undone).
  useEffect(() => {
    setSelected((sel) => sel.filter((n) => patch.some((f) => f.fixtureNumber === n)));
  }, [patch]);

  const select = (number: number, e: React.MouseEvent) => {
    if (e.shiftKey && anchor !== null) {
      const from = ordered.findIndex((f) => f.fixtureNumber === anchor);
      const to = ordered.findIndex((f) => f.fixtureNumber === number);
      const [lo, hi] = from < to ? [from, to] : [to, from];
      const range = ordered.slice(lo, hi + 1).map((f) => f.fixtureNumber);
      setSelected((sel) => [...sel.filter((n) => !range.includes(n)), ...range]);
      return;
    }
    setAnchor(number);
    if (e.metaKey || e.ctrlKey) {
      setSelected((sel) => (sel.includes(number) ? sel.filter((n) => n !== number) : [...sel, number]));
    } else {
      setSelected([number]);
    }
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isEditable(e.target)) return;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "a") {
        e.preventDefault();
        setSelected(ordered.map((f) => f.fixtureNumber));
      } else if (e.key === "Escape") {
        setSelected([]);
      } else if ((e.key === "Backspace" || e.key === "Delete") && selected.length > 0) {
        e.preventDefault();
        void run(() => showApi.removeFixtures(selected));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [ordered, selected, run]);

  if (!show) return null;

  return (
    <div className="flex h-full overflow-hidden">
      <div className="flex flex-1 flex-col overflow-hidden">
        <div className="flex items-center gap-2 border-b border-border px-4 py-2">
          <div className="flex rounded-md border border-border p-0.5">
            {(Object.keys(VIEW_LABELS) as View[]).map((v) => (
              <button
                key={v}
                onClick={() => setView(v)}
                className={cn(
                  "rounded px-3 py-1 text-xs font-medium",
                  view === v ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:text-foreground",
                )}
              >
                {VIEW_LABELS[v]}
              </button>
            ))}
          </div>
          <span className="text-xs text-muted-foreground">
            {selected.length > 0 ? `${selected.length} selected` : `${patch.length} fixtures`}
          </span>
          {selected.length > 0 && (
            <Button size="sm" variant="ghost" onClick={() => void run(() => showApi.removeFixtures(selected))}>
              <Trash2 />
              Remove
            </Button>
          )}
          <div className="flex-1" />
          <Button size="sm" onClick={() => setAdding({})} disabled={universes.length === 0}>
            <Plus />
            Add fixtures
          </Button>
          <Button
            size="iconSm"
            variant="ghost"
            title={groupsHidden ? "Show groups" : "Hide groups"}
            className="text-muted-foreground"
            onClick={() => setGroupsHidden(!groupsHidden)}
          >
            {groupsHidden ? <PanelRightOpen /> : <PanelRightClose />}
          </Button>
        </div>

        <div className="flex-1 overflow-y-auto p-4">
          {universes.length === 0 && (
            <p className="text-sm text-muted-foreground">No universes yet. Add your controllers in Setup first.</p>
          )}
          <div className="flex flex-col gap-4">
            {universes.map((universe) =>
              view === "dmx" ? (
                <RawDmx key={universe.id} universe={universe} patch={fixturesIn(patch, universe.id)} library={library} />
              ) : (
                <UniverseSection
                  key={universe.id}
                  universe={universe}
                  fixtures={fixturesIn(patch, universe.id)}
                  library={library}
                  onAdd={() => setAdding({ universe: universe.id })}
                >
                  {(fixture, mode, def) =>
                    view === "patch" ? (
                      <PatchRow
                        key={fixture.id}
                        fixture={fixture}
                        def={def}
                        mode={mode}
                        universes={universes}
                        selected={selected.includes(fixture.fixtureNumber)}
                        unresolved={show.unresolved.includes(fixture.fixtureNumber)}
                        onSelect={(e) => select(fixture.fixtureNumber, e)}
                      />
                    ) : (
                      <ValuesRow
                        key={fixture.id}
                        fixture={fixture}
                        mode={mode}
                        selected={selected.includes(fixture.fixtureNumber)}
                        onSelect={(e) => select(fixture.fixtureNumber, e)}
                      />
                    )
                  }
                </UniverseSection>
              ),
            )}
          </div>
        </div>
      </div>

      {!groupsHidden && <GroupsPanel selected={selected} onSelect={setSelected} />}

      {adding && <AddFixturesDialog initialUniverse={adding.universe} onClose={() => setAdding(null)} />}
    </div>
  );
}

function UniverseSection({
  universe,
  fixtures,
  library,
  onAdd,
  children,
}: {
  universe: UniverseConfig;
  fixtures: PatchedFixture[];
  library: FixtureDefinition[];
  onAdd: () => void;
  children: (fixture: PatchedFixture, mode: FixtureMode | undefined, def: FixtureDefinition | undefined) => React.ReactNode;
}) {
  const status = useUniverseStatusesStore((s) => s.statuses[universe.id]);
  const used = fixtures.reduce((sum, f) => sum + (findMode(library, f).mode?.channelCount ?? 1), 0);
  const live = universe.driver.kind !== "null" && status?.connected;

  return (
    <section className="rounded-md border border-border">
      <header className="flex items-center gap-3 border-b border-border bg-card px-3 py-2">
        <span
          className={cn("h-2 w-2 rounded-full", live ? "bg-success" : universe.driver.kind === "null" ? "bg-muted-foreground/40" : "bg-destructive")}
        />
        <span className="text-sm font-medium">
          U{universe.id} · {universe.name}
        </span>
        <span className="truncate text-xs text-muted-foreground">{controllerLabel(universe.driver)}</span>
        <div className="flex-1" />
        <span className="text-xs text-muted-foreground">
          {fixtures.length} fixtures · {used}/512 ch
        </span>
        <Button size="iconSm" variant="ghost" title="Add fixtures to this universe" onClick={onAdd}>
          <Plus />
        </Button>
      </header>
      {fixtures.length === 0 ? (
        <p className="px-3 py-3 text-xs text-muted-foreground">No fixtures.</p>
      ) : (
        <div className="divide-y divide-border">
          {fixtures.map((f) => {
            const { def, mode } = findMode(library, f);
            return children(f, mode, def);
          })}
        </div>
      )}
    </section>
  );
}

/** A text/number field that commits on blur or Enter and snaps back to the
 * show's value if the edit is refused. */
function CommitField({
  value,
  onCommit,
  type = "text",
  className,
  label,
}: {
  value: string | number;
  onCommit: (value: string) => void;
  type?: "text" | "number";
  className?: string;
  label: string;
}) {
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const commit = () => {
    if (draft.trim() !== String(value)) onCommit(draft.trim());
    else setDraft(String(value));
  };
  return (
    <Input
      type={type}
      aria-label={label}
      className={cn("h-7 px-2 text-xs", className)}
      value={draft}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        if (e.key === "Escape") {
          setDraft(String(value));
          (e.target as HTMLInputElement).blur();
        }
      }}
    />
  );
}

function Toggle({ on, label, title, onChange }: { on: boolean; label: string; title: string; onChange: (on: boolean) => void }) {
  return (
    <button
      title={title}
      onClick={(e) => {
        e.stopPropagation();
        onChange(!on);
      }}
      className={cn(
        "rounded border px-1.5 py-0.5 text-[10px] font-medium",
        on ? "border-primary bg-primary/20 text-primary" : "border-border text-muted-foreground hover:text-foreground",
      )}
    >
      {label}
    </button>
  );
}

function PatchRow({
  fixture,
  def,
  mode,
  universes,
  selected,
  unresolved,
  onSelect,
}: {
  fixture: PatchedFixture;
  def?: FixtureDefinition;
  mode?: FixtureMode;
  universes: UniverseConfig[];
  selected: boolean;
  unresolved: boolean;
  onSelect: (e: React.MouseEvent) => void;
}) {
  const run = useShowStore((s) => s.run);
  const update = (changes: Parameters<typeof showApi.updateFixture>[1]) =>
    void run(() => showApi.updateFixture(fixture.fixtureNumber, changes));
  const channels = mode?.channelCount ?? 1;
  const hasPan = hasAttribute(mode, "pan");
  const hasTilt = hasAttribute(mode, "tilt");

  return (
    <div
      onClick={onSelect}
      className={cn(
        "grid cursor-default grid-cols-[4rem_minmax(8rem,1fr)_minmax(8rem,1.2fr)_6rem_7rem_9rem_2rem] items-center gap-3 px-3 py-1.5 text-xs",
        selected ? "bg-primary/15" : "hover:bg-accent/40",
      )}
    >
      <CommitField label="Fixture number" type="number" value={fixture.fixtureNumber} onCommit={(v) => update({ fixtureNumber: Number(v) })} />
      <CommitField label="Name" value={fixture.name} onCommit={(name) => update({ name })} />
      <span className={cn("truncate", unresolved ? "text-destructive" : "text-muted-foreground")} title={unresolved ? "Fixture type not in the library" : undefined}>
        {def ? `${def.manufacturer} ${def.model}` : fixture.fixtureId} · {mode?.name ?? `mode ${fixture.modeIndex + 1}`}
      </span>
      <div className="flex items-center gap-1">
        <CommitField label="Address" type="number" className="w-14" value={fixture.address} onCommit={(v) => update({ address: Number(v) })} />
        <span className="text-muted-foreground">–{fixture.address + channels - 1}</span>
      </div>
      <Select
        aria-label="Universe"
        className="h-7 px-1 text-xs"
        value={fixture.universe}
        onClick={(e) => e.stopPropagation()}
        onChange={(e) => update({ universe: Number(e.target.value) })}
      >
        {universes.map((u) => (
          <option key={u.id} value={u.id}>
            U{u.id} · {u.name}
          </option>
        ))}
      </Select>
      <div className="flex gap-1">
        {hasPan && <Toggle on={fixture.invertPan} label="PAN ⇄" title="Invert pan" onChange={(v) => update({ invertPan: v })} />}
        {hasTilt && <Toggle on={fixture.invertTilt} label="TILT ⇅" title="Invert tilt" onChange={(v) => update({ invertTilt: v })} />}
        {hasPan && hasTilt && <Toggle on={fixture.swapPanTilt} label="SWAP" title="Swap pan and tilt" onChange={(v) => update({ swapPanTilt: v })} />}
      </div>
      <Button
        size="iconSm"
        variant="ghost"
        title="Remove fixture"
        onClick={(e) => {
          e.stopPropagation();
          void run(() => showApi.removeFixtures([fixture.fixtureNumber]));
        }}
      >
        <Trash2 />
      </Button>
    </div>
  );
}

/** Attributes the Values view gives their own column; the rest are listed. */
const MAIN_ATTRIBUTES = new Set(["dimmer", "red", "green", "blue", "white", "amber", "uv", "cyan", "magenta", "yellow", "pan", "tilt"]);

function percent(value: number): string {
  return `${Math.round((value / 255) * 100)}%`;
}

function ValuesRow({
  fixture,
  mode,
  selected,
  onSelect,
}: {
  fixture: PatchedFixture;
  mode?: FixtureMode;
  selected: boolean;
  onSelect: (e: React.MouseEvent) => void;
}) {
  const frame = useUniverseChannelsStore((s) => s.channels[fixture.universe]);
  if (!mode) {
    return (
      <div onClick={onSelect} className="px-3 py-1.5 text-xs text-destructive">
        #{fixture.fixtureNumber} {fixture.name}: fixture type not in the library
      </div>
    );
  }
  const values = channelValues(fixture, mode, frame);
  const valueOf = (type: string) => {
    const ch = mode.channels.find((c) => c.type === type);
    return ch ? values[ch.offset] : undefined;
  };
  const dimmer = valueOf("dimmer");
  const pan = valueOf("pan");
  const tilt = valueOf("tilt");
  const others = mode.channels.filter((c) => !MAIN_ATTRIBUTES.has(c.type));
  const strobing = strobeHz(mode, values);

  return (
    <div
      onClick={onSelect}
      className={cn(
        "grid grid-cols-[2rem_4rem_minmax(8rem,1fr)_8rem_5rem_5rem_minmax(10rem,2fr)] items-center gap-3 px-3 py-1.5 text-xs",
        selected ? "bg-primary/15" : "hover:bg-accent/40",
      )}
    >
      <span
        className="h-5 w-5 rounded border border-border"
        style={{ backgroundColor: swatchColor(mode, values), animation: strobing > 0 ? `strobe-flash ${1 / strobing}s step-end infinite` : undefined }}
        title={strobing > 0 ? `Strobing at about ${strobing.toFixed(1)} Hz` : undefined}
      />
      <span className="tabular-nums text-muted-foreground">#{fixture.fixtureNumber}</span>
      <span className="truncate">{fixture.name}</span>
      <div className="flex items-center gap-2" title="Intensity">
        {dimmer === undefined ? (
          <span className="text-muted-foreground">—</span>
        ) : (
          <>
            <div className="h-1.5 flex-1 overflow-hidden rounded bg-muted">
              <div className="h-full bg-primary" style={{ width: percent(dimmer) }} />
            </div>
            <span className="w-9 text-right tabular-nums">{percent(dimmer)}</span>
          </>
        )}
      </div>
      <span className="tabular-nums text-muted-foreground" title="Pan">
        {pan === undefined ? "" : `P ${pan}`}
      </span>
      <span className="tabular-nums text-muted-foreground" title="Tilt">
        {tilt === undefined ? "" : `T ${tilt}`}
      </span>
      <div className="flex flex-wrap gap-1">
        {others.map((c) => (
          <span key={c.offset} className="rounded bg-muted px-1.5 py-0.5 tabular-nums text-muted-foreground" title={c.label ?? c.type}>
            {(c.label ?? c.type).slice(0, 10)} {values[c.offset]}
          </span>
        ))}
      </div>
    </div>
  );
}

/** One universe's 512 output channels; patched ranges are tinted per fixture. */
function RawDmx({ universe, patch, library }: { universe: UniverseConfig; patch: PatchedFixture[]; library: FixtureDefinition[] }) {
  const frame = useUniverseChannelsStore((s) => s.channels[universe.id]);
  const owner = useMemo(() => {
    const map = new Map<number, PatchedFixture>();
    for (const f of patch) {
      const count = findMode(library, f).mode?.channelCount ?? 1;
      for (let ch = f.address; ch < f.address + count; ch++) map.set(ch, f);
    }
    return map;
  }, [patch, library]);

  return (
    <section className="rounded-md border border-border">
      <header className="flex items-center gap-3 border-b border-border bg-card px-3 py-2">
        <span className="text-sm font-medium">
          U{universe.id} · {universe.name}
        </span>
        <span className="text-xs text-muted-foreground">{controllerLabel(universe.driver)}</span>
      </header>
      <div className="grid grid-cols-[repeat(32,minmax(0,1fr))] gap-px p-2 font-mono text-[10px]">
        {Array.from({ length: 512 }, (_, i) => {
          const value = frame?.[i] ?? 0;
          const fixture = owner.get(i + 1);
          return (
            <div
              key={i}
              title={`${i + 1}${fixture ? ` · #${fixture.fixtureNumber} ${fixture.name}` : ""}: ${value}`}
              className={cn(
                "flex h-6 items-center justify-center rounded-sm tabular-nums",
                fixture ? (fixture.fixtureNumber % 2 ? "bg-primary/10" : "bg-primary/20") : "bg-muted/40",
                value > 0 ? "text-foreground" : "text-muted-foreground/50",
              )}
              style={value > 0 ? { boxShadow: `inset 0 -${Math.max(1, Math.round((value / 255) * 24))}px 0 hsl(var(--primary) / 0.35)` } : undefined}
            >
              {value}
            </div>
          );
        })}
      </div>
    </section>
  );
}
