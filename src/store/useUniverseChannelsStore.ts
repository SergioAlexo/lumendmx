import { create } from "zustand";
import * as engineApi from "../lib/engine";

interface UniverseChannelsState {
  /** Keyed by universe id. Populated from the engine's frame event, so
   * this works for any universe, not just the primary one `useDmxStore`
   * tracks for the top bar's quick-connect UI. */
  channels: Record<number, Uint8Array>;
  init: () => Promise<void>;
}

function sameFrame(a: Uint8Array | undefined, b: Uint8Array): boolean {
  if (!a || a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

export const useUniverseChannelsStore = create<UniverseChannelsState>((set, get) => {
  // Frames for several universes arrive as separate events; collect them and
  // commit once per animation frame so subscribers re-render once, not once
  // per universe per event.
  let pending: Record<number, Uint8Array> = {};
  let scheduled = false;
  const flush = () => {
    scheduled = false;
    const next = pending;
    pending = {};
    set((s) => ({ channels: { ...s.channels, ...next } }));
  };

  return {
    channels: {},

    init: async () => {
      try {
        // Must be awaited, not fire-and-forget: `listen()` rejects
        // asynchronously outside the Tauri runtime, so an un-awaited call here
        // would surface as an unhandled promise rejection instead of being
        // caught below.
        await engineApi.onUniverseFrame((frame) => {
          const data = Uint8Array.from(frame.channels);
          // The engine re-sends unchanged frames as a keepalive; skipping
          // them keeps an idle show from re-rendering anything.
          if (!(frame.universeId in pending) && sameFrame(get().channels[frame.universeId], data)) return;
          pending[frame.universeId] = data;
          if (!scheduled) {
            scheduled = true;
            requestAnimationFrame(flush);
          }
        });
      } catch (err) {
        // Expected outside the Tauri runtime (plain browser preview).
        console.warn("Could not subscribe to universe frames:", err);
      }
    },
  };
});

/** Returns a reader for live channel values (1-based `channel`). Subscribes
 * the calling component to engine frames so controlled sliders re-render as
 * values arrive — a plain `getState()` read would leave them stuck, snapping
 * back to the last rendered value after every drag. */
export function useUniverseChannelReader(): (universe: number, channel: number) => number {
  const channels = useUniverseChannelsStore((s) => s.channels);
  return (universe, channel) => channels[universe]?.[channel - 1] ?? 0;
}
