import { memo, useState } from "react";
import { Handle, NodeResizer, Position, type NodeProps } from "@xyflow/react";
import { ChevronDown, ChevronRight } from "lucide-react";
import * as graphApi from "../../lib/graph";
import { SOCKET_COLORS } from "../../lib/graph";
import { cn } from "../../lib/utils";
import { useShowStore } from "../../store/useShowStore";
import type { GraphFlowNode } from "./GraphNode";
import { ExposeMenu, type ExposeTarget } from "./ExposeMenu";
import { exposedParam, exposedSockets, socketKey, type FramePort } from "./frames";

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

/** A socket on a collapsed frame. Double-click renames it; right-click opens
 * the expose menu (name, stop exposing). Shown only because a link crosses
 * into the frame: dimmed, and right-click exposes it. */
function PortRow({ port }: { port: FramePort }) {
  const run = useShowStore((s) => s.run);
  const view = useShowStore((s) => s.show?.graph.nodes.find((n) => n.id === port.node));
  const [draft, setDraft] = useState<string | null>(null);
  const [exposing, setExposing] = useState<ExposeTarget | null>(null);
  const key = socketKey(port.side, port.socket);

  const setName = (name: string | null) => {
    if (!view) return;
    const map = exposedSockets(view);
    if (name === null) map.delete(key);
    else map.set(key, name);
    void run(() => graphApi.setParams(port.node, { exposed: exposedParam(map) }));
  };
  const commit = () => {
    if (draft !== null && draft.trim() !== port.label) setName(draft.trim());
    setDraft(null);
  };

  const out = port.side === "out";
  return (
    <div
      className={cn("relative flex h-5 items-center px-3", out && "justify-end")}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
        setExposing({ node: port.node, side: port.side, socket: port.socket, x: e.clientX, y: e.clientY });
      }}
      title={
        port.exposed
          ? "Double-click to rename. Right-click to rename it or stop exposing it"
          : "Not exposed: shown because a link crosses into the frame. Right-click to expose it"
      }
    >
      <Handle
        type={out ? "source" : "target"}
        position={out ? Position.Right : Position.Left}
        id={port.handle}
        className="!h-2.5 !w-2.5 !border-black/60"
        style={{ background: SOCKET_COLORS[port.type] }}
      />
      {draft === null ? (
        <span
          className={cn("truncate", !port.exposed && "italic text-muted-foreground")}
          onDoubleClick={(e) => {
            if (!port.exposed) return;
            e.stopPropagation();
            setDraft(port.label);
          }}
        >
          {port.label}
        </span>
      ) : (
        <input
          autoFocus
          className={cn("nodrag w-full rounded bg-muted px-1 outline-none", out && "text-right")}
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
      {exposing && <ExposeMenu target={exposing} onClose={() => setExposing(null)} />}
    </div>
  );
}

/** A Frame: a labelled box drawn behind other nodes. Only its header takes
 * the pointer, so you can still box-select and pan inside it. Dragging and
 * resizing are handled by the editor. Collapsed, it's a compact node with
 * the sockets of the nodes inside (`frames.ts`). */
function FrameNodeImpl({ id, data, selected }: NodeProps<GraphFlowNode>) {
  const { view, ports = [] } = data;
  const collapsed = view.params.collapsed === true;
  const run = useShowStore((s) => s.run);
  const [draft, setDraft] = useState<string | null>(null);
  const label = String(view.params.label ?? "Frame");
  const color = FRAME_COLORS[String(view.params.color ?? "gray")] ?? FRAME_COLORS.gray;
  const setParam = (params: graphApi.ParamChanges) => void run(() => graphApi.setParams(id, params));

  const commit = () => {
    if (draft !== null && draft.trim() && draft.trim() !== label) setParam({ label: draft.trim() });
    setDraft(null);
  };

  const chevron = (
    <button
      className="nodrag shrink-0 rounded hover:bg-black/20"
      title={collapsed ? "Expand: show the nodes inside" : "Collapse into a node: hides the nodes inside and shows their sockets"}
      onClick={() => setParam({ collapsed: !collapsed })}
    >
      {collapsed ? <ChevronRight className="h-3.5 w-3.5" /> : <ChevronDown className="h-3.5 w-3.5" />}
    </button>
  );

  const title =
    draft === null ? (
      <span className="truncate" onDoubleClick={() => setDraft(label)}>
        {label}
      </span>
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
    );

  if (collapsed) {
    const inputs = ports.filter((p) => p.side === "in");
    const outputs = ports.filter((p) => p.side === "out");
    return (
      <div
        className={cn("w-56 rounded-md border-2 bg-card/95 text-[11px] shadow-lg", selected ? "ring-1 ring-primary" : "")}
        style={{ borderColor: color }}
      >
        <div className="flex h-7 items-center gap-1.5 rounded-t-sm px-2 text-xs font-medium text-white" style={{ backgroundColor: color }}>
          {chevron}
          {title}
        </div>
        <div className="flex flex-col gap-1 py-1.5">
          {ports.length === 0 && (
            <p className="px-3 text-muted-foreground">Nothing exposed. Expand the frame and right-click a socket of a node inside to expose it.</p>
          )}
          {outputs.map((p) => (
            <PortRow key={p.handle} port={p} />
          ))}
          {inputs.map((p) => (
            <PortRow key={p.handle} port={p} />
          ))}
        </div>
      </div>
    );
  }

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
          title="Drag to move the frame and the nodes inside it. Double-click the name to rename."
        >
          {chevron}
          {title}
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
