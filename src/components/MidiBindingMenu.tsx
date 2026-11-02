import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { describeTrigger, learnMidi, toTrigger } from "../lib/midiLearn";
import type { MidiEvent, MidiTrigger } from "../lib/showfile/generated";
import { Button } from "./ui/button";

/** A row of buttons for choosing which of several bindings the menu edits (a fader's knob, up,
 * down and sweep). Goes in the menu's `header`. */
export function SlotTabs<T extends string>({ slots, value, onChange, title }: { slots: Record<T, string>; value: T; onChange: (slot: T) => void; title: string }) {
  return (
    <div className="flex gap-1" title={title}>
      {(Object.keys(slots) as T[]).map((slot) => (
        <Button key={slot} size="sm" variant={value === slot ? "default" : "ghost"} className="h-6 flex-1 px-2 text-xs" onClick={() => onChange(slot)}>
          {slots[slot]}
        </Button>
      ))}
    </div>
  );
}

/** "Full range in [ ] s": how fast held Up, Down and Sweep pads move a value. Saves on Enter or
 * when the field is left, not on every key. */
export function TravelField({
  seconds,
  onCommit,
  velocity,
  onVelocity,
}: {
  seconds: number;
  onCommit: (seconds: number) => void;
  /** Whether a harder hit is faster, and a way to change that. */
  velocity: boolean;
  onVelocity: (velocity: boolean) => void;
}) {
  const [text, setText] = useState(String(seconds));
  useEffect(() => setText(String(seconds)), [seconds]);
  const commit = () => {
    const next = Math.min(60, Math.max(0.1, Number(text)));
    if (Number.isFinite(next) && next !== seconds) onCommit(next);
    else setText(String(seconds));
  };
  return (
    <div className="flex flex-col gap-1.5">
      <label className="flex items-center gap-1.5 text-xs text-muted-foreground" title="How long a held Up, Down or Sweep pad takes to cross the whole range">
        Full range in
        <input
          type="number"
          min={0.1}
          max={60}
          step={0.1}
          value={text}
          className="h-6 w-16 rounded border border-border bg-background px-1.5 text-xs text-foreground"
          onChange={(e) => setText(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        />
        s{velocity && " at full velocity"}
      </label>
      <label
        className="flex items-center gap-1.5 text-xs text-muted-foreground"
        title="A harder hit on the pad moves faster, a soft one slower (down to a tenth). Needs velocity-sensitive pads: pads that always send the same value won't change."
      >
        <input type="checkbox" checked={velocity} onChange={(e) => onVelocity(e.target.checked)} />
        Faster with harder hits
      </label>
    </div>
  );
}

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
  header,
  footer,
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
  /** Shown under the title, e.g. a choice of which binding this edits. */
  header?: React.ReactNode;
  /** Shown under the buttons, e.g. a speed setting. */
  footer?: React.ReactNode;
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
      {header}
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
      {footer}
    </div>,
    document.body,
  );
}
