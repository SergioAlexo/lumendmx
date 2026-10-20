import { create } from "zustand";
import * as dmx from "../lib/dmx";
import * as engine from "../lib/engine";

/** The two output-wide masters in the top bar: blackout and grand master. */
interface DmxStore {
  blackout: boolean;
  /** 0-100. */
  grandMaster: number;

  init: () => Promise<void>;
  toggleBlackout: () => Promise<void>;
  setGrandMaster: (percent: number) => void;
}

export const useDmxStore = create<DmxStore>((set, get) => ({
  blackout: false,
  grandMaster: 100,

  init: async () => {
    try {
      const [blackout, grandMaster] = await Promise.all([dmx.getBlackout(), engine.grandMaster()]);
      set({ blackout, grandMaster });
    } catch (err) {
      // Expected outside the Tauri runtime (plain browser preview).
      console.warn("DMX backend unavailable:", err);
    }
  },

  toggleBlackout: async () => {
    const next = !get().blackout;
    await dmx.setBlackout(next);
    set({ blackout: next });
  },

  setGrandMaster: (percent) => {
    set({ grandMaster: percent });
    void engine.setGrandMaster(percent).catch(() => undefined);
  },
}));
