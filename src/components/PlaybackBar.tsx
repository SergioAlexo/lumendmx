import { ChevronDown, ChevronRight, FolderPlus, Move, RotateCcw, SquareArrowOutUpRight, Undo2, X, Zap } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import * as graphApi from "../lib/graph";
import { describeTrigger, isPress } from "../lib/midiLearn";
import * as timing from "../lib/timing";
import type { MidiTrigger, NodeView } from "../lib/showfile/generated";
import { GROUP_HEADER_H, GROUP_PAD, groupAt, groupBoxes, groupName, type Rect } from "../lib/stripGroups";
import { cn } from "../lib/utils";
import { popOut, poppedOutWindow } from "../lib/windows";
import { useGraphStore } from "../store/useGraphStore";
import { useShowStore } from "../store/useShowStore";
import { MidiBindingMenu, SlotTabs, TravelField } from "./MidiBindingMenu";
import { Button } from "./ui/button";
import { VerticalFader } from "./ui/vertical-fader";

// Fast enough that a fader swept by MIDI looks like it glides.
const POLL_MS = 50;
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

/** Each control's place: where it was put, else the first free spot along a bottom-aligned row,
 * so a new control never lands on top of one that's already there. */
function layout(controls: NodeView[]): Map<string, Spot> {
  const spots = new Map<string, Spot>();
  const taken: { x: number; y: number; w: number; h: number }[] = [];
  const unplaced: NodeView[] = [];
  for (const view of controls) {
    const { panelX, panelY } = view.params;
    if (typeof panelX === "number" && typeof panelY === "number") {
      spots.set(view.id, { x: panelX, y: panelY });
      taken.push({ x: panelX, y: panelY, ...sizeOf(view) });
    } else unplaced.push(view);
  }
  const free = (x: number, y: number, w: number, h: number) =>
    taken.every((t) => x + w + GRID <= t.x || t.x + t.w + GRID <= x || y + h + GRID <= t.y || t.y + t.h + GRID <= y);
  for (const view of unplaced) {
    const right = taken.reduce((r, t) => Math.max(r, t.x + t.w), 0);
    const { w, h } = sizeOf(view);
    const y = GAP + ROW_H - h;
    let x = GAP;
    // Past the right edge of everything is always free, so this stops.
    while (x <= right && !free(x, y, w, h)) x += GRID;
    spots.set(view.id, { x, y });
    taken.push({ x, y, w, h });
  }
  return spots;
}

const snap = (v: number) => Math.max(0, Math.round(v / GRID) * GRID);
const snapDelta = (v: number) => Math.round(v / GRID) * GRID;

const FOLDED_KEY = "lumendmx-strip-folded";

/** The groups folded shut (by name), remembered per machine. */
function useFoldedGroups(): [Set<string>, (name: string) => void] {
  const [folded, setFolded] = useState<Set<string>>(() => {
    try {
      return new Set(JSON.parse(localStorage.getItem(FOLDED_KEY) ?? "[]") as string[]);
    } catch {
      return new Set();
    }
  });
  const toggle = (name: string) => {
    const next = new Set(folded);
    if (!next.delete(name)) next.add(name);
    setFolded(next);
    try {
      localStorage.setItem(FOLDED_KEY, JSON.stringify([...next]));
    } catch {
      // Not remembered; still folds for this session.
    }
  };
  return [folded, toggle];
}

/** What a control's MIDI binding does: its own, or (Faders) a button that steps it up or down. */
type Slot = "main" | "up" | "down" | "sweep";

const SLOT_PARAMS: Record<Slot, { mode: string; channel: string; number: string }> = {
  main: { mode: "midi", channel: "midiChannel", number: "midiNumber" },
  up: { mode: "upMidi", channel: "upChannel", number: "upNumber" },
  down: { mode: "downMidi", channel: "downChannel", number: "downNumber" },
  sweep: { mode: "sweepMidi", channel: "sweepChannel", number: "sweepNumber" },
};

const SLOT_LABELS: Record<Slot, string> = { main: "Knob", up: "Up", down: "Down", sweep: "Sweep" };

/** A Button's or Fader's MIDI binding in `slot`, from its params. */
function bindingOf(view: NodeView, slot: Slot = "main"): MidiTrigger | null {
  const p = SLOT_PARAMS[slot];
  const message = view.params[p.mode];
  if (message !== "note" && message !== "cc") return null;
  return { message, channel: Number(view.params[p.channel] ?? 0), number: Number(view.params[p.number] ?? 0) };
}

