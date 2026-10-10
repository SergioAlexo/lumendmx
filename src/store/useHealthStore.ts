import { create } from "zustand";
import * as reliability from "../lib/reliability";
import type { HealthReport } from "../lib/showfile/generated";

interface HealthStore {
  report: HealthReport | null;
  init: () => Promise<void>;
  refresh: () => Promise<void>;
}

let listening = false;

/** Watchdog results from Rust: pushed on every change, fetched once at start. */
export const useHealthStore = create<HealthStore>((set) => ({
  report: null,

  init: async () => {
    if (listening) return;
    listening = true;
    try {
      await reliability.onHealthChanged((report) => set({ report }));
      set({ report: await reliability.healthReport() });
    } catch (err) {
      listening = false;
      console.warn("Health report unavailable:", err);
    }
  },

  refresh: async () => {
    try {
      set({ report: await reliability.healthReport() });
    } catch {
      // Not in the Tauri runtime.
    }
  },
}));

/** Whether the watchdog sees a problem worth showing the operator. */
export function hasProblem(report: HealthReport | null): boolean {
  return !!report && (report.engine !== "ok" || report.stalledOutputs.length > 0);
}
