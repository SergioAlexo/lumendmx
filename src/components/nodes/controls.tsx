import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ChevronDown, CircleHelp, Trash2 } from "lucide-react";
import { curveValue, formatCurve, parseCurve, type CurveMode, type CurvePoint } from "../../lib/curve";
import type { Group, ParamInfo, ParamValue } from "../../lib/showfile/generated";
import { BEAT_PRESETS, parseLength, rateLabel } from "../../lib/beatRate";
import { CONSTANTS, FUNCTIONS, OPERATORS } from "../../lib/expressionHelp";
import { fanOffsets, type FanSettings } from "../../lib/waves";
import { WAVE_H, WAVE_W, WaveDots, WaveGrid, waveValue, waveY } from "./WavePreview";
import { colorAt, formatStops, parseStops, stopsCss, type Stop } from "../../lib/gradient";
import { cn } from "../../lib/utils";

function decimals(step: number): number {
  return step >= 1 ? 0 : Math.min(3, Math.ceil(-Math.log10(step)));
}

/** A Blender-style number field: drag sideways to scrub (Shift for fine
 * steps), click to type. Commits once, on release or Enter. */
export function ScrubNumber({
  value,
  onCommit,
  min = -Infinity,
  max = Infinity,
  step = 0.01,
  label,
  className,
}: {
  value: number;
  onCommit: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  label?: string;
  className?: string;
}) {
  const [draft, setDraft] = useState<number | null>(null);
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState("");
  const drag = useRef<{ x: number; start: number; moved: boolean } | null>(null);
  const shown = draft ?? value;
  const clamp = (v: number) => Math.min(max, Math.max(min, v));
  const round = (v: number) => Number(v.toFixed(decimals(step)));

  if (editing) {
    return (
      <input
        autoFocus
        className={cn("nodrag h-5 w-full rounded bg-background px-1 text-[11px] outline-none ring-1 ring-primary", className)}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => {
          setEditing(false);
          const v = Number(text);
          if (text.trim() !== "" && Number.isFinite(v)) onCommit(clamp(v));
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          if (e.key === "Escape") setEditing(false);
          e.stopPropagation();
        }}
      />
    );
  }

  return (
    <div
      role="spinbutton"
      aria-label={label}
      aria-valuenow={shown}
      className={cn(
        "nodrag flex h-5 w-full cursor-ew-resize select-none items-center justify-between gap-1 rounded bg-muted px-1.5 text-[11px] hover:bg-muted/70",
        className,
      )}
      onPointerDown={(e) => {
        (e.target as HTMLElement).setPointerCapture(e.pointerId);
        drag.current = { x: e.clientX, start: value, moved: false };
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d) return;
        const dx = e.clientX - d.x;
        if (Math.abs(dx) > 2) d.moved = true;
        if (d.moved) setDraft(clamp(round(d.start + Math.round(dx / 4) * step * (e.shiftKey ? 0.1 : 1))));
      }}
      onPointerUp={() => {
        const d = drag.current;
        drag.current = null;
        if (!d) return;
        if (d.moved && draft !== null) {
          onCommit(draft);
        } else if (!d.moved) {
          setText(String(value));
          setEditing(true);
        }
        setDraft(null);
      }}
    >
      {label && <span className="truncate text-muted-foreground">{label}</span>}
      <span className="tabular-nums">{shown.toFixed(decimals(step))}</span>
    </div>
  );
}

/** A text field that commits on blur or Enter. */
function CommitText({ value, onCommit, placeholder }: { value: string; onCommit: (v: string) => void; placeholder?: string }) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return (
    <input
      className="nodrag h-5 w-full min-w-0 rounded bg-muted px-1.5 text-[11px] outline-none focus:ring-1 focus:ring-primary"
      value={draft}
      placeholder={placeholder}
      title={placeholder}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => draft !== value && onCommit(draft)}
      onKeyDown={(e) => {
        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        e.stopPropagation();
      }}
    />
  );
}

