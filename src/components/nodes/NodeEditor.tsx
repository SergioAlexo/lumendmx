import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  applyNodeChanges,
  useReactFlow,
  type Connection,
  type Edge as FlowEdge,
  type FinalConnectionState,
  type IsValidConnection,
  type NodeChange,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Bookmark, Plus } from "lucide-react";
import * as graphApi from "../../lib/graph";
import { CATEGORY_COLORS, SOCKET_COLORS, feeds } from "../../lib/graph";
import type { NodeKind, NodePreset, NodeTypeInfo, NodeView, SocketType } from "../../lib/showfile/generated";
import { useGraphStore } from "../../store/useGraphStore";
import { useShowStore } from "../../store/useShowStore";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { AddNodeMenu } from "./AddNodeMenu";
import { FRAME_MIN, FrameNode } from "./FrameNode";
import { GraphNode, type GraphFlowNode, type GraphNodeData } from "./GraphNode";

const nodeTypes = { graph: GraphNode, frame: FrameNode };

/** Space around the nodes a new frame is drawn around, and its header. */
const FRAME_PAD = 30;
const FRAME_HEADER = 28;

/** A link being dragged that was dropped on empty canvas: the add menu
 * offers nodes that can take it, and links the new node in. */
interface PendingLink {
  nodeId: string;
  socketId: string;
  /** The dragged-from socket is an output ("source") or an input ("target"). */
  side: "source" | "target";
  type: SocketType;
}

interface MenuState {
  screen: { x: number; y: number };
  pending?: PendingLink;
}

function isEditable(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName));
}

function toFlowNode(
  view: NodeView,
  info: NodeTypeInfo | undefined,
  connected: string[],
  enabledBy: GraphNodeData["enabledBy"],
  previous?: GraphFlowNode,
  select?: boolean,
): GraphFlowNode {
  const frame = view.kind === "frame";
  // Keep the local position (and a frame's size) while it's being dragged or resized.
  const live = previous?.dragging || previous?.resizing;
  return {
    id: view.id,
    type: frame ? "frame" : "graph",
    position: live ? previous!.position : { x: view.position.x, y: view.position.y },
    selected: select || previous?.selected,
    dragging: previous?.dragging,
    data: { view, info, connected, enabledBy },
    ...(frame && {
      width: live ? previous!.width : Number(view.params.width ?? 400),
      height: live ? previous!.height : Number(view.params.height ?? 300),
      // Behind the nodes and links; only the header takes the pointer.
      zIndex: -1,
      dragHandle: ".frame-handle",
      style: { pointerEvents: "none" as const },
    }),
  };
}

/** Whether node `n` lies entirely inside `frame`. */
function inside(n: GraphFlowNode, frame: GraphFlowNode): boolean {
  const w = n.measured?.width ?? n.width ?? 0;
  const h = n.measured?.height ?? n.height ?? 0;
  const fw = frame.width ?? frame.measured?.width ?? 0;
  const fh = frame.height ?? frame.measured?.height ?? 0;
  return n.position.x >= frame.position.x && n.position.y >= frame.position.y && n.position.x + w <= frame.position.x + fw && n.position.y + h <= frame.position.y + fh;
}

