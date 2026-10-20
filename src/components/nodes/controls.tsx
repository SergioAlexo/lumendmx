import { useEffect, useRef, useState } from "react";
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
  }
}