export function ColorField({ value, onCommit }: { value: string; onCommit: (v: string) => void }) {
  return (
    <input
      type="color"
      className="nodrag h-5 w-full cursor-pointer rounded border-0 bg-transparent p-0"
      value={/^#[0-9a-f]{6}$/i.test(value) ? value : "#000000"}
      onChange={(e) => onCommit(e.target.value)}
    />
  );
}

/** How near (px) a press must land to grab a point. */
const CURVE_GRAB = 7;

/** A drawn LFO wave: drag points, press empty space to add one, double- or
 * right-click a point to remove it. A whole drag commits once (one undo). */
export function CurveEditor({
  value,
  mode,
  playhead,
  fan,
  count = 8,
  onCommit,
}: {
  value: string;
  mode: CurveMode;
  /** Where in the cycle the LFO is (0-1), from its live preview. */
  playhead?: number;
  /** The Fan feeding the LFO, for a dot per fixture. */
  fan?: FanSettings;
  count?: number;
  onCommit: (value: string) => void;
}) {
  const [draft, setDraft] = useState<CurvePoint[] | null>(null);
  const drag = useRef<{ index: number; moved: boolean } | null>(null);
  const points = draft ?? parseCurve(value);
  const sorted = [...points].sort((a, b) => a[0] - b[0]);
  const offsets = fan ? fanOffsets(fan, count) : [0];

  // Pointer position in curve space (0-1), whatever the editor's zoom.
  const at = (e: React.PointerEvent | React.MouseEvent): CurvePoint => {
    const r = (e.currentTarget as SVGElement).getBoundingClientRect();
    const x = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    const y = Math.min(1, Math.max(0, waveValue(((e.clientY - r.top) / r.height) * WAVE_H)));
    return [x, y];
  };
  const hit = (e: React.PointerEvent | React.MouseEvent, list: CurvePoint[]) => {
    const r = (e.currentTarget as SVGElement).getBoundingClientRect();
    const scale = r.width / WAVE_W;
    return list.findIndex(([x, y]) => Math.hypot(r.left + x * r.width - e.clientX, r.top + (waveY(y) / WAVE_H) * r.height - e.clientY) <= CURVE_GRAB * scale);
  };
  const remove = (e: React.MouseEvent) => {
    const list = parseCurve(value);
    const i = hit(e, list);
    if (i >= 0 && list.length > 1) onCommit(formatCurve(list.filter((_, j) => j !== i)));
  };

  const line = Array.from({ length: WAVE_W + 1 }, (_, i) => `${i},${waveY(curveValue(sorted, mode, Math.min(i / WAVE_W, 1 - 1e-9))).toFixed(1)}`).join(" L");

  return (
    <svg
      width={WAVE_W}
      height={WAVE_H}
      className="nodrag nowheel cursor-crosshair rounded bg-muted"
      aria-label="Curve: drag points, click to add, double-click to remove"
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        const list = parseCurve(value);
        const i = hit(e, list);
        if (i >= 0) {
          drag.current = { index: i, moved: false };
          setDraft(list);
        } else {
          drag.current = { index: list.length, moved: true };
          setDraft([...list, at(e)]);
        }
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d || !draft) return;
        d.moved = true;
        setDraft(draft.map((p, j) => (j === d.index ? at(e) : p)));
      }}
      onPointerUp={() => {
        const d = drag.current;
        drag.current = null;
        if (d?.moved && draft) onCommit(formatCurve(draft));
        setDraft(null);
      }}
      onDoubleClick={remove}
      onContextMenu={(e) => {
        e.preventDefault();
        remove(e);
      }}
    >
      <WaveGrid />
      <path d={`M${line}`} fill="none" stroke="#a1a1a1" strokeWidth={1.5} />
      <WaveDots playhead={playhead} offsets={offsets} valueAt={(p) => curveValue(sorted, mode, Math.min(p, 1 - 1e-9))} />
      {points.map(([x, y], i) => (
        <circle key={i} cx={x * WAVE_W} cy={waveY(y)} r={3} className="fill-muted" stroke="#a1a1a1" strokeWidth={1.5} />
      ))}
    </svg>
  );
}

