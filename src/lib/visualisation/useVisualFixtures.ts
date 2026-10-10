import { useMemo } from "react";
import type { FixtureDefinition, FixtureMode } from "../fixtures/types";
import { getAllFixtures, useFixtureLibraryStore } from "../../store/useFixtureLibraryStore";
import { usePatchStore, type PatchedFixture } from "../../store/usePatchStore";

export interface VisualFixture {
  instance: PatchedFixture;
  def: FixtureDefinition;
  mode: FixtureMode;
}

/** Patched fixtures whose library definition and mode resolve, in fixture-number order. */
export function useVisualFixtures(): VisualFixture[] {
  const fixtures = usePatchStore((s) => s.fixtures);
  const custom = useFixtureLibraryStore((s) => s.customFixtures);
  return useMemo(() => {
    const library = getAllFixtures();
    const out: VisualFixture[] = [];
    for (const instance of fixtures) {
      const def = library.find((f) => f.id === instance.fixtureId);
      const mode = def?.modes[instance.modeIndex];
      if (def && mode) out.push({ instance, def, mode });
    }
    return out.sort((a, b) => a.instance.fixtureNumber - b.instance.fixtureNumber);
  }, [fixtures, custom]);
}
