import { useEffect, useRef, useState } from "react";
import * as graphApi from "../lib/graph";
import type { NodeView } from "../lib/showfile/generated";
import { cn } from "../lib/utils";
import { useShowStore } from "../store/useShowStore";
import { VerticalFader } from "./ui/vertical-fader";

const POLL_MS = 300;
/** How long a locally-moved control is trusted over the polled value. */
const HOLD_MS = 600;

function label(view: NodeView): string {
  return String(view.params.label ?? (view.kind === "fader" ? "Fader" : "Button"));
}

/** Live control values, polled (an MCP client can move them too), with
 * local moves shown immediately and trusted for a moment. */
function useControls(): [Record<string, number>, (id: string, value: number) => void] {
  const [remote, setRemote] = useState<Record<string, number>>({});
  const [local, setLocal] = useState<Record<string, { value: number; at: number }>>({});
  useEffect(() => {
    const poll = () => void graphApi.controls().then(setRemote).catch(() => undefined);
    poll();
    const id = setInterval(poll, POLL_MS);
    return () => clearInterval(id);
  }, []);
  const now = Date.now();
  const merged = { ...remote };
  for (const [id, { value, at }] of Object.entries(local)) if (now - at < HOLD_MS) merged[id] = value;
  const set = (id: string, value: number) => {
    setLocal((l) => ({ ...l, [id]: { value, at: Date.now() } }));
    void graphApi.setControl(id, value).catch(() => undefined);
  };
  return [merged, set];
}

function FlashButton({ view, value, onChange }: { view: NodeView; value: number; onChange: (v: number) => void }) {
  const toggle = view.params.mode === "toggle";
  const held = useRef(false);
  const on = value > 0.5;
  return (
    <button
      className={cn(
        "h-11 w-20 select-none truncate rounded-md border px-2 text-xs font-semibold uppercase tracking-wide transition-colors",
        on ? "border-primary bg-primary text-primary-foreground shadow-lg shadow-primary/30" : "border-border bg-muted text-muted-foreground hover:text-foreground",
      )}
      title={toggle ? "Toggle" : "Flash: on while held"}
      onPointerDown={(e) => {
        (e.target as HTMLElement).setPointerCapture(e.pointerId);
        held.current = true;
        onChange(toggle ? (on ? 0 : 1) : 1);
      }}
      onPointerUp={() => {
        if (held.current && !toggle) onChange(0);
        held.current = false;
      }}
    >
      {label(view)}
    </button>
  );
}

/** The bottom bar: one control per Fader and Button node in the graph, in
 * the order they sit on the canvas (left to right). */
export function PlaybackBar() {
  const nodes = useShowStore((s) => s.show?.graph.nodes);
  const [values, setValue] = useControls();
  const controls = (nodes ?? [])
    .filter((n) => n.kind === "fader" || n.kind === "button")
    .sort((a, b) => a.position.x - b.position.x || a.position.y - b.position.y);

  if (controls.length === 0) return null;

  return (
    <div className="flex shrink-0 items-end gap-3 overflow-x-auto border-t border-border bg-card px-4 py-3">
      {controls.map((view) =>
        view.kind === "fader" ? (
          <div key={view.id} className="flex w-16 flex-col items-center gap-1">
            <VerticalFader
              value={Math.round((values[view.id] ?? 0) * 100)}
              onChange={(v) => setValue(view.id, v / 100)}
              title={label(view)}
              label={`${Math.round((values[view.id] ?? 0) * 100)}%`}
              className="h-24"
            />
            <span className="w-full truncate text-center text-[11px] text-muted-foreground">{label(view)}</span>
          </div>
        ) : (
          <FlashButton key={view.id} view={view} value={values[view.id] ?? 0} onChange={(v) => setValue(view.id, v)} />
        ),
      )}
    </div>
  );
}
