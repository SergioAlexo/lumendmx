import { memo, useState } from "react";
import { NodeResizer, type NodeProps } from "@xyflow/react";
import * as graphApi from "../../lib/graph";
import { cn } from "../../lib/utils";
import { useShowStore } from "../../store/useShowStore";
import type { GraphFlowNode } from "./GraphNode";

/** Mirrors FRAME_COLORS in src-tauri/src/graph/registry.rs. */
export const FRAME_COLORS: Record<string, string> = {
  gray: "#6b7280",
  red: "#dc2626",
  orange: "#ea580c",
  yellow: "#ca8a04",
  green: "#16a34a",
  blue: "#2563eb",
  purple: "#9333ea",
};

export const FRAME_MIN = { width: 100, height: 80 };

/** A Frame: a labelled box drawn behind other nodes. Only its header takes
 * the pointer, so you can still box-select and pan inside it. Dragging and
 * resizing are handled by the editor. */
function FrameNodeImpl({ id, data, selected }: NodeProps<GraphFlowNode>) {
  const { view } = data;
  const run = useShowStore((s) => s.run);
  const [draft, setDraft] = useState<string | null>(null);
  const label = String(view.params.label ?? "Frame");
  const color = FRAME_COLORS[String(view.params.color ?? "gray")] ?? FRAME_COLORS.gray;
  const setParam = (params: graphApi.ParamChanges) => void run(() => graphApi.setParams(id, params));

  const commit = () => {
    if (draft !== null && draft.trim() && draft.trim() !== label) setParam({ label: draft.trim() });
    setDraft(null);
  };

  return (
    <>
      <NodeResizer
        isVisible={selected}
        minWidth={FRAME_MIN.width}
        minHeight={FRAME_MIN.height}
        color={color}
        handleClassName="!pointer-events-auto"
        lineClassName="!pointer-events-auto"
        onResizeEnd={(_e, r) =>
          void run(async () => {
            // Dragged from the top or left: the corner moved too.
            if (r.x !== view.position.x || r.y !== view.position.y) await graphApi.moveNodes([{ id, position: { x: r.x, y: r.y } }]);
            await graphApi.setParams(id, { width: Math.round(r.width), height: Math.round(r.height) });
          })
        }
      />
      <div
        className="flex h-full w-full flex-col rounded-md border-2"
        style={{ borderColor: `${color}99`, backgroundColor: `${color}14` }}
      >
        <div
          className="frame-handle pointer-events-auto flex h-7 shrink-0 cursor-grab items-center gap-2 rounded-t px-2 text-xs font-medium text-white"
          style={{ backgroundColor: `${color}cc` }}
          onDoubleClick={() => setDraft(label)}
          title="Drag to move the frame and the nodes inside it. Double-click to rename."
        >
          {draft === null ? (
            <span className="truncate">{label}</span>
          ) : (
            <input
              autoFocus
              className="nodrag min-w-0 flex-1 rounded bg-black/30 px-1 text-xs text-white outline-none"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onFocus={(e) => e.currentTarget.select()}
              onBlur={commit}
              onKeyDown={(e) => {
                if (e.key === "Enter") e.currentTarget.blur();
                if (e.key === "Escape") setDraft(null);
              }}
            />
          )}
          {selected && draft === null && (
            <span className="nodrag ml-auto flex shrink-0 gap-1">
              {Object.entries(FRAME_COLORS).map(([name, c]) => (
                <button
                  key={name}
                  className={cn("h-3 w-3 rounded-full border", c === color ? "border-white" : "border-black/40")}
                  style={{ backgroundColor: c }}
                  title={name}
                  onClick={() => setParam({ color: name })}
                />
              ))}
            </span>
          )}
        </div>
      </div>
    </>
  );
}

export const FrameNode = memo(FrameNodeImpl);
