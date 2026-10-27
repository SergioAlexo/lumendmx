import { useMemo, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { Group, PatchedFixture } from "../../lib/showfile/generated";
import { cn } from "../../lib/utils";
import { useAllFixtures } from "../../store/useFixtureLibraryStore";
import { useEditedGraph } from "../../store/useEditedGraph";
import { EMPTY_GROUPS, useShowStore } from "../../store/useShowStore";
import { useUniverseChannelsStore } from "../../store/useUniverseChannelsStore";
import { channelValues, findMode, strobeHz, strobeStyle, swatchColor } from "../fixtures/fixtureLook";
import type { FixtureDefinition } from "../../lib/fixtures/types";

const COLLAPSED_KEY = "lumendmx-fixture-preview-collapsed";

function useCollapsed(): [boolean, (collapsed: boolean) => void] {
  const [collapsed, setCollapsed] = useState(() => {
    try {
      return localStorage.getItem(COLLAPSED_KEY) === "1";
    } catch {
      return false;
    }
  });
  const set = (next: boolean) => {
    setCollapsed(next);
    try {
      localStorage.setItem(COLLAPSED_KEY, next ? "1" : "0");
    } catch {
      // Not remembered; still toggles for this session.
    }
  };
  return [collapsed, set];
}

/** One fixture: a square showing the colour it is putting out right now. */
function Cell({ fixture, library, draft }: { fixture: PatchedFixture; library: FixtureDefinition[]; draft: boolean }) {
  const frame = useUniverseChannelsStore((s) => (draft ? s.draft : s.channels)[fixture.universe]);
  const { mode } = findMode(library, fixture);
  const title = `#${fixture.fixtureNumber} ${fixture.name}`;
  if (!mode) {
    return (
      <div className="flex aspect-square items-center justify-center rounded border border-dashed border-destructive text-[10px] text-destructive" title={`${title}: fixture type not in the library`}>
        ?
      </div>
    );
  }
  const values = channelValues(fixture, mode, frame);
  const hz = strobeHz(mode, values);
  return (
    <div
      className="relative flex aspect-square items-start justify-start rounded border border-border"
      style={{ backgroundColor: swatchColor(mode, values), ...strobeStyle(hz) }}
      title={hz > 0 ? `${title} (strobing at about ${hz.toFixed(1)} Hz)` : title}
    >
      <span className="m-0.5 text-[9px] font-medium leading-none text-white mix-blend-difference">{fixture.fixtureNumber}</span>
    </div>
  );
}

type Section = { key: string; name: string | null; fixtures: PatchedFixture[] };

/** One section per group, its fixtures in the group's own order (the order a
 * fan spreads along), then the fixtures in no group. A fixture in two groups
 * shows in both. With no groups at all there is just one unnamed section. */
function sections(fixtures: PatchedFixture[], groups: Group[]): Section[] {
  const byNumber = new Map(fixtures.map((f) => [f.fixtureNumber, f]));
  const grouped = new Set<number>();
  const result: Section[] = [];
  for (const g of groups) {
    const members = g.fixtureNumbers.flatMap((n) => byNumber.get(n) ?? []);
    members.forEach((f) => grouped.add(f.fixtureNumber));
    if (members.length > 0) result.push({ key: `group-${g.id}`, name: g.name, fixtures: members });
  }
  const rest = fixtures.filter((f) => !grouped.has(f.fixtureNumber));
  if (rest.length > 0) result.push({ key: "ungrouped", name: result.length > 0 ? "Ungrouped" : null, fixtures: rest });
  return result;
}

/** The patched fixtures as a grid of squares, as they look right now. Folds
 * away to a thin strip. */
export function FixturePreview() {
  const patch = useShowStore((s) => s.show?.patch);
  const library = useAllFixtures();
  const groups = useShowStore((s) => s.show?.groups ?? EMPTY_GROUPS);
  const [collapsed, setCollapsed] = useCollapsed();
  const draft = useEditedGraph().view === "draft";
  const fixtures = useMemo(() => [...(patch ?? [])].sort((a, b) => a.fixtureNumber - b.fixtureNumber), [patch]);
  const parts = useMemo(() => sections(fixtures, groups), [fixtures, groups]);

  if (collapsed) {
    return (
      <button
        className="flex w-7 shrink-0 flex-col items-center gap-2 border-l border-border bg-card py-2 text-muted-foreground hover:text-foreground"
        onClick={() => setCollapsed(false)}
        title="Show the fixture preview"
      >
        <ChevronLeft className="h-4 w-4" />
        <span className="text-[11px] [writing-mode:vertical-rl]">Fixtures</span>
      </button>
    );
  }

  return (
    <aside className={cn("flex w-56 shrink-0 flex-col border-l border-border bg-card")}>
      <div className="flex items-center justify-between border-b border-border px-3 py-1.5 text-xs font-medium">
        <span>
          Fixtures <span className="text-muted-foreground">({fixtures.length})</span>
          {draft && <span className="ml-1.5 rounded bg-amber-500 px-1 text-[10px] text-black">draft preview</span>}
        </span>
        <button className="text-muted-foreground hover:text-foreground" onClick={() => setCollapsed(true)} title="Hide the fixture preview">
          <ChevronRight className="h-4 w-4" />
        </button>
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-2">
        {parts.length === 0 && <p className="text-xs text-muted-foreground">No fixtures patched yet.</p>}
        {parts.map((part) => (
          <section key={part.key} className="flex flex-col gap-1.5">
            {part.name !== null && (
              <h4 className="flex items-baseline justify-between border-b border-border pb-0.5 text-[11px] font-medium">
                <span className="truncate">{part.name}</span>
                <span className="text-muted-foreground">{part.fixtures.length}</span>
              </h4>
            )}
            <div className="grid gap-1.5" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(36px, 1fr))" }}>
              {part.fixtures.map((f) => (
                <Cell key={f.fixtureNumber} fixture={f} library={library} draft={draft} />
              ))}
            </div>
          </section>
        ))}
      </div>
    </aside>
  );
}
