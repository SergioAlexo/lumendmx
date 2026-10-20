import { useEffect, useMemo, useRef, useState } from "react";
import { CATEGORY_COLORS } from "../../lib/graph";
import type { NodeKind, NodeTypeInfo } from "../../lib/showfile/generated";
import { cn } from "../../lib/utils";

const CATEGORY_ORDER = ["Input", "Fixtures", "Math", "Colour", "Position", "Switching", "Output"];

/** Blender's Shift+A: a searchable list of node types at the cursor. Type to
 * filter, arrows to move, Enter to add. `accepts` narrows the list (e.g. to
 * nodes that can take a link being dragged out). */
export function AddNodeMenu({
  x,
  y,
  types,
  accepts,
  onPick,
  onClose,
}: {
  x: number;
  y: number;
  types: NodeTypeInfo[];
  accepts?: (info: NodeTypeInfo) => boolean;
  onPick: (kind: NodeKind) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const ref = useRef<HTMLDivElement>(null);

  const matches = useMemo(() => {
    const q = query.trim().toLowerCase();
    return types
      .filter((t) => (accepts ? accepts(t) : true))
      .filter((t) => !q || t.label.toLowerCase().includes(q) || t.category.toLowerCase().includes(q) || t.description.toLowerCase().includes(q))
      .sort((a, b) => CATEGORY_ORDER.indexOf(a.category) - CATEGORY_ORDER.indexOf(b.category));
  }, [types, query, accepts]);

  useEffect(() => setActive(0), [query]);

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    window.addEventListener("pointerdown", onDown);
    return () => window.removeEventListener("pointerdown", onDown);
  }, [onClose]);

  // Keep the menu on screen.
  const left = Math.min(x, window.innerWidth - 260);
  const top = Math.min(y, window.innerHeight - 380);

  return (
    <div ref={ref} className="fixed z-50 flex w-64 flex-col overflow-hidden rounded-md border border-border bg-card shadow-2xl" style={{ left, top }}>
      <input
        autoFocus
        className="border-b border-border bg-transparent px-3 py-2 text-sm outline-none"
        placeholder="Search nodes…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Escape") onClose();
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setActive((a) => Math.min(matches.length - 1, a + 1));
          }
          if (e.key === "ArrowUp") {
            e.preventDefault();
            setActive((a) => Math.max(0, a - 1));
          }
          if (e.key === "Enter" && matches[active]) onPick(matches[active].kind);
        }}
      />
      <div className="max-h-80 overflow-y-auto py-1">
        {matches.length === 0 && <p className="px-3 py-2 text-xs text-muted-foreground">No matching nodes.</p>}
        {matches.map((t, i) => (
          <button
            key={t.kind}
            className={cn("flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm", i === active ? "bg-accent" : "hover:bg-accent/50")}
            onMouseEnter={() => setActive(i)}
            onClick={() => onPick(t.kind)}
            title={t.description}
          >
            <span className="h-2.5 w-2.5 shrink-0 rounded-sm" style={{ backgroundColor: CATEGORY_COLORS[t.category] }} />
            <span className="flex-1 truncate">{t.label}</span>
            <span className="text-[10px] text-muted-foreground">{t.category}</span>
          </button>
        ))}
      </div>
      {matches[active] && <p className="border-t border-border px-3 py-2 text-[11px] leading-snug text-muted-foreground">{matches[active].description}</p>}
    </div>
  );
}