/** The editor for one node param. */
export function ParamControl({
  info,
  value,
  groups,
  onCommit,
}: {
  info: ParamInfo;
  value: ParamValue | undefined;
  groups: Group[];
  onCommit: (value: ParamValue) => void;
}) {
  const v = value ?? info.default;
  switch (info.kind) {
    case "number":
      return <ScrubNumber label={info.label} value={Number(v)} min={info.min} max={info.max} step={info.step} onCommit={onCommit} />;
    case "bool":
      return (
        <label className="nodrag flex h-5 items-center gap-1.5 text-[11px] text-muted-foreground">
          <input type="checkbox" checked={Boolean(v)} onChange={(e) => onCommit(e.target.checked)} />
          {info.label}
        </label>
      );
    case "choice":
      return (
        <select
          aria-label={info.label}
          title={info.label}
          className="nodrag nowheel h-5 w-full rounded bg-muted px-1 text-[11px] outline-none"
          value={String(v)}
          onChange={(e) => onCommit(e.target.value)}
        >
          {info.options.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>
      );
    case "group":
      return (
        <select
          aria-label={info.label}
          className="nodrag nowheel h-5 w-full rounded bg-muted px-1 text-[11px] outline-none"
          value={Number(v)}
          onChange={(e) => onCommit(Number(e.target.value))}
        >
          <option value={0}>No group (fixture list)</option>
          {groups.map((g) => (
            <option key={g.id} value={g.id}>
              {g.name} ({g.fixtureNumbers.length})
            </option>
          ))}
        </select>
      );
    case "text":
      return <CommitText value={String(v)} placeholder={info.hint} onCommit={onCommit} />;
    case "curve":
      // Drawn by `CurveEditor`, which needs the node's mode and playhead; see GraphNode.
      return <CommitText value={String(v)} placeholder="x:y, x:y" onCommit={onCommit} />;
  }
}

/** A Gradient's colours, as in any paint program: the gradient with a handle
 * for every stop. Drag a handle along it, click the bar to add a stop there,
 * pick the selected stop's colour below. A drag commits once, on release. */
export function GradientEditor({
  value,
  wrap,
  position,
  onCommit,
}: {
  value: string;
  wrap: boolean;
  /** Where on the gradient it is picking right now (0-1), for the line on the bar. */
  position?: number;
  onCommit: (value: string) => void;
}) {
  const [draft, setDraft] = useState<Stop[] | null>(null);
  const [selected, setSelected] = useState(0);
  const bar = useRef<HTMLDivElement>(null);
  const drag = useRef<{ index: number; moved: boolean } | null>(null);
  const stops = draft ?? parseStops(value, wrap);
  const current = stops[Math.min(selected, stops.length - 1)];
  const at = (e: React.PointerEvent) => {
    const r = bar.current!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
  };
  const commit = (next: Stop[], pick?: Stop) => {
    const sorted = [...next].sort((a, b) => a.place - b.place);
    onCommit(formatStops(sorted));
    if (pick) setSelected(sorted.indexOf(pick));
  };
  const change = (patch: Partial<Stop>) => {
    if (!current) return;
    const edited = { ...current, ...patch };
    commit(stops.map((s) => (s === current ? edited : s)), edited);
  };

  return (
    <div className="flex flex-col gap-1.5">
      <div
        ref={bar}
        className="nodrag nowheel relative mb-3.5 mt-2 h-6 cursor-copy rounded border border-black/40"
        style={{ background: stopsCss(stops, wrap) }}
        title="Click to add a colour stop"
        onPointerDown={(e) => {
          if (e.button !== 0 || e.target !== e.currentTarget) return;
          const place = at(e);
          const added = { color: colorAt(stops, wrap, place), place };
          commit([...stops, added], added);
        }}
      >
        {/* Where it is picking now: a line through the bar with a point above it. */}
        {position !== undefined && (
          <div className="pointer-events-none absolute -top-2 bottom-0 w-0 -translate-x-1/2" style={{ left: `${Math.min(1, Math.max(0, position)) * 100}%` }}>
            <div className="absolute left-1/2 top-0 h-0 w-0 -translate-x-1/2 border-x-[4px] border-t-[6px] border-x-transparent border-t-white drop-shadow" />
            <div className="absolute bottom-0 left-1/2 top-1.5 w-px -translate-x-1/2 bg-white shadow-[0_0_0_1px_rgba(0,0,0,0.55)]" />
          </div>
        )}
        {stops.map((s, i) => (
          <div
            key={i}
            className="absolute top-full -translate-x-1/2 cursor-ew-resize touch-none"
            style={{ left: `${s.place * 100}%` }}
            title="Drag to move"
            onPointerDown={(e) => {
              if (e.button !== 0) return;
              e.stopPropagation();
              e.currentTarget.setPointerCapture(e.pointerId);
              drag.current = { index: i, moved: false };
              setSelected(i);
              setDraft(stops);
            }}
            onPointerMove={(e) => {
              const d = drag.current;
              if (!d) return;
              d.moved = true;
              const place = at(e);
              setDraft((list) => list && list.map((x, j) => (j === d.index ? { ...x, place } : x)));
            }}
            onPointerUp={() => {
              const d = drag.current;
              drag.current = null;
              if (d?.moved && draft) {
                const moved = draft[d.index];
                commit(draft, moved);
              }
              setDraft(null);
            }}
          >
            {/* A handle: a point up at the bar, filled with the stop's colour. */}
            <div className="h-0 w-0 border-x-[5px] border-b-[6px] border-x-transparent border-b-white/80" />
            <div
              className={cn("h-3 w-3 -translate-x-[1px] rounded-sm border", i === selected ? "border-primary ring-1 ring-primary" : "border-white/70")}
              style={{ backgroundColor: s.color, marginLeft: -1 }}
            />
          </div>
        ))}
      </div>
      {current && (
        <>
          <div className="flex items-center gap-1.5">
            <div className="w-10 shrink-0">
              <StopColor color={current.color} onCommit={(color) => change({ color })} />
            </div>
            <ScrubNumber
              className="min-w-0 flex-1"
              label="Place %"
              value={Math.round(current.place * 100)}
              min={0}
              max={100}
              step={1}
              onCommit={(percent) => change({ place: percent / 100 })}
            />
            <button
              className="nodrag flex h-5 w-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-muted/60 hover:text-foreground disabled:opacity-30"
              title="Remove this stop"
              disabled={stops.length < 2}
              onClick={() => commit(stops.filter((s) => s !== current), stops[Math.max(0, selected - 1)])}
            >
              <Trash2 className="h-3 w-3" />
            </button>
          </div>
        </>
      )}
    </div>
  );
}

/** A stop's colour picker. Follows the picker locally and commits once it
 * rests, so dragging through colours is a single undo step. */
function StopColor({ color, onCommit }: { color: string; onCommit: (color: string) => void }) {
  const [draft, setDraft] = useState(color);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => setDraft(color), [color]);
  useEffect(() => () => clearTimeout(timer.current), []);
  return (
    <input
      type="color"
      className="nodrag h-5 w-full cursor-pointer rounded border border-black/40 bg-transparent p-0"
      value={draft}
      onChange={(e) => {
        const next = e.target.value;
        setDraft(next);
        clearTimeout(timer.current);
        timer.current = setTimeout(() => onCommit(next), 250);
      }}
    />
  );
}

