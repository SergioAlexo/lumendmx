import { useEffect, useMemo, useRef, useState } from "react";
import { X } from "lucide-react";
import { CATEGORY_COLORS } from "../../lib/graph";
import type { NodeKind, NodePreset, NodeTypeInfo } from "../../lib/showfile/generated";
import { cn } from "../../lib/utils";

const CATEGORY_ORDER = ["Input", "Fixtures", "Math", "Colour", "Position", "Switching", "Output", "Layout"];

type Item = { type: "node"; info: NodeTypeInfo } | { type: "preset"; preset: NodePreset };

/** Blender's Shift+A: a searchable list of saved presets and node types at
 * the cursor. Type to filter, arrows to move, Enter to add. `accepts`
 * narrows the list (e.g. to nodes that can take a link being dragged out)
 * and hides the presets. */
export function AddNodeMenu({
  x,
  y,
  types,
  presets,
  accepts,
  onPick,
  onPickPreset,
  onDeletePreset,
  onClose,
}: {
  x: number;
  y: number;
  types: NodeTypeInfo[];
  presets: NodePreset[];
  accepts?: (info: NodeTypeInfo) => boolean;
  onPick: (kind: NodeKind) => void;
  onPickPreset: (preset: NodePreset) => void;
  onDeletePreset: (preset: NodePreset) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  /** A preset whose delete button was clicked once; the second click deletes. */
  const [confirming, setConfirming] = useState<string | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  const matches = useMemo((): Item[] => {
    const q = query.trim().toLowerCase();
    const saved: Item[] = accepts
      ? []
      : presets.filter((p) => !q || p.name.toLowerCase().includes(q) || "preset".includes(q)).map((preset) => ({ type: "preset", preset }));
    const nodes: Item[] = types
      .filter((t) => (accepts ? accepts(t) : true))
      .filter((t) => !q || t.label.toLowerCase().includes(q) || t.category.toLowerCase().includes(q) || t.description.toLowerCase().includes(q))
      .sort((a, b) => CATEGORY_ORDER.indexOf(a.category) - CATEGORY_ORDER.indexOf(b.category))
      .map((info) => ({ type: "node", info }));
    return [...saved, ...nodes];
  }, [types, presets, query, accepts]);

  const pick = (item: Item) => (item.type === "node" ? onPick(item.info.kind) : onPickPreset(item.preset));
  const current = matches[active];

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
        placeholder="Search nodes and presets…"
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
          if (e.key === "Enter" && current) pick(current);
        }}
      />
      <div className="max-h-80 overflow-y-auto py-1">
        {matches.length === 0 && <p className="px-3 py-2 text-xs text-muted-foreground">No matching nodes.</p>}
        {matches.map((item, i) =>
          item.type === "node" ? (
            <button
              key={item.info.kind}
              className={cn("flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm", i === active ? "bg-accent" : "hover:bg-accent/50")}
              onMouseEnter={() => setActive(i)}
              onClick={() => pick(item)}
              title={item.info.description}
            >
              <span className="h-2.5 w-2.5 shrink-0 rounded-sm" style={{ backgroundColor: CATEGORY_COLORS[item.info.category] }} />
              <span className="flex-1 truncate">{item.info.label}</span>
              <span className="text-[10px] text-muted-foreground">{item.info.category}</span>
            </button>
          ) : (
            <div
              key={item.preset.id}
              role="button"
              className={cn("group flex w-full cursor-pointer items-center gap-2 px-3 py-1.5 text-left text-sm", i === active ? "bg-accent" : "hover:bg-accent/50")}
              onMouseEnter={() => setActive(i)}
              onClick={() => pick(item)}
            >
              <span className="h-2.5 w-2.5 shrink-0 rounded-full border border-foreground/60" />
              <span className="flex-1 truncate">{item.preset.name}</span>
              {confirming === item.preset.id ? (
                <button
                  className="text-[11px] text-destructive hover:underline"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDeletePreset(item.preset);
                  }}
                >
                  Delete?
                </button>
              ) : (
                <>
                  <span className="text-[10px] text-muted-foreground group-hover:hidden">Preset</span>
                  <button
                    className="hidden text-muted-foreground hover:text-destructive group-hover:block"
                    title="Delete this preset (from every show)"
                    onClick={(e) => {
                      e.stopPropagation();
                      setConfirming(item.preset.id);
                    }}
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                </>
              )}
            </div>
          ),
        )}
      </div>
      {current && (
        <p className="border-t border-border px-3 py-2 text-[11px] leading-snug text-muted-foreground">
          {current.type === "node"
            ? current.info.description
            : `${current.preset.nodes.length} node${current.preset.nodes.length === 1 ? "" : "s"}: ${current.preset.nodes
                .map((n) => types.find((t) => t.kind === n.kind)?.label ?? n.kind)
                .join(", ")}`}
        </p>
      )}
    </div>
  );
}
