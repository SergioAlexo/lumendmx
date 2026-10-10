import { create } from "zustand";
import * as engineApi from "../lib/engine";

interface UniverseChannelsState {
  /** Keyed by universe id. Populated from the engine's ~20Hz frame event, so
   * this works for any universe, not just the primary one `useDmxStore`
   * tracks for the top bar's quick-connect UI. */
  channels: Record<number, Uint8Array>;
  init: () => Promise<void>;
}

export const useUniverseChannelsStore = create<UniverseChannelsState>((set) => ({
  channels: {},

  init: async () => {
    try {
      // Must be awaited, not fire-and-forget: `listen()` rejects
      // asynchronously outside the Tauri runtime, so an un-awaited call here
      // would surface as an unhandled promise rejection instead of being
      // caught below.
      await engineApi.onUniverseFrame((frame) => {
        set((s) => ({
          channels: { ...s.channels, [frame.universeId]: Uint8Array.from(frame.channels) },
        }));
      });
    } catch (err) {
      // Expected outside the Tauri runtime (plain browser preview).
      console.warn("Could not subscribe to universe frames:", err);
    }
  },
}));

/** Returns a reader for live channel values (1-based `channel`). Subscribes
 * the calling component to engine frames so controlled sliders re-render as
 * values arrive — a plain `getState()` read would leave them stuck, snapping
 * back to the last rendered value after every drag. */
export function useUniverseChannelReader(): (universe: number, channel: number) => number {
  const channels = useUniverseChannelsStore((s) => s.channels);
  return (universe, channel) => channels[universe]?.[channel - 1] ?? 0;
}
