import { Move, RotateCcw, SquareArrowOutUpRight, Undo2, Zap } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import * as graphApi from "../lib/graph";
import { describeTrigger, isPress } from "../lib/midiLearn";
import * as timing from "../lib/timing";
import type { MidiTrigger, NodeView } from "../lib/showfile/generated";
import { cn } from "../lib/utils";
import { popOut, poppedOutWindow } from "../lib/windows";
import { useGraphStore } from "../store/useGraphStore";
import { useShowStore } from "../store/useShowStore";
import { MidiBindingMenu } from "./MidiBindingMenu";
import { Button } from "./ui/button";
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
const MIDI_HINT = "Right-click to bind MIDI";

// Where a control sits on the strip is its `panelX` / `panelY` param, in pixels.
const GRID = 8;
const GAP = 16;
const FADER_SIZE = { w: 64, h: 120 };
const ROW_H = FADER_SIZE.h;

function defaultSize(view: NodeView): { w: number; h: number } {
  if (view.kind === "fader") return FADER_SIZE;
  return { w: 88, h: 44 };
}

const MIN_SIZE = { fader: { w: 40, h: 72 }, button: { w: 40, h: 28 } };

/** A control's size: what it was resized to (`panelW`, `panelH`), else the default. */
function sizeOf(view: NodeView): { w: number; h: number } {
  const { panelW, panelH } = view.params;
  const size = defaultSize(view);
  return { w: typeof panelW === "number" ? panelW : size.w, h: typeof panelH === "number" ? panelH : size.h };
}

/** A control's custom colour ("#rrggbb"), if it has one. */
function colorOf(view: NodeView): string | null {
  const c = view.params.color;
  return typeof c === "string" && /^#[0-9a-f]{6}$/i.test(c) ? c : null;
}

/** Black or white, whichever reads on `hex`. */
function contrastOn(hex: string): string {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  return 0.299 * r + 0.587 * g + 0.114 * b > 150 ? "#000000" : "#ffffff";
}

type Spot = { x: number; y: number };

/** Each control's place: where it was put, else in a row left to right (in canvas order), bottom-aligned. */
function layout(controls: NodeView[]): Map<string, Spot> {
  const spots = new Map<string, Spot>();
  let x = GAP;
  for (const view of controls) {
    const { panelX, panelY } = view.params;
    const size = sizeOf(view);
    if (typeof panelX === "number" && typeof panelY === "number") spots.set(view.id, { x: panelX, y: panelY });
    else spots.set(view.id, { x, y: GAP + ROW_H - size.h });
    x += size.w + GAP;
  }
  return spots;
}

const snap = (v: number) => Math.max(0, Math.round(v / GRID) * GRID);

/** A Button's or Fader's MIDI binding, from its params. */
function bindingOf(view: NodeView): MidiTrigger | null {
  const message = view.params.midi;
  if (message !== "note" && message !== "cc") return null;
  return { message, channel: Number(view.params.midiChannel ?? 0), number: Number(view.params.midiNumber ?? 0) };
}

function FlashButton({ view, value, hint, onChange }: { view: NodeView; value: number; hint: string; onChange: (v: number) => void }) {
  const toggle = view.params.mode === "toggle";
  const held = useRef(false);
  const on = value > 0.5;
  const color = colorOf(view);
  const style: React.CSSProperties | undefined = color
    ? toggle
      ? { borderColor: on ? color : `${color}80`, backgroundColor: on ? `${color}33` : `${color}14` }
      : on
        ? { borderColor: color, backgroundColor: color, color: contrastOn(color), boxShadow: `0 8px 16px -4px ${color}66` }
        : { borderColor: `${color}80`, backgroundColor: `${color}14`, color }
    : undefined;
  return (
    <button
      style={style}
      className={cn(
        "flex h-full w-full select-none items-center gap-1.5 border text-xs font-semibold uppercase tracking-wide transition-colors",
        toggle
          ? cn("rounded-md px-2", !color && (on ? "border-primary/70 bg-primary/15 text-foreground" : "border-border bg-muted text-muted-foreground hover:text-foreground"))
          : cn(
              "rounded-md px-2",
              !color && (on ? "border-primary bg-primary text-primary-foreground shadow-lg shadow-primary/30" : "border-border bg-muted text-muted-foreground hover:text-foreground"),
            ),
      )}
      title={`${toggle ? "Toggle" : "Flash: on while held"}. ${hint}`}
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
      {toggle ? (
        <span
          className={cn("h-2.5 w-2.5 shrink-0 rounded-full", !color && (on ? "bg-primary shadow-[0_0_6px_1px] shadow-primary" : "bg-muted-foreground/40"))}
          style={color ? { backgroundColor: on ? color : `${color}66`, boxShadow: on ? `0 0 6px 1px ${color}` : undefined } : undefined}
        />
      ) : (
        <Zap className="h-3 w-3 shrink-0" />
      )}
      <span className="min-w-0 truncate">{label(view)}</span>
    </button>
  );
}