/** A rate per beat as a musical length: a dropdown of 1 beat, 1/2 beat, 1/4
 * beat... that also takes anything typed ("1/3", "2.5", "2 bars"). */
export function BeatRateField({ rate, onCommit }: { rate: number; onCommit: (rate: number) => void }) {
  const [text, setText] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const shown = text ?? rateLabel(rate);

  const finish = () => {
    if (text !== null) {
      const typed = parseLength(text);
      if (typed !== undefined && typed !== rate) onCommit(typed);
    }
    setText(null);
    setOpen(false);
  };
  const rect = open ? box.current?.getBoundingClientRect() : undefined;

  return (
    <div ref={box} className="nodrag relative flex h-5 items-center rounded bg-muted text-[11px]">
      <span className="pl-1.5 pr-1 text-muted-foreground">Rate</span>
      <input
        className="nowheel h-full min-w-0 flex-1 bg-transparent text-right tabular-nums outline-none"
        value={shown}
        spellCheck={false}
        title="Length of one cycle in beats. Pick one or type your own: 1/3, 2.5, 2 bars"
        onFocus={(e) => e.target.select()}
        onChange={(e) => setText(e.target.value)}
        onBlur={finish}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          if (e.key === "Escape") {
            setText(null);
            setOpen(false);
            (e.target as HTMLInputElement).blur();
          }
          e.stopPropagation();
        }}
      />
      <button
        className="flex h-full w-5 shrink-0 items-center justify-center text-muted-foreground hover:text-foreground"
        tabIndex={-1}
        onMouseDown={(e) => {
          e.preventDefault();
          setOpen((o) => !o);
        }}
        title="Common lengths"
      >
        <ChevronDown className="h-3 w-3" />
      </button>
      {open &&
        rect &&
        createPortal(
          <div
            className="fixed z-50 max-h-64 overflow-y-auto rounded-md border border-border bg-card py-1 text-[11px] shadow-xl"
            style={{ left: rect.left, top: rect.bottom + 2, width: Math.max(rect.width, 130) }}
          >
            {BEAT_PRESETS.map((p) => (
              <button
                key={p.label}
                className={cn("block w-full px-2 py-1 text-left hover:bg-accent", rateLabel(rate) === p.label && "bg-accent/60")}
                onMouseDown={(e) => {
                  e.preventDefault();
                  onCommit(1 / p.length);
                  setText(null);
                  setOpen(false);
                }}
              >
                {p.label}
              </button>
            ))}
          </div>,
          document.body,
        )}
    </div>
  );
}

