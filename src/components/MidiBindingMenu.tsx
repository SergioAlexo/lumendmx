import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { describeTrigger, learnMidi, toTrigger } from "../lib/midiLearn";
import type { MidiEvent, MidiTrigger } from "../lib/showfile/generated";
import { Button } from "./ui/button";

/** A right-click popup to bind something to a MIDI pad, button or knob:
 * shows the binding, learns a new one from the next message `accept` takes,
 * or unbinds. Used by TAP and the playback bar. */
export function MidiBindingMenu({
  x,
  y,
  title,
  binding,
  accept,
  hint,
  noPorts,
  onBind,
  onClose,
}: {
  x: number;
  y: number;
  title: string;
  binding: MidiTrigger | null;
  accept: (e: MidiEvent) => boolean;
  /** What to press while learning, e.g. "Press a pad or button…". */
  hint: string;
  /** No controller port is ticked in Setup, so nothing can arrive. */
  noPorts?: boolean;
  onBind: (binding: MidiTrigger | null) => void;
  onClose: () => void;
}) {
  const [learning, setLearning] = useState<AbortController | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("pointerdown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  // Closing stops learning.
  useEffect(() => () => learning?.abort(), [learning]);

  const learn = async () => {
    const abort = new AbortController();
    setLearning(abort);
    const hit = await learnMidi(accept, abort.signal);
    setLearning(null);
    if (hit) {
      onBind(toTrigger(hit));
      onClose();
    }
  };

  return createPortal(
    <div
      ref={ref}
      className="fixed z-50 flex w-60 flex-col gap-2 rounded-md border border-border bg-card p-3 text-sm shadow-2xl"
      style={{ left: Math.min(x, window.innerWidth - 250), top: Math.min(y + 4, window.innerHeight - 150) }}
      onContextMenu={(e) => e.preventDefault()}
    >
      <span className="truncate text-xs font-semibold uppercase tracking-wider text-muted-foreground">{title}</span>
      <span>{binding ? describeTrigger(binding) : <span className="text-muted-foreground">Not bound to MIDI</span>}</span>
      {noPorts && <span className="text-xs text-muted-foreground">Tick a controller port in Setup first.</span>}
      <div className="flex gap-2">
        {learning ? (
          <>
            <span className="flex-1 animate-pulse self-center text-xs">{hint}</span>
            <Button size="sm" variant="ghost" onClick={() => learning.abort()}>
              Cancel
            </Button>
          </>
        ) : (
          <>
            <Button size="sm" variant="secondary" onClick={() => void learn()}>
              {binding ? "Learn again" : "Learn MIDI"}
            </Button>
            {binding && (
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  onBind(null);
                  onClose();
                }}
              >
                Unbind
              </Button>
            )}
          </>
        )}
      </div>
    </div>,
    document.body,
  );
}