function Editor() {
  const graph = useShowStore((s) => s.show?.graph);
  const run = useShowStore((s) => s.run);
  const types = useGraphStore((s) => s.nodeTypes);
  const byKind = useGraphStore((s) => s.byKind);
  const flow = useReactFlow<GraphFlowNode>();
  const [nodes, setNodes] = useState<GraphFlowNode[]>([]);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const pointer = useRef({ x: 200, y: 200 });
  const [presets, setPresets] = useState<NodePreset[]>([]);
  /** The name being typed for a new preset; null when not saving one. */
  const [presetName, setPresetName] = useState<string | null>(null);
  /** Nodes to select once they arrive from the backend (a spawned preset). */
  const selectNext = useRef<Set<string>>(new Set());
  /** While a frame is dragged: the nodes inside it and where each started,
   * and where the frame started. */
  const carried = useRef<{ frames: Map<string, { x: number; y: number }>; nodes: Map<string, { x: number; y: number; frame: string }> } | null>(null);

  useEffect(() => {
    void graphApi.nodePresets().then(setPresets).catch(() => undefined);
  }, []);

  const views = useMemo(() => new Map((graph?.nodes ?? []).map((v) => [v.id, v])), [graph]);

  useEffect(() => {
    if (!graph) return;
    const connected = new Map<string, string[]>();
    const enabledBy = new Map<string, GraphNodeData["enabledBy"]>();
    for (const e of graph.edges) {
      connected.set(e.toNode, [...(connected.get(e.toNode) ?? []), e.toSocket]);
      if (e.toSocket === "enabled") {
        const socket = graph.nodes.find((n) => n.id === e.fromNode)?.outputs.findIndex((s) => s.id === e.fromSocket) ?? -1;
        if (socket >= 0) enabledBy.set(e.toNode, { node: e.fromNode, socket });
      }
    }
    const select = selectNext.current;
    selectNext.current = new Set();
    setNodes((prev) => {
      const byId = new Map(prev.map((n) => [n.id, n]));
      const next = graph.nodes.map((v) => toFlowNode(v, byKind[v.kind], connected.get(v.id) ?? [], enabledBy.get(v.id), byId.get(v.id), select.has(v.id)));
      // A spawned preset replaces the selection.
      return select.size > 0 ? next.map((n) => ({ ...n, selected: select.has(n.id) })) : next;
    });
  }, [graph, byKind]);

  const socketType = useCallback(
    (nodeId: string | null | undefined, socketId: string | null | undefined, side: "source" | "target"): SocketType | undefined => {
      const view = nodeId ? views.get(nodeId) : undefined;
      return (side === "source" ? view?.outputs : view?.inputs)?.find((s) => s.id === socketId)?.type;
    },
    [views],
  );

  const edges: FlowEdge[] = useMemo(
    () =>
      (graph?.edges ?? []).map((e) => ({
        id: `${e.toNode}:${e.toSocket}`,
        source: e.fromNode,
        sourceHandle: e.fromSocket,
        target: e.toNode,
        targetHandle: e.toSocket,
        style: { stroke: SOCKET_COLORS[socketType(e.fromNode, e.fromSocket, "source") ?? "number"], strokeWidth: 2 },
      })),
    [graph, socketType],
  );

  const isValidConnection: IsValidConnection = useCallback(
    (c) => {
      const from = socketType(c.source, c.sourceHandle, "source");
      const to = socketType(c.target, c.targetHandle, "target");
      return c.source !== c.target && !!from && !!to && feeds(from, to);
    },
    [socketType],
  );

  const connect = (c: Connection) => {
    if (!c.sourceHandle || !c.targetHandle) return;
    void run(() => graphApi.connect({ fromNode: c.source, fromSocket: c.sourceHandle!, toNode: c.target, toSocket: c.targetHandle! }));
  };

  const onNodesChange = (changes: NodeChange<GraphFlowNode>[]) => {
    // Deletion goes through onDelete, so the show stays the source of truth.
    setNodes((ns) => applyNodeChanges(changes.filter((c) => c.type !== "remove"), ns));
  };

  const openMenu = (screen: { x: number; y: number }, pending?: PendingLink) => setMenu({ screen, pending });

  const addNode = async (kind: NodeKind) => {
    if (!menu) return;
    const position = flow.screenToFlowPosition(menu.screen);
    const pending = menu.pending;
    setMenu(null);
    const node = await run(() => graphApi.addNode(kind, position));
    if (!node || !pending) return;
    const info = byKind[kind];
    if (pending.side === "source") {
      const input = info?.inputs.find((s) => feeds(pending.type, s.type));
      if (input) void run(() => graphApi.connect({ fromNode: pending.nodeId, fromSocket: pending.socketId, toNode: node.id, toSocket: input.id }));
    } else {
      const output = info?.outputs.find((s) => feeds(s.type, pending.type));
      if (output) void run(() => graphApi.connect({ fromNode: node.id, fromSocket: output.id, toNode: pending.nodeId, toSocket: pending.socketId }));
    }
  };

  /** Cmd/Ctrl+J: a new frame around the selected nodes. */
  const frameSelected = async () => {
    const selected = flow.getNodes().filter((n) => n.selected);
    if (selected.length === 0) return;
    const b = flow.getNodesBounds(selected);
    const position = { x: b.x - FRAME_PAD, y: b.y - FRAME_PAD - FRAME_HEADER };
    const size = {
      width: Math.max(FRAME_MIN.width, Math.round(b.width + 2 * FRAME_PAD)),
      height: Math.max(FRAME_MIN.height, Math.round(b.height + 2 * FRAME_PAD + FRAME_HEADER)),
    };
    const frame = await run(() => graphApi.addNode("frame", position, size));
    if (frame) selectNext.current = new Set([frame.id]);
  };

  const onNodeDragStart = (_e: unknown, _node: GraphFlowNode, dragged: GraphFlowNode[]) => {
    const frames = dragged.filter((n) => n.type === "frame");
    if (frames.length === 0) {
      carried.current = null;
      return;
    }
    const all = flow.getNodes();
    const draggedIds = new Set(dragged.map((n) => n.id));
    const nodes = new Map<string, { x: number; y: number; frame: string }>();
    for (const frame of frames) {
      for (const n of all) {
        if (n.id !== frame.id && !draggedIds.has(n.id) && !nodes.has(n.id) && inside(n, frame)) {
          nodes.set(n.id, { ...n.position, frame: frame.id });
        }
      }
    }
    carried.current = { frames: new Map(frames.map((f) => [f.id, { ...f.position }])), nodes };
  };

  const onNodeDrag = (_e: unknown, _node: GraphFlowNode, dragged: GraphFlowNode[]) => {
    const c = carried.current;
    if (!c || c.nodes.size === 0) return;
    const delta = new Map<string, { x: number; y: number }>();
    for (const f of dragged) {
      const start = c.frames.get(f.id);
      if (start) delta.set(f.id, { x: f.position.x - start.x, y: f.position.y - start.y });
    }
    setNodes((ns) =>
      ns.map((n) => {
        const start = c.nodes.get(n.id);
        const d = start && delta.get(start.frame);
        return d ? { ...n, position: { x: start.x + d.x, y: start.y + d.y } } : n;
      }),
    );
  };

  const onNodeDragStop = (_e: unknown, _node: GraphFlowNode, dragged: GraphFlowNode[]) => {
    const moved = new Map(dragged.map((n) => [n.id, n.position]));
    const c = carried.current;
    carried.current = null;
    if (c) {
      for (const n of flow.getNodes()) if (c.nodes.has(n.id)) moved.set(n.id, n.position);
    }
    void run(() => graphApi.moveNodes([...moved].map(([id, p]) => ({ id, position: { x: p.x, y: p.y } }))));
  };

  const spawnPreset = async (preset: NodePreset) => {
    if (!menu) return;
    const position = flow.screenToFlowPosition(menu.screen);
    setMenu(null);
    const ids = await run(() => graphApi.spawnNodePreset(preset.id, position));
    // The new graph may already be here; if not, select when it arrives.
    if (!ids) return;
    setNodes((ns) => {
      if (!ns.some((n) => ids.includes(n.id))) {
        selectNext.current = new Set(ids);
        return ns;
      }
      return ns.map((n) => ({ ...n, selected: ids.includes(n.id) }));
    });
  };

  const deletePreset = async (preset: NodePreset) => {
    await run(() => graphApi.deleteNodePreset(preset.id));
    setPresets((ps) => ps.filter((p) => p.id !== preset.id));
  };

  const selectedIds = nodes.filter((n) => n.selected).map((n) => n.id);

  const savePreset = async () => {
    const name = presetName?.trim();
    if (!name || selectedIds.length === 0) return;
    const saved = await run(() => graphApi.saveNodePreset(name, selectedIds));
    if (!saved) return;
    setPresetName(null);
    void graphApi.nodePresets().then(setPresets).catch(() => undefined);
  };

  const onConnectEnd = (event: MouseEvent | TouchEvent, state: FinalConnectionState) => {
    if (state.isValid || !state.fromNode || !state.fromHandle?.id) return;
    const side = state.fromHandle.type;
    const type = socketType(state.fromNode.id, state.fromHandle.id, side);
    if (!type) return;
    const point = "changedTouches" in event ? event.changedTouches[0] : event;
    openMenu({ x: point.clientX, y: point.clientY }, { nodeId: state.fromNode.id, socketId: state.fromHandle.id, side, type });
  };

  const duplicateSelected = async () => {
    const selected = nodes.filter((n) => n.selected);
    for (const n of selected) {
      const { view } = n.data;
      await run(() => graphApi.addNode(view.kind, { x: view.position.x + 40, y: view.position.y + 40 }, view.params));
    }
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isEditable(e.target) || menu) return;
      if ((e.shiftKey && e.key.toLowerCase() === "a") || e.key === "Tab") {
        e.preventDefault();
        openMenu(pointer.current);
      } else if ((e.shiftKey && e.key.toLowerCase() === "d") || ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "d")) {
        e.preventDefault();
        void duplicateSelected();
      } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "j") {
        e.preventDefault();
        void frameSelected();
      } else if (e.key === "Home" || (e.key.toLowerCase() === "f" && !e.metaKey && !e.ctrlKey)) {
        void flow.fitView({ padding: 0.2, duration: 200 });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const accepts = useMemo(() => {
    const pending = menu?.pending;
    if (!pending) return undefined;
    return (info: NodeTypeInfo) =>
      pending.side === "source" ? info.inputs.some((s) => feeds(pending.type, s.type)) : info.outputs.some((s) => feeds(s.type, pending.type));
  }, [menu]);

  return (
    <div
      className="relative h-full w-full"
      onMouseMove={(e) => {
        pointer.current = { x: e.clientX, y: e.clientY };
      }}
    >
      <ReactFlow<GraphFlowNode>
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        colorMode="dark"
        onNodesChange={onNodesChange}
        onConnect={connect}
        onConnectEnd={onConnectEnd}
        isValidConnection={isValidConnection}
        onNodeDragStart={onNodeDragStart}
        onNodeDrag={onNodeDrag}
        onNodeDragStop={onNodeDragStop}
        elevateNodesOnSelect={false}
        onDelete={({ nodes: gone, edges: cut }) => {
          const ids = gone.map((n) => n.id);
          if (ids.length > 0) void run(() => graphApi.removeNodes(ids));
          for (const e of cut) {
            if (!ids.includes(e.source) && !ids.includes(e.target) && e.targetHandle) {
              void run(() => graphApi.disconnect(e.target, e.targetHandle!));
            }
          }
        }}
        edgesReconnectable
        onReconnect={(old, next) => {
          // Connect first: unplugging can drop an Any node's spare input,
          // which may be the one being dropped onto.
          void run(async () => {
            if (next.sourceHandle && next.targetHandle) {
              await graphApi.connect({ fromNode: next.source, fromSocket: next.sourceHandle, toNode: next.target, toSocket: next.targetHandle });
            }
            if (old.target !== next.target || old.targetHandle !== next.targetHandle) await graphApi.disconnect(old.target, old.targetHandle!);
          });
        }}
        onReconnectEnd={(_e, edge, _side, state) => {
          // Dragged off into empty space: unplug, like Blender.
          if (!state.isValid && edge.targetHandle) void run(() => graphApi.disconnect(edge.target, edge.targetHandle!));
        }}
        onPaneContextMenu={(e) => {
          e.preventDefault();
          openMenu({ x: e.clientX, y: e.clientY });
        }}
        deleteKeyCode={["Backspace", "Delete"]}
        multiSelectionKeyCode={["Meta", "Control", "Shift"]}
        // Trackpad-friendly, like Figma: drag on empty canvas draws a
        // selection box, two-finger scroll pans, pinch (or Ctrl+wheel) zooms.
        // A mouse pans with the middle button or Space+drag.
        selectionOnDrag
        panOnScroll
        panOnDrag={[1]}
        fitView
        fitViewOptions={{ padding: 0.2 }}
        minZoom={0.2}
        proOptions={{ hideAttribution: true }}
      >
        <Background variant={BackgroundVariant.Dots} gap={20} size={1} />
        <Controls showInteractive={false} />
        <MiniMap
          pannable
          zoomable
          nodeColor={(n) => CATEGORY_COLORS[(n as GraphFlowNode).data.info?.category ?? ""] ?? "#555"}
          className="!bg-card"
        />
      </ReactFlow>

      <div className="absolute left-3 top-3 flex items-center gap-2">
        <Button size="sm" variant="secondary" onClick={() => openMenu({ x: pointer.current.x, y: pointer.current.y })}>
          <Plus />
          Add node
        </Button>
        {presetName === null ? (
          selectedIds.length > 0 && (
            <Button
              size="sm"
              variant="secondary"
              onClick={() => setPresetName("")}
              title="Save the selected nodes and the links between them, to add again from the Add node menu in any show"
            >
              <Bookmark />
              Save {selectedIds.length === 1 ? "node" : `${selectedIds.length} nodes`} as preset
            </Button>
          )
        ) : (
          <form
            className="flex items-center gap-2 rounded-md bg-card p-1 shadow-lg"
            onSubmit={(e) => {
              e.preventDefault();
              void savePreset();
            }}
          >
            <Input
              autoFocus
              className="h-8 w-48"
              placeholder="Preset name"
              value={presetName}
              onChange={(e) => setPresetName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") setPresetName(null);
              }}
            />
            <Button size="sm" type="submit" disabled={!presetName.trim() || selectedIds.length === 0}>
              Save
            </Button>
            <Button size="sm" variant="ghost" type="button" onClick={() => setPresetName(null)}>
              Cancel
            </Button>
          </form>
        )}
      </div>

      {nodes.length === 0 && (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
          <div className="max-w-sm text-center text-sm text-muted-foreground">
            <p className="mb-2 text-foreground">Nothing plays yet.</p>
            <p>
              Right-click or press <kbd className="rounded bg-muted px-1">Shift</kbd>+<kbd className="rounded bg-muted px-1">A</kbd> to add
              nodes. Start with a Fixture Output aimed at a group, then feed it a Color and an LFO.
            </p>
          </div>
        </div>
      )}

      {menu && (
        <AddNodeMenu
          x={menu.screen.x}
          y={menu.screen.y}
          types={types}
          presets={presets}
          accepts={accepts}
          onPick={(k) => void addNode(k)}
          onPickPreset={(p) => void spawnPreset(p)}
          onDeletePreset={(p) => void deletePreset(p)}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}

/** The Nodes tab: the show's node graph. */
export function NodeEditor() {
  return (
    <ReactFlowProvider>
      <Editor />
    </ReactFlowProvider>
  );
}
