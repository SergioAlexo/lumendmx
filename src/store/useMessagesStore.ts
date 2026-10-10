import { create } from "zustand";
import * as reliability from "../lib/reliability";
import type { LogEntry } from "../lib/showfile/generated";

const MAX_ENTRIES = 1000;

/** Appends `entry`, dropping the oldest past the cap, ignoring a repeat of an
 * id already held (the initial fetch can race the live event). */
export function appendEntry(entries: LogEntry[], entry: LogEntry): LogEntry[] {
  if (entries.some((e) => e.id === entry.id)) return entries;
  const next = [...entries, entry];
  return next.length > MAX_ENTRIES ? next.slice(next.length - MAX_ENTRIES) : next;
}

export function countProblems(entries: LogEntry[], afterId: number): number {
  return entries.filter((e) => e.id > afterId && (e.level === "error" || e.level === "warn")).length;
}

interface MessagesStore {
  entries: LogEntry[];
  /** Highest entry id the operator has seen (window open). */
  seenUpTo: number;
  open: boolean;
  init: () => Promise<void>;
  setOpen: (open: boolean) => void;
  clear: () => Promise<void>;
}

let listening = false;

export const useMessagesStore = create<MessagesStore>((set, get) => ({
  entries: [],
  seenUpTo: 0,
  open: false,

  init: async () => {
    if (listening) return;
    listening = true;
    try {
      await reliability.onLogMessage((entry) => {
        set((s) => ({ entries: appendEntry(s.entries, entry) }));
      });
      const recent = await reliability.logRecent();
      set((s) => ({ entries: recent.reduce(appendEntry, s.entries) }));
    } catch (err) {
      listening = false;
      console.warn("Messages unavailable:", err);
    }
  },

  setOpen: (open) => {
    const last = get().entries.at(-1)?.id ?? 0;
    set(open ? { open, seenUpTo: last } : { open, seenUpTo: Math.max(get().seenUpTo, last) });
  },

  clear: async () => {
    await reliability.logClear().catch(() => undefined);
    set({ entries: [] });
  },
}));
