import { useEffect, useRef, useState } from "react";
import * as graphApi from "../lib/graph";
import { describeTrigger, isPress } from "../lib/midiLearn";
import * as timing from "../lib/timing";
import type { MidiTrigger, NodeView } from "../lib/showfile/generated";
import { cn } from "../lib/utils";
import { useGraphStore } from "../store/useGraphStore";
import { useShowStore } from "../store/useShowStore";
import { MidiBindingMenu } from "./MidiBindingMenu";
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

const SHOW_NODE_HINT = "Right-click to bind MIDI, Ctrl+right-click to show its node";

/** A Button's or Fader's MIDI binding, from its params. */
function bindingOf(view: NodeView): MidiTrigger | null {
  const message = view.params.midi;
  if (message !== "note" && message !== "cc") return null;
  return { message, channel: Number(view.params.midiChannel ?? 0), number: Number(view.params.midiNumber ?? 0) };
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
      title={`${toggle ? "Toggle" : "Flash: on while held"}. ${SHOW_NODE_HINT}`}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
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
  const focusNode = useGraphStore((s) => s.focusNode);
  const run = useShowStore((s) => s.run);
  const [menu, setMenu] = useState<{ view: NodeView; x: number; y: number; noPorts: boolean } | null>(null);
  /** Right-click: the MIDI menu. Ctrl (or Cmd) + right-click: show the node. */
  const onContextMenu = (e: React.MouseEvent, view: NodeView) => {
    e.preventDefault();
    if (e.ctrlKey || e.metaKey) {
      setMenu(null);
      focusNode(view.id);
      return;
    }
    const at = { view, x: e.clientX, y: e.clientY };
    setMenu({ ...at, noPorts: false });
    void timing
      .timingStatus()
      .then((s) => setMenu((m) => (m && m.view.id === view.id ? { ...m, noPorts: s.settings.controllerPorts.length === 0 } : m)))
      .catch(() => undefined);
  };

  const bind = (view: NodeView, b: MidiTrigger | null) =>
    void run(() => graphApi.setParams(view.id, b ? { midi: b.message, midiChannel: b.channel, midiNumber: b.number } : { midi: "none" }));
  const controls = (nodes ?? [])
    .filter((n) => n.kind === "fader" || n.kind === "button")
    .sort((a, b) => a.position.x - b.position.x || a.position.y - b.position.y);

  if (controls.length === 0) return null;

  return (
    <div className="flex shrink-0 items-end gap-3 overflow-x-auto border-t border-border bg-card px-4 py-3">
      {controls.map((view) =>
        view.kind === "fader" ? (
          <div
            key={view.id}
            className="relative flex w-16 flex-col items-center gap-1"
            onContextMenu={(e) => onContextMenu(e, view)}
          >
            {bindingOf(view) && <span className="absolute right-0 top-0 h-1.5 w-1.5 rounded-full bg-primary" title={describeTrigger(bindingOf(view)!)} />}
            <VerticalFader
              value={Math.round((values[view.id] ?? 0) * 100)}
              onChange={(v) => setValue(view.id, v / 100)}
              title={`${label(view)}. ${SHOW_NODE_HINT}`}
              label={`${Math.round((values[view.id] ?? 0) * 100)}%`}
              className="h-24"
            />
            <span className="w-full truncate text-center text-[11px] text-muted-foreground">{label(view)}</span>
          </div>
        ) : (
          <div key={view.id} className="relative" onContextMenu={(e) => onContextMenu(e, view)}>
            <FlashButton view={view} value={values[view.id] ?? 0} onChange={(v) => setValue(view.id, v)} />
            {bindingOf(view) && (
              <span className="pointer-events-none absolute right-1 top-1 h-1.5 w-1.5 rounded-full bg-primary-foreground/80" title={describeTrigger(bindingOf(view)!)} />
            )}
          </div>
        ),
      )}
      {menu && (
        <MidiBindingMenu
          x={menu.x}
          y={menu.y}
          title={`${label(menu.view)} · MIDI`}
          binding={bindingOf(controls.find((c) => c.id === menu.view.id) ?? menu.view)}
          accept={menu.view.kind === "fader" ? (e) => e.kind === "cc" : isPress}
          hint={menu.view.kind === "fader" ? "Move a knob or fader…" : "Press a pad or button…"}
          noPorts={menu.noPorts}
          onBind={(b) => bind(menu.view, b)}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}
