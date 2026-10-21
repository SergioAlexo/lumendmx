import { useEffect, useMemo, useRef, useState } from "react";
import { Plus } from "lucide-react";
import * as graphApi from "../lib/graph";
import * as timing from "../lib/timing";
import type { MidiEvent, MidiPortInfo, NodeView } from "../lib/showfile/generated";
import { cn } from "../lib/utils";
import { useShowStore } from "../store/useShowStore";
import { Button } from "./ui/button";

const POLL_MS = 50;
const LOG_LEN = 64;
/** How long a control stays highlighted after it moves. */
const FRESH_MS = 400;

const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/** 60 → "C4". */
function noteName(n: number): string {
  return `${NOTE_NAMES[n % 12]}${Math.floor(n / 12) - 1}`;
}

const KIND_LABELS: Record<MidiEvent["kind"], string> = {
  cc: "CC",
  noteOn: "Note on",
  noteOff: "Note off",
  pitchBend: "Pitch bend",
  programChange: "Program",
  aftertouch: "Aftertouch",
};

/** A control the graph's MIDI nodes can read: a CC or a note on one channel. */
type Seen = { isNote: boolean; channel: number; number: number; value: number; at: number; port: string };

const seenKey = (isNote: boolean, channel: number, number: number) => `${isNote ? "n" : "c"}${channel}:${number}`;

/** MIDI CC / MIDI Note nodes listening to this control (channel 0 is any). */
function listeners(nodes: NodeView[], s: Seen): NodeView[] {
  return nodes.filter((n) => {
    if (n.kind !== (s.isNote ? "midiNote" : "midiCc")) return false;
    const channel = Number(n.params.channel ?? 0);
    const number = Number(n.params[s.isNote ? "note" : "controller"] ?? (s.isNote ? 36 : 1));
    return (channel === 0 || channel === s.channel) && number === s.number;
  });
}

/** What the controller ports are sending: every knob, fader and pad as it
 * moves, which MIDI nodes read it, and the raw messages. */
