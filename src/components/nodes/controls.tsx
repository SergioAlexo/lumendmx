import { useEffect, useRef, useState } from "react";
import { curveValue, formatCurve, parseCurve, type CurveMode, type CurvePoint } from "../../lib/curve";
import type { Group, ParamInfo, ParamValue } from "../../lib/showfile/generated";
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

const CURVE_W = 184;
const CURVE_H = 64;
/** How near (px) a press must land to grab a point. */
const CURVE_GRAB = 7;

/** A drawn LFO wave: drag points, press empty space to add one, double- or
 * right-click a point to remove it. A whole drag commits once (one undo). */
export function CurveEditor({
  value,
  mode,
  playhead,
  onCommit,
}: {
  value: string;
  mode: CurveMode;
  /** Where in the cycle the LFO is (0-1), from its live preview. */
  playhead?: number;
  onCommit: (value: string) => void;
}) {
  const [draft, setDraft] = useState<CurvePoint[] | null>(null);
  const drag = useRef<{ index: number; moved: boolean } | null>(null);
  const points = draft ?? parseCurve(value);
  const sorted = [...points].sort((a, b) => a[0] - b[0]);

  // Pointer position in curve space (0-1), whatever the editor's zoom.
  const at = (e: React.PointerEvent | React.MouseEvent): CurvePoint => {
    const r = (e.currentTarget as SVGElement).getBoundingClientRect();
    const x = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    const y = Math.min(1, Math.max(0, 1 - (e.clientY - r.top) / r.height));
    return [x, y];
  };
  const hit = (e: React.PointerEvent | React.MouseEvent, list: CurvePoint[]) => {
    const r = (e.currentTarget as SVGElement).getBoundingClientRect();
    const scale = r.width / CURVE_W;
    return list.findIndex(
      ([x, y]) => Math.hypot(r.left + x * r.width - e.clientX, r.top + (1 - y) * r.height - e.clientY) <= CURVE_GRAB * scale,
    );
  };
  const remove = (e: React.MouseEvent) => {
    const list = parseCurve(value);
    const i = hit(e, list);
    if (i >= 0 && list.length > 1) onCommit(formatCurve(list.filter((_, j) => j !== i)));
  };

  const line = Array.from({ length: CURVE_W + 1 }, (_, i) => {
    const y = curveValue(sorted, mode, Math.min(i / CURVE_W, 1 - 1e-9));
    return `${i},${((1 - y) * CURVE_H).toFixed(1)}`;
  }).join(" L");

  return (
    <svg
      width={CURVE_W}
      height={CURVE_H}
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
      <path d={`M0,${CURVE_H} L${line} L${CURVE_W},${CURVE_H} Z`} className="fill-primary/15" />
      <path d={`M${line}`} className="fill-none stroke-primary" strokeWidth={1.5} />
      {playhead !== undefined && (
        <line x1={playhead * CURVE_W} x2={playhead * CURVE_W} y1={0} y2={CURVE_H} className="stroke-foreground/50" strokeWidth={1} />
      )}
      {points.map(([x, y], i) => (
        <circle key={i} cx={x * CURVE_W} cy={(1 - y) * CURVE_H} r={3.5} className="fill-background stroke-primary" strokeWidth={1.5} />
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
    case "color":
      return <ColorField value={String(v)} onCommit={onCommit} />;
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
