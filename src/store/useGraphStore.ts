import { create } from "zustand";
import * as graphApi from "../lib/graph";
import type { NodeKind, NodeTypeInfo } from "../lib/showfile/generated";

interface GraphState {
  nodeTypes: NodeTypeInfo[];
  /** Node type info by kind. */
  byKind: Partial<Record<NodeKind, NodeTypeInfo>>;
  /** Each node's live output values, by node id. */
  values: Record<string, number[][]>;
  init: () => Promise<void>;
}

export const useGraphStore = create<GraphState>((set) => ({
  nodeTypes: [],
  byKind: {},
  values: {},

  init: async () => {
    try {
      const nodeTypes = await graphApi.nodeTypes();
      set({ nodeTypes, byKind: Object.fromEntries(nodeTypes.map((t) => [t.kind, t])) });
      await graphApi.onGraphValues((values) => set({ values }));
    } catch (err) {
      // Expected outside the Tauri runtime (plain browser preview).
      console.warn("Graph backend unavailable:", err);
    }
  },
}));