/** Whether any of a control's bindings is set, and what they are, for the tooltip. */
function bindingsOf(view: NodeView): string[] {
  const names: Record<Slot, string> = { main: "", up: "up ", down: "down ", sweep: "sweep " };
  return (["main", "up", "down", "sweep"] as Slot[]).flatMap((slot) => {
    const b = bindingOf(view, slot);
    return b ? [`${names[slot]}${describeTrigger(b)}`] : [];
  });
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
  const [menu, setMenu] = useState<{ view: NodeView; x: number; y: number; noPorts: boolean; slot: Slot } | null>(null);
  const [arranging, setArranging] = useState(false);
  const [drag, setDrag] = useState<(Spot & { id: string }) | null>(null);
  const grab = useRef<{ x: number; y: number; from: Spot } | null>(null);
  const [resize, setResize] = useState<{ id: string; w: number; h: number } | null>(null);
  const sizing = useRef<{ x: number; y: number; from: { w: number; h: number } } | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [naming, setNaming] = useState<{ rename: string | null } | null>(null);
  const [groupDrag, setGroupDrag] = useState<{ name: string; dx: number; dy: number } | null>(null);
  const groupGrab = useRef<{ x: number; y: number; moved: boolean } | null>(null);
  const [folded, toggleFolded] = useFoldedGroups();
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
    setMenu({ ...at, noPorts: false, slot: "main" });
    void timing
      .timingStatus()
      .then((s) => setMenu((m) => (m && m.view.id === view.id ? { ...m, noPorts: s.settings.controllerPorts.length === 0 } : m)))
      .catch(() => undefined);
  };

  const bind = (view: NodeView, slot: Slot, b: MidiTrigger | null) => {
    const p = SLOT_PARAMS[slot];
    void run(() => graphApi.setParams(view.id, b ? { [p.mode]: b.message, [p.channel]: b.channel, [p.number]: b.number } : { [p.mode]: "none" }));
  };
  const controls = (nodes ?? [])
    .filter((n) => n.kind === "fader" || n.kind === "button")
    .sort((a, b) => a.position.x - b.position.x || a.position.y - b.position.y);
  const spots = layout(controls);

  /** Writes new places, and group changes by control id. Controls with no place yet get the one
   * they show now, so they stay put. */
  const commit = (moves: Map<string, Spot>, groups: Record<string, string | null> = {}) =>
    void run(async () => {
      for (const view of controls) {
        const placed = typeof view.params.panelX === "number" && typeof view.params.panelY === "number";
        const spot = moves.get(view.id) ?? (placed ? null : spots.get(view.id)!);
        const changes: graphApi.ParamChanges = {};
        if (spot) Object.assign(changes, { panelX: spot.x, panelY: spot.y });
        if (view.id in groups) changes.panelGroup = groups[view.id];
        if (Object.keys(changes).length > 0) await graphApi.setParams(view.id, changes);
      }
    });
  const setGroup = (ids: string[], name: string | null) =>
    void run(async () => {
      for (const id of ids) await graphApi.setParams(id, { panelGroup: name });
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

  /** Where a control rests, with its group's move in progress applied. */
  const restAt = (view: NodeView): Spot => {
    const spot = spots.get(view.id)!;
    return groupDrag && groupName(view.params) === groupDrag.name ? { x: spot.x + groupDrag.dx, y: spot.y + groupDrag.dy } : spot;
  };
  const at = (view: NodeView): Spot => (drag?.id === view.id ? drag : restAt(view));
  const sizeAt = (view: NodeView) => (resize?.id === view.id ? resize : sizeOf(view));
  const rectOf = (view: NodeView, where: (v: NodeView) => Spot): Rect => ({ ...where(view), ...sizeAt(view) });
  const boxesOf = (where: (v: NodeView) => Spot) =>
    groupBoxes(
      controls.map((v) => ({ name: groupName(v.params), rect: rectOf(v, where) })),
      folded,
    );
  const boxes = boxesOf(at);
  /** Members of folded groups are hidden. */
  const shown = controls.filter((v) => !folded.has(groupName(v.params) ?? ""));
  // Blocks reach above and left of their members, so the strip is shifted to keep them in view.
  // The shift ignores moves in progress, or the strip would slide under the pointer.
  const resting = [...boxesOf((v) => spots.get(v.id)!).values(), ...shown.map((v) => rectOf(v, (c) => spots.get(c.id)!))];
  const [ox, oy] = [-Math.min(0, ...resting.map((r) => r.x)), -Math.min(0, ...resting.map((r) => r.y))];
  const live = [...boxes.values(), ...shown.map((v) => rectOf(v, at))];
  const extent = { w: Math.max(0, ...live.map((r) => r.x + r.w)) + ox, h: Math.max(0, ...live.map((r) => r.y + r.h)) + oy };

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
  /** A drag that didn't move is a click: it picks the control for grouping. Otherwise the control
   * lands where it was dropped, joining the block it was dropped on, or leaving its own if dropped
   * well clear of it. */
  const endDrag = (view: NodeView) => {
    const g = grab.current;
    const dropped = drag;
    grab.current = null;
    setDrag(null);
    if (!g || !dropped) return;
    if (dropped.x === g.from.x && dropped.y === g.from.y) {
      setSelected((sel) => {
        const next = new Set(sel);
        if (!next.delete(view.id)) next.add(view.id);
        return next;
      });
      return;
    }
    const rect = { x: dropped.x, y: dropped.y, ...sizeOf(view) };
    const current = groupName(view.params);
    const rested = boxesOf(restAt);
    const others = new Map([...rested].filter(([name]) => name !== current && !folded.has(name)));
    let target = groupAt(others, rect);
    const own = current ? rested.get(current) : undefined;
    if (target === null && current && own) {
      // Some slack, so a control can be pulled outwards to widen its own block.
      const slack = 2 * GROUP_PAD;
      target = groupAt(new Map([[current, { x: own.x - slack, y: own.y - slack, w: own.w + 2 * slack, h: own.h + 2 * slack }]]), rect);
    }
    commit(new Map([[view.id, { x: dropped.x, y: dropped.y }]]), target !== current ? { [view.id]: target } : {});
  };

  const membersOf = (name: string) => controls.filter((c) => groupName(c.params) === name);
  const startGroupDrag = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    groupGrab.current = { x: e.clientX, y: e.clientY, moved: false };
  };
  const moveGroupDrag = (e: React.PointerEvent, name: string) => {
    const g = groupGrab.current;
    if (!g || !arranging) return;
    const [dx, dy] = [e.clientX - g.x, e.clientY - g.y];
    if (!g.moved && Math.hypot(dx, dy) < 4) return;
    g.moved = true;
    const members = membersOf(name).map((c) => spots.get(c.id)!);
    // Not past the strip's top-left edge.
    setGroupDrag({
      name,
      dx: Math.max(-Math.min(...members.map((m) => m.x)), snapDelta(dx)),
      dy: Math.max(-Math.min(...members.map((m) => m.y)), snapDelta(dy)),
    });
  };
  /** A click folds or unfolds the block; a drag moves it with everything in it. */
  const endGroupDrag = (name: string) => {
    const g = groupGrab.current;
    const d = groupDrag;
    groupGrab.current = null;
    setGroupDrag(null);
    if (!g) return;
    if (!g.moved || !d) {
      toggleFolded(name);
      return;
    }
    if (d.dx === 0 && d.dy === 0) return;
    commit(new Map(membersOf(name).map((c) => [c.id, { x: spots.get(c.id)!.x + d.dx, y: spots.get(c.id)!.y + d.dy }])));
  };
  const submitName = (value: string) => {
    const name = value.trim();
    const rename = naming?.rename ?? null;
    setNaming(null);
    if (!name) return;
    if (rename === null) {
      setGroup([...selected], name);
      setSelected(new Set());
    } else if (name !== rename) {
      setGroup(membersOf(rename).map((c) => c.id), name);
    }
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
    <div className={cn("relative flex bg-card", popped ? "h-full" : "shrink-0 border-t border-border")}>
      <div className={cn("min-w-0 flex-1 overflow-auto", !popped && "max-h-[45vh]")}>
        <div
          className={cn("relative", popped && "min-h-full min-w-full")}
          style={{ width: extent.w + GAP, height: extent.h + GAP }}
        >
          <div className="absolute" style={{ left: ox, top: oy }}>
          {[...boxes].map(([name, box]) => {
            const count = membersOf(name).length;
            const isFolded = folded.has(name);
            return (
              <div key={`group-${name}`}>
                {!isFolded && (
                  <div className="pointer-events-none absolute rounded-lg border border-dashed border-border bg-muted/20" style={{ left: box.x, top: box.y, width: box.w, height: box.h }} />
                )}
                <div
                  className={cn(
                    "absolute z-20 flex touch-none select-none items-center gap-1 px-2 text-[11px] font-medium text-muted-foreground hover:text-foreground",
                    isFolded && "rounded-lg border border-dashed border-border bg-muted/40",
                    arranging ? "cursor-grab" : "cursor-pointer",
                  )}
                  style={{ left: box.x, top: box.y, width: box.w, height: GROUP_HEADER_H }}
                  title={`${name}: click to ${isFolded ? "show" : "fold"}${arranging ? ", drag to move it with its controls, double-click to rename" : ""}`}
                  onPointerDown={startGroupDrag}
                  onPointerMove={(e) => moveGroupDrag(e, name)}
                  onPointerUp={() => endGroupDrag(name)}
                  onPointerCancel={() => endGroupDrag(name)}
                  onDoubleClick={() => arranging && setNaming({ rename: name })}
                >
                  {isFolded ? <ChevronRight className="h-3 w-3 shrink-0" /> : <ChevronDown className="h-3 w-3 shrink-0" />}
                  <span className="min-w-0 flex-1 truncate">{name}</span>
                  <span className="opacity-60">{count}</span>
                  {arranging && (
                    <button
                      className="shrink-0 rounded hover:bg-muted"
                      title="Ungroup (the controls stay)"
                      onPointerDown={(e) => e.stopPropagation()}
                      onClick={() => setGroup(membersOf(name).map((c) => c.id), null)}
                    >
                      <X className="h-3 w-3" />
                    </button>
                  )}
                </div>
              </div>
            );
          })}
          {shown.map((view) => {
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
                    {bindingsOf(view).length > 0 && <span className="absolute right-0 top-0 h-1.5 w-1.5 rounded-full bg-primary" title={bindingsOf(view).join(", ")} />}
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
                    className={cn(
                      "absolute -inset-1 z-10 touch-none rounded-md border",
                      selected.has(view.id) ? "border-2 border-primary bg-primary/25" : "border-dashed border-primary/60 bg-background/20",
                      drag?.id === view.id ? "cursor-grabbing" : "cursor-grab",
                    )}
                    title={`Drag to move ${label(view)}, click to pick it for a group`}
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
      </div>
      {naming && (
        <form
          className="absolute right-12 top-2 z-30 flex items-center gap-1 rounded-md border border-border bg-card p-1.5 shadow-lg"
          onSubmit={(e) => {
            e.preventDefault();
            submitName(String(new FormData(e.currentTarget).get("name") ?? ""));
          }}
        >
          <input
            name="name"
            autoFocus
            defaultValue={naming.rename ?? "Group"}
            maxLength={40}
            className="h-7 w-40 rounded border border-border bg-background px-2 text-xs"
            onFocus={(e) => e.currentTarget.select()}
            onKeyDown={(e) => e.key === "Escape" && setNaming(null)}
          />
          <Button size="sm" type="submit" className="h-7 px-2 text-xs">
            {naming.rename === null ? "Group" : "Rename"}
          </Button>
        </form>
      )}
      <div className="flex shrink-0 flex-col gap-1 border-l border-border p-1.5">
        <Button
          size="iconSm"
          variant={arranging ? "default" : "ghost"}
          title={arranging ? "Done arranging" : "Arrange: drag controls anywhere"}
          className={cn(!arranging && "text-muted-foreground", "[&_svg]:size-3.5")}
          onClick={() => {
            setArranging((a) => !a);
            setSelected(new Set());
            setNaming(null);
          }}
        >
          <Move />
        </Button>
        {arranging && selected.size > 0 && (
          <Button
            size="iconSm"
            variant="secondary"
            title={`Put the ${selected.size} picked control${selected.size > 1 ? "s" : ""} in a group (an existing group's name adds to it)`}
            className="[&_svg]:size-3.5"
            onClick={() => setNaming({ rename: null })}
          >
            <FolderPlus />
          </Button>
        )}
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
          key={menu.slot}
          x={menu.x}
          y={menu.y}
          title={`${label(menu.view)} · MIDI`}
          binding={bindingOf(controls.find((c) => c.id === menu.view.id) ?? menu.view, menu.slot)}
          accept={menu.view.kind === "fader" && menu.slot === "main" ? (e) => e.kind === "cc" : isPress}
          hint={menu.view.kind === "fader" && menu.slot === "main" ? "Move a knob or fader…" : "Press a pad or button…"}
          noPorts={menu.noPorts}
          header={
            menu.view.kind === "fader" && (
              <SlotTabs
                slots={SLOT_LABELS}
                value={menu.slot}
                onChange={(slot) => setMenu({ ...menu, slot })}
                title="Bind a knob, two buttons that move the fader up and down while held, or one sweep pad that moves it the opposite way each press"
              />
            )
          }
          footer={
            menu.view.kind === "fader" &&
            menu.slot !== "main" && (
              <TravelField
                seconds={Number((controls.find((c) => c.id === menu.view.id) ?? menu.view).params.travel ?? 2)}
                onCommit={(travel) => void run(() => graphApi.setParams(menu.view.id, { travel }))}
                velocity={Number((controls.find((c) => c.id === menu.view.id) ?? menu.view).params.travelVelocity ?? 0) > 0.5}
                onVelocity={(travelVelocity) => void run(() => graphApi.setParams(menu.view.id, { travelVelocity }))}
              />
            )
          }
          onBind={(b) => bind(menu.view, menu.slot, b)}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}
