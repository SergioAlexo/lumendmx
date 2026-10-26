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
import { FixturePreview } from "./FixturePreview";
import { AddNodeMenu } from "./AddNodeMenu";
import { FRAME_MIN, FrameNode } from "./FrameNode";
import { GraphNode, type GraphFlowNode, type GraphNodeData } from "./GraphNode";
import {
  crossedFrames,
  exposedParam,
  exposedSockets,
  frameLayout,
  descendants,
  portHandle,
  resolveHandle,
  socketKey,
  type FramePort,
} from "./frames";

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

type Link = { fromNode: string; fromSocket: string; toNode: string; toSocket: string };

/** A link that crosses into or out of a frame through sockets that aren't
 * exposed yet: the dialog asks to expose them (with a name) first. */
interface ExposeRequest {
  link: Link;
  /** The link this one replaces (a reconnected wire), unplugged after. */
  replaces?: { toNode: string; toSocket: string };
  items: { node: string; side: "in" | "out"; socket: string; what: string; frames: string; name: string }[];
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
  hidden?: boolean,
  ports?: FramePort[],
  framed?: boolean,
): GraphFlowNode {
  const frame = view.kind === "frame";
  const collapsed = frame && view.params.collapsed === true;
  // Keep the local position (and a frame's size) while it's being dragged or resized.
  const live = previous?.dragging || previous?.resizing;
  return {
    id: view.id,
    type: frame ? "frame" : "graph",
    position: live ? previous!.position : { x: view.position.x, y: view.position.y },
    selected: !hidden && (select || previous?.selected),
    dragging: previous?.dragging,
    hidden,
    data: { view, info, connected, enabledBy, ports, framed },
    // Collapsed, a frame is an ordinary node: sized by its sockets.
    ...(frame && !collapsed && {
      width: live ? previous!.width : Number(view.params.width ?? 400),
      height: live ? previous!.height : Number(view.params.height ?? 300),
      // Behind the nodes and links; only the header takes the pointer.
      zIndex: -1,
      dragHandle: ".frame-handle",
      style: { pointerEvents: "none" as const },
    }),
  };
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
  const layout = useMemo(() => frameLayout(graph?.nodes ?? [], graph?.edges ?? [], byKind), [graph, byKind]);

  /** The nodes in frame `frameId`, at any depth, as they are on the canvas now. */
  const members = (frameId: string): GraphFlowNode[] => {
    const inside = descendants(graph?.nodes ?? [], frameId);
    return flow.getNodes().filter((n) => inside.has(n.id));
  };

  // Asked to show a node: centre on it and select only it. Waits until the
  // node is on the canvas (the tab may have just opened).
  const focus = useGraphStore((s) => s.focus);
  const focused = useRef(0);
  useEffect(() => {
    if (!focus || focus.seq === focused.current) return;
    // Hidden in a collapsed frame: show the frame.
    const target = layout.owner.get(focus.id) ?? focus.id;
    const node = nodes.find((n) => n.id === target);
    if (!node) return;
    focused.current = focus.seq;
    setNodes((ns) => ns.map((n) => ({ ...n, selected: n.id === target })));
    const w = node.measured?.width ?? node.width ?? 200;
    const h = node.measured?.height ?? node.height ?? 100;
    void flow.setCenter(node.position.x + w / 2, node.position.y + h / 2, { zoom: Math.max(flow.getZoom(), 1), duration: 300 });
  }, [focus, nodes, flow, layout]);

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
      const next = graph.nodes.map((v) =>
        toFlowNode(
          v,
          byKind[v.kind],
          connected.get(v.id) ?? [],
          enabledBy.get(v.id),
          byId.get(v.id),
          select.has(v.id),
          layout.owner.has(v.id),
          layout.ports.get(v.id),
          v.kind !== "frame" && v.frame != null,
        ),
      );
      // A spawned preset replaces the selection.
      return select.size > 0 ? next.map((n) => ({ ...n, selected: select.has(n.id) })) : next;
    });
  }, [graph, byKind, layout]);

  const socketType = useCallback(
    (nodeId: string | null | undefined, socketId: string | null | undefined, side: "source" | "target"): SocketType | undefined => {
      if (!nodeId || !socketId) return undefined;
      const real = resolveHandle(nodeId, socketId);
      const view = views.get(real.node);
      return (side === "source" ? view?.outputs : view?.inputs)?.find((s) => s.id === real.socket)?.type;
    },
    [views],
  );

  const edges: FlowEdge[] = useMemo(
    () =>
      (graph?.edges ?? []).map((e) => {
        // An end inside a collapsed frame attaches to the frame's socket for it.
        const from = layout.owner.get(e.fromNode);
        const to = layout.owner.get(e.toNode);
        return {
          id: `${e.toNode}:${e.toSocket}`,
          source: from ?? e.fromNode,
          sourceHandle: from ? portHandle("out", e.fromNode, e.fromSocket) : e.fromSocket,
          target: to ?? e.toNode,
          targetHandle: to ? portHandle("in", e.toNode, e.toSocket) : e.toSocket,
          hidden: !!from && from === to,
          data: { ...e },
          style: { stroke: SOCKET_COLORS[socketType(e.fromNode, e.fromSocket, "source") ?? "number"], strokeWidth: 2 },
        };
      }),
    [graph, socketType, layout],
  );

  const isValidConnection: IsValidConnection = useCallback(
    (c) => {
      const from = socketType(c.source, c.sourceHandle, "source");
      const to = socketType(c.target, c.targetHandle, "target");
      const real = (node: string, handle: string | null | undefined) => (handle ? resolveHandle(node, handle).node : node);
      return real(c.source, c.sourceHandle) !== real(c.target, c.targetHandle) && !!from && !!to && feeds(from, to);
    },
    [socketType],
  );

  /** A connection drawn on the canvas, as a link between real nodes. */
  const toEdge = (c: { source: string; sourceHandle?: string | null; target: string; targetHandle?: string | null }) => {
    if (!c.sourceHandle || !c.targetHandle) return null;
    const from = resolveHandle(c.source, c.sourceHandle);
    const to = resolveHandle(c.target, c.targetHandle);
    return { fromNode: from.node, fromSocket: from.socket, toNode: to.node, toSocket: to.socket };
  };

  const [expose, setExpose] = useState<ExposeRequest | null>(null);

  /** Links `link` (unplugging `replaces` after), unless it crosses a frame
   * through a socket that isn't exposed: then asks first. */
  const linkUp = (link: Link, replaces?: { toNode: string; toSocket: string }) => {
    const all = graph?.nodes ?? [];
    const { into, outOf } = crossedFrames(all, link.fromNode, link.toNode);
    const items: ExposeRequest["items"] = [];
    const ask = (node: string, side: "in" | "out", socket: string, frames: NodeView[]) => {
      const view = views.get(node);
      if (!view || frames.length === 0 || exposedSockets(view).has(socketKey(side, socket))) return;
      const info = (side === "in" ? view.inputs : view.outputs).find((s) => s.id === socket);
      const nodeName = String(view.params.label ?? byKind[view.kind]?.label ?? view.kind);
      const what = `${side === "in" ? "Input" : "Output"} “${info?.label ?? socket}” of ${nodeName}`;
      items.push({ node, side, socket, what, frames: frames.map((f) => `“${String(f.params.label ?? "Frame")}”`).join(", "), name: info?.label ?? socket });
    };
    ask(link.toNode, "in", link.toSocket, into);
    ask(link.fromNode, "out", link.fromSocket, outOf);
    if (items.length > 0) {
      setExpose({ link, replaces, items });
      return;
    }
    void run(async () => {
      await graphApi.connect(link);
      if (replaces && (replaces.toNode !== link.toNode || replaces.toSocket !== link.toSocket)) await graphApi.disconnect(replaces.toNode, replaces.toSocket);
    });
  };

  const confirmExpose = () => {
    if (!expose) return;
    const { link, replaces, items } = expose;
    setExpose(null);
    void run(async () => {
      for (const item of items) {
        const view = views.get(item.node);
        if (!view) continue;
        const map = exposedSockets(view);
        map.set(socketKey(item.side, item.socket), item.name.trim());
        await graphApi.setParams(item.node, { exposed: exposedParam(map) });
      }
      await graphApi.connect(link);
      if (replaces && (replaces.toNode !== link.toNode || replaces.toSocket !== link.toSocket)) await graphApi.disconnect(replaces.toNode, replaces.toSocket);
    });
  };

  const connect = (c: Connection) => {
    const edge = toEdge(c);
    if (edge) linkUp(edge);
  };

  /** The real link a canvas edge draws. */
  const realEdge = (e: FlowEdge) => e.data as unknown as { toNode: string; toSocket: string };

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
    const draggedIds = new Set(dragged.map((n) => n.id));
    const nodes = new Map<string, { x: number; y: number; frame: string }>();
    for (const frame of frames) {
      for (const n of members(frame.id)) {
        if (!draggedIds.has(n.id) && !nodes.has(n.id)) nodes.set(n.id, { ...n.position, frame: frame.id });
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
    // A selected frame brings the nodes inside it (the backend adds them).
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
    const real = resolveHandle(state.fromNode.id, state.fromHandle.id);
    openMenu({ x: point.clientX, y: point.clientY }, { nodeId: real.node, socketId: real.socket, side, type });
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
            const real = realEdge(e);
            if (!ids.includes(e.source) && !ids.includes(e.target)) void run(() => graphApi.disconnect(real.toNode, real.toSocket));
          }
        }}
        edgesReconnectable
        onReconnect={(old, next) => {
          // Connect first: unplugging can drop an Any node's spare input,
          // which may be the one being dropped onto.
          const was = realEdge(old);
          const edge = toEdge(next);
          if (edge) linkUp(edge, was);
          else void run(() => graphApi.disconnect(was.toNode, was.toSocket));
        }}
        onReconnectEnd={(_e, edge, _side, state) => {
          // Dragged off into empty space: unplug, like Blender.
          if (!state.isValid) void run(() => graphApi.disconnect(realEdge(edge).toNode, realEdge(edge).toSocket));
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

      {expose && (
        <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/40" onPointerDown={() => setExpose(null)}>
          <form
            className="flex w-96 flex-col gap-3 rounded-md border border-border bg-card p-4 text-sm shadow-2xl"
            onPointerDown={(e) => e.stopPropagation()}
            onSubmit={(e) => {
              e.preventDefault();
              confirmExpose();
            }}
          >
            <h3 className="font-semibold">Expose on the frame?</h3>
            <p className="text-muted-foreground">This link crosses a frame. Its sockets on the frame need exposing first, so the frame shows them when it's collapsed.</p>
            {expose.items.map((item, i) => (
              <label key={`${item.node}:${item.side}:${item.socket}`} className="flex flex-col gap-1">
                <span>
                  {item.what}, on frame {item.frames}. Name:
                </span>
                <Input
                  autoFocus={i === 0}
                  value={item.name}
                  onChange={(e) => {
                    const name = e.target.value;
                    setExpose((x) => x && { ...x, items: x.items.map((it, j) => (j === i ? { ...it, name } : it)) });
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Escape") setExpose(null);
                  }}
                />
              </label>
            ))}
            <div className="flex justify-end gap-2">
              <Button type="button" size="sm" variant="ghost" onClick={() => setExpose(null)}>
                Cancel
              </Button>
              <Button type="submit" size="sm">
                Expose and connect
              </Button>
            </div>
          </form>
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
    <div className="flex h-full w-full">
      <div className="h-full min-w-0 flex-1">
        <ReactFlowProvider>
          <Editor />
        </ReactFlowProvider>
      </div>
      <FixturePreview />
    </div>
  );
}