/** Follows the controls window: whether it is open, and a way to close it. */
function useDetached(): [boolean, () => void, () => void] {
  const [window_, setWindow] = useState<Awaited<ReturnType<typeof popOut>> | null>(null);
  const watch = (w: Awaited<ReturnType<typeof popOut>>) => {
    setWindow(w);
    void w.once("tauri://destroyed", () => setWindow(null));
  };
  useEffect(() => {
    void poppedOutWindow("controls").then((w) => w && watch(w));
  }, []);
  const detach = () =>
    void popOut("controls")
      .then(watch)
      .catch((err) => useShowStore.setState({ error: `Couldn't open window: ${err}` }));
  const reattach = () => void window_?.close();
  return [window_ !== null, detach, reattach];
}

/** The playback controls: one per Fader and Button node in the graph being edited. They can
 * be moved anywhere (Arrange), and the whole strip can be popped out into its own window. */
export function PlaybackBar({ popped = false }: { popped?: boolean }) {
  // The draft's controls while there is one: it's a copy of the playing graph plus the edits,
  // and its buttons drive the draft preview (controls are shared by node id).
  const nodes = useShowStore((s) => (s.show?.draft ?? s.show?.graph)?.nodes);
  const [values, setValue] = useControls();
  const focusNode = useGraphStore((s) => s.focusNode);
  const run = useShowStore((s) => s.run);
  const [menu, setMenu] = useState<{ view: NodeView; x: number; y: number; noPorts: boolean } | null>(null);
  const [arranging, setArranging] = useState(false);
  const [drag, setDrag] = useState<(Spot & { id: string }) | null>(null);
  const grab = useRef<{ x: number; y: number; from: Spot } | null>(null);
  const [resize, setResize] = useState<{ id: string; w: number; h: number } | null>(null);
  const sizing = useRef<{ x: number; y: number; from: { w: number; h: number } } | null>(null);
  const [detached, detach, reattach] = useDetached();
  const hint = popped ? MIDI_HINT : SHOW_NODE_HINT;

  /** Right-click: the MIDI menu. Ctrl (or Cmd) + right-click: show the node. */
  const onContextMenu = (e: React.MouseEvent, view: NodeView) => {
    e.preventDefault();
    if (!popped && (e.ctrlKey || e.metaKey)) {
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
  const spots = layout(controls);

  /** Puts `id` at `to`. Controls with no place yet get the one they show now, so they stay put. */
  const place = (id: string, to: Spot) =>
    void run(async () => {
      for (const view of controls) {
        const placed = typeof view.params.panelX === "number" && typeof view.params.panelY === "number";
        const spot = view.id === id ? to : placed ? null : spots.get(view.id)!;
        if (spot) await graphApi.setParams(view.id, { panelX: spot.x, panelY: spot.y });
      }
    });
  const resetLayout = () =>
    void run(async () => {
      for (const view of controls) await graphApi.setParams(view.id, { panelX: null, panelY: null, panelW: null, panelH: null });
    });
  const setColor = (view: NodeView, color: string | null) => void run(() => graphApi.setParams(view.id, { color }));

  if (controls.length === 0) {
    return popped ? <p className="p-4 text-xs text-muted-foreground">Add Fader and Button nodes to the graph and they appear here.</p> : null;
  }

  if (detached && !popped) {
    return (
      <div className="flex shrink-0 items-center justify-between gap-3 border-t border-border bg-card px-4 py-2 text-xs text-muted-foreground">
        <span>Controls are in their own window.</span>
        <Button size="sm" variant="ghost" onClick={reattach}>
          <Undo2 /> Bring back
        </Button>
      </div>
    );
  }

  const at = (view: NodeView): Spot => (drag?.id === view.id ? drag : spots.get(view.id)!);
  const sizeAt = (view: NodeView) => (resize?.id === view.id ? resize : sizeOf(view));
  const extent = controls.reduce(
    (acc, view) => {
      const { x, y } = at(view);
      const { w, h } = sizeAt(view);
      return { w: Math.max(acc.w, x + w), h: Math.max(acc.h, y + h) };
    },
    { w: 0, h: 0 },
  );

  const startDrag = (e: React.PointerEvent, view: NodeView) => {
    if (e.button !== 0) return;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const from = at(view);
    grab.current = { x: e.clientX, y: e.clientY, from };
    setDrag({ id: view.id, ...from });
  };
  const moveDrag = (e: React.PointerEvent, view: NodeView) => {
    const g = grab.current;
    if (g) setDrag({ id: view.id, x: snap(g.from.x + e.clientX - g.x), y: snap(g.from.y + e.clientY - g.y) });
  };
  const endDrag = (view: NodeView) => {
    const g = grab.current;
    grab.current = null;
    if (g && drag && (drag.x !== g.from.x || drag.y !== g.from.y)) place(view.id, { x: drag.x, y: drag.y });
    setDrag(null);
  };

  const startResize = (e: React.PointerEvent, view: NodeView) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    const from = sizeOf(view);
    sizing.current = { x: e.clientX, y: e.clientY, from };
    setResize({ id: view.id, ...from });
  };
  const moveResize = (e: React.PointerEvent, view: NodeView) => {
    const g = sizing.current;
    if (!g) return;
    const min = MIN_SIZE[view.kind === "fader" ? "fader" : "button"];
    setResize({
      id: view.id,
      w: Math.max(min.w, snap(g.from.w + e.clientX - g.x)),
      h: Math.max(min.h, snap(g.from.h + e.clientY - g.y)),
    });
  };
  const endResize = (view: NodeView) => {
    const g = sizing.current;
    sizing.current = null;
    if (g && resize && (resize.w !== g.from.w || resize.h !== g.from.h)) void run(() => graphApi.setParams(view.id, { panelW: resize.w, panelH: resize.h }));
    setResize(null);
  };

  return (
    <div className={cn("flex bg-card", popped ? "h-full" : "shrink-0 border-t border-border")}>
      <div className={cn("min-w-0 flex-1 overflow-auto", !popped && "max-h-[45vh]")}>
        <div
          className={cn("relative", popped && "min-h-full min-w-full")}
          style={{ width: extent.w + GAP, height: extent.h + GAP }}
        >
          {controls.map((view) => {
            const { x, y } = at(view);
            const { w, h } = sizeAt(view);
            return (
              <div
                key={view.id}
                className="absolute"
                style={{ left: x, top: y, width: w, height: h }}
                onContextMenu={(e) => onContextMenu(e, view)}
              >
                {view.kind === "fader" ? (
                  <div className="relative flex h-full flex-col items-center gap-1">
                    {bindingOf(view) && <span className="absolute right-0 top-0 h-1.5 w-1.5 rounded-full bg-primary" title={describeTrigger(bindingOf(view)!)} />}
                    <VerticalFader
                      value={Math.round((values[view.id] ?? 0) * 100)}
                      onChange={(v) => setValue(view.id, v / 100)}
                      title={`${label(view)}. ${hint}`}
                      label={`${Math.round((values[view.id] ?? 0) * 100)}%`}
                      color={colorOf(view) ?? undefined}
                      className="h-auto min-h-0 w-full flex-1"
                    />
                    <span className="w-full truncate text-center text-[11px] text-muted-foreground">{label(view)}</span>
                  </div>
                ) : (
                  <div className="relative h-full">
                    <FlashButton view={view} value={values[view.id] ?? 0} hint={hint} onChange={(v) => setValue(view.id, v)} />
                    {bindingOf(view) && (
                      <span className="pointer-events-none absolute right-1 top-1 h-1.5 w-1.5 rounded-full bg-primary-foreground/80" title={describeTrigger(bindingOf(view)!)} />
                    )}
                  </div>
                )}
                {arranging && (
                  <div
                    className={cn("absolute -inset-1 z-10 touch-none rounded-md border border-dashed border-primary/60 bg-background/20", drag?.id === view.id ? "cursor-grabbing" : "cursor-grab")}
                    title={`Drag to move ${label(view)}`}
                    onPointerDown={(e) => startDrag(e, view)}
                    onPointerMove={(e) => moveDrag(e, view)}
                    onPointerUp={() => endDrag(view)}
                    onPointerCancel={() => endDrag(view)}
                  >
                    <label
                      className="absolute -left-1.5 -top-1.5 h-3.5 w-3.5 cursor-pointer overflow-hidden rounded-full border-2 border-foreground shadow"
                      style={{ backgroundColor: colorOf(view) ?? "hsl(var(--primary))" }}
                      title="Colour (double-click to reset)"
                      onPointerDown={(e) => e.stopPropagation()}
                      onDoubleClick={() => setColor(view, null)}
                    >
                      <input
                        type="color"
                        className="absolute inset-0 cursor-pointer opacity-0"
                        value={colorOf(view) ?? "#ffffff"}
                        onChange={(e) => setColor(view, e.target.value)}
                      />
                    </label>
                    <div
                      className="absolute -bottom-1.5 -right-1.5 h-3.5 w-3.5 cursor-nwse-resize rounded-sm border border-background bg-primary shadow"
                      title="Drag to resize"
                      onPointerDown={(e) => startResize(e, view)}
                      onPointerMove={(e) => moveResize(e, view)}
                      onPointerUp={() => endResize(view)}
                      onPointerCancel={() => endResize(view)}
                    />
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
      <div className="flex shrink-0 flex-col gap-1 border-l border-border p-1.5">
        <Button
          size="iconSm"
          variant={arranging ? "default" : "ghost"}
          title={arranging ? "Done arranging" : "Arrange: drag controls anywhere"}
          className={cn(!arranging && "text-muted-foreground", "[&_svg]:size-3.5")}
          onClick={() => setArranging((a) => !a)}
        >
          <Move />
        </Button>
        {arranging && (
          <Button size="iconSm" variant="ghost" title="Reset positions and sizes" className="text-muted-foreground [&_svg]:size-3.5" onClick={resetLayout}>
            <RotateCcw />
          </Button>
        )}
        {!popped && (
          <Button size="iconSm" variant="ghost" title="Open the controls in a new window" className="text-muted-foreground [&_svg]:size-3.5" onClick={detach}>
            <SquareArrowOutUpRight />
          </Button>
        )}
      </div>
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
