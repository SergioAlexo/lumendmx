import { create } from "zustand";
import * as graphApi from "../lib/graph";
import type { NodeKind, NodeTypeInfo } from "../lib/showfile/generated";

interface GraphState {
  nodeTypes: NodeTypeInfo[];
  /** Node type info by kind. */
  byKind: Partial<Record<NodeKind, NodeTypeInfo>>;
  /** Each node's live output values, by node id. */
  values: Record<string, number[][]>;
  /** The same for the draft graph, while there is one. */
  draftValues: Record<string, number[][]>;
  /** Which graph the editor shows: the playing one or the draft. */
  view: "live" | "draft";
  setView: (view: "live" | "draft") => void;
  /** A node the editor should show and select (e.g. from the playback
   * bar). `seq` makes asking again for the same node count. */
  focus: { id: string; seq: number } | null;
  focusNode: (id: string) => void;
  init: () => Promise<void>;
}

export const useGraphStore = create<GraphState>((set) => ({
  nodeTypes: [],
  byKind: {},
  values: {},
  draftValues: {},
  view: "live",
  setView: (view) => set({ view }),
  focus: null,
  focusNode: (id) => set((s) => ({ focus: { id, seq: (s.focus?.seq ?? 0) + 1 } })),

  init: async () => {
    try {
      const nodeTypes = await graphApi.nodeTypes();
      set({ nodeTypes, byKind: Object.fromEntries(nodeTypes.map((t) => [t.kind, t])) });
      await graphApi.onGraphValues((values) => set({ values }));
      await graphApi.onDraftValues((draftValues) => set({ draftValues }));
    } catch (err) {
      // Expected outside the Tauri runtime (plain browser preview).
      console.warn("Graph backend unavailable:", err);
    }
  },
}));