/** What a formula can use, folded away under the formula field. */
export function ExpressionHelp() {
  const [open, setOpen] = useState(false);
  return (
    <div className="text-[10px] leading-snug text-muted-foreground">
      <button className="nodrag flex items-center gap-1 hover:text-foreground" onClick={() => setOpen(!open)}>
        <CircleHelp className="h-3 w-3" />
        {open ? "Hide help" : "What can I write?"}
      </button>
      {open && (
        <div className="nodrag nowheel mt-1 flex max-h-56 select-text flex-col gap-1.5 overflow-y-auto rounded bg-muted/60 p-1.5">
          <p>
            <span className="text-foreground">Inputs:</span> any other name you write becomes an input on the node, in the order it first appears.
            In <code>sin(t * 2) * depth</code> that is <code>t</code> and <code>depth</code>. Unconnected, it uses the number typed beside it.
          </p>
          <div>
            <span className="text-foreground">Operators</span>
            {OPERATORS.map((o) => (
              <div key={o.symbol} className="flex gap-1.5">
                <code className="w-14 shrink-0 text-foreground">{o.symbol}</code>
                <span>{o.meaning}</span>
              </div>
            ))}
          </div>
          <div>
            <span className="text-foreground">Constants</span>
            {CONSTANTS.map((c) => (
              <div key={c.name} className="flex gap-1.5">
                <code className="w-14 shrink-0 text-foreground">{c.name}</code>
                <span>{c.meaning}</span>
              </div>
            ))}
          </div>
          <div>
            <span className="text-foreground">Functions</span>
            {FUNCTIONS.map((f) => (
              <div key={f.signature} className="flex flex-col">
                <code className="text-foreground">{f.signature}</code>
                <span className="pl-2">{f.meaning}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