export function MidiMonitor({ ports }: { ports: MidiPortInfo[] }) {
  const nodes = useShowStore((s) => s.show?.graph.nodes);
  const run = useShowStore((s) => s.run);
  const [log, setLog] = useState<MidiEvent[]>([]);
  const [seen, setSeen] = useState<Map<string, Seen>>(new Map());
  const [now, setNow] = useState(() => Date.now());
  const lastSeq = useRef(0);

  useEffect(() => {
    let stopped = false;
    const poll = async () => {
      try {
        const events = await timing.midiEvents(lastSeq.current);
        if (stopped) return;
        const at = Date.now();
        setNow(at);
        if (events.length === 0) return;
        // Older messages are already on screen when this isn't the first poll.
        const fresh = lastSeq.current === 0 ? events.slice(-LOG_LEN) : events;
        lastSeq.current = events[events.length - 1].seq;
        setLog((l) => [...fresh.reverse(), ...l].slice(0, LOG_LEN));
        setSeen((prev) => {
          const next = new Map(prev);
          for (const e of events) {
            const isNote = e.kind === "noteOn" || e.kind === "noteOff";
            if (!isNote && e.kind !== "cc") continue;
            next.set(seenKey(isNote, e.channel, e.number), { isNote, channel: e.channel, number: e.number, value: e.value, at, port: e.port });
          }
          return next;
        });
      } catch {
        // Outside Tauri: nothing to show.
      }
    };
    const id = setInterval(() => void poll(), POLL_MS);
    return () => {
      stopped = true;
      clearInterval(id);
    };
  }, []);

  const controls = useMemo(
    () => [...seen.values()].sort((a, b) => Number(a.isNote) - Number(b.isNote) || a.channel - b.channel || a.number - b.number),
    [seen],
  );
  const active = log.length > 0 && controls.some((c) => now - c.at < FRESH_MS);
  const portName = (id: string) => ports.find((p) => p.id === id)?.name ?? "Unplugged port";

  const addNode = (s: Seen) =>
    void run(() =>
      graphApi.addNode(s.isNote ? "midiNote" : "midiCc", { x: 0, y: 0 }, s.isNote ? { channel: s.channel, note: s.number } : { channel: s.channel, controller: s.number }),
    );

  return (
    <div className="flex flex-col gap-3 rounded-md bg-muted/50 p-3 text-sm">
      <div className="flex items-center justify-between gap-2">
        <span className="flex items-center gap-2">
          <span className={cn("h-2.5 w-2.5 rounded-full", active ? "bg-success" : "bg-muted-foreground/40")} />
          <span>{log.length === 0 ? "Move a knob or hit a pad to test it." : "MIDI monitor"}</span>
        </span>
        <Button
          size="sm"
          variant="ghost"
          disabled={log.length === 0}
          onClick={() => {
            setLog([]);
            setSeen(new Map());
          }}
        >
          Clear
        </Button>
      </div>

      {controls.length > 0 && (
        <div className="flex flex-col gap-1">
          <div className="grid grid-cols-[5.5rem_3.5rem_1fr_2.5rem_minmax(0,9rem)] gap-2 px-1 text-xs text-muted-foreground">
            <span>Control</span>
            <span>Channel</span>
            <span>Value</span>
            <span />
            <span>Used by</span>
          </div>
          {controls.map((c) => {
            const users = listeners(nodes ?? [], c);
            return (
              <div
                key={seenKey(c.isNote, c.channel, c.number)}
                className={cn(
                  "grid grid-cols-[5.5rem_3.5rem_1fr_2.5rem_minmax(0,9rem)] items-center gap-2 rounded px-1 text-xs transition-colors",
                  now - c.at < FRESH_MS && "bg-primary/15",
                )}
              >
                <span className="tabular-nums" title={`From ${portName(c.port)}`}>
                  {c.isNote ? `Note ${c.number} ${noteName(c.number)}` : `CC ${c.number}`}
                </span>
                <span className="tabular-nums">{c.channel}</span>
                <span className="h-2 overflow-hidden rounded bg-muted">
                  <span className="block h-full bg-primary" style={{ width: `${(c.value / 127) * 100}%` }} />
                </span>
                <span className="text-right tabular-nums">{c.value}</span>
                {users.length > 0 ? (
                  <span className="truncate" title={users.map((u) => String(u.params.label ?? u.id)).join(", ")}>
                    {users.length === 1 ? (c.isNote ? "MIDI Note" : "MIDI CC") : `${users.length} nodes`}
                  </span>
                ) : (
                  <button
                    className="flex items-center gap-1 text-muted-foreground hover:text-foreground"
                    onClick={() => addNode(c)}
                    title={`Add a ${c.isNote ? "MIDI Note" : "MIDI CC"} node set to this ${c.isNote ? "note" : "controller"} and channel`}
                  >
                    <Plus className="h-3 w-3" />
                    Add node
                  </button>
                )}
              </div>
            );
          })}
        </div>
      )}

      {log.length > 0 && (
        <details>
          <summary className="cursor-pointer text-xs text-muted-foreground">Messages ({log.length})</summary>
          <div className="mt-1 max-h-48 overflow-y-auto font-mono text-xs">
            {log.map((e) => (
              <div key={e.seq} className="grid grid-cols-[6rem_3rem_5rem_3rem_minmax(0,1fr)] gap-2">
                <span>{KIND_LABELS[e.kind]}</span>
                <span className="text-muted-foreground">ch {e.channel}</span>
                <span>
                  {e.kind === "noteOn" || e.kind === "noteOff"
                    ? `${e.number} ${noteName(e.number)}`
                    : e.kind === "pitchBend" || e.kind === "programChange" || (e.kind === "aftertouch" && e.number === 0)
                      ? ""
                      : e.number}
                </span>
                <span className="tabular-nums">{e.kind === "programChange" ? e.number : e.value}</span>
                <span className="truncate text-muted-foreground">{portName(e.port)}</span>
              </div>
            ))}
          </div>
        </details>
      )}
    </div>
  );
}
