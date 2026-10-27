import type { GraphView } from "../lib/showfile/generated";
import { useGraphStore } from "./useGraphStore";
import { useShowStore } from "./useShowStore";

/** The graph the editor shows: the draft while the Draft view is on and
 * there is one, else the playing graph. `readOnly` while a draft exists but
 * the playing graph is shown: edits would land in the draft. */
export function useEditedGraph(): { graph: GraphView | undefined; view: "live" | "draft"; readOnly: boolean; hasDraft: boolean } {
  const live = useShowStore((s) => s.show?.graph);
  const draft = useShowStore((s) => s.show?.draft ?? undefined);
  const wanted = useGraphStore((s) => s.view);
  const view = wanted === "draft" && draft ? "draft" : "live";
  return { graph: view === "draft" ? draft : live, view, readOnly: view === "live" && draft !== undefined, hasDraft: draft !== undefined };
}
