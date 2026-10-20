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
import { Plus } from "lucide-react";
import * as graphApi from "../../lib/graph";
import { CATEGORY_COLORS, SOCKET_COLORS, feeds } from "../../lib/graph";
import type { NodeKind, NodeTypeInfo, NodeView, SocketType } from "../../lib/showfile/generated";
import { useGraphStore } from "../../store/useGraphStore";
import { useShowStore } from "../../store/useShowStore";
import { Button } from "../ui/button";
import { AddNodeMenu } from "./AddNodeMenu";
import { GraphNode, type GraphFlowNode } from "./GraphNode";

const nodeTypes = { graph: GraphNode };

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

function toFlowNode(view: NodeView, info: NodeTypeInfo | undefined, connected: string[], previous?: GraphFlowNode): GraphFlowNode {
  return {
    id: view.id,
    type: "graph",
    // Keep the local position while it's being dragged.
    position: previous?.dragging ? previous.position : { x: view.position.x, y: view.position.y },
    selected: previous?.selected,
    dragging: previous?.dragging,
    data: { view, info, connected },
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

  const views = useMemo(() => new Map((graph?.nodes ?? []).map((v) => [v.id, v])), [graph]);

  useEffect(() => {
    if (!graph) return;
    const connected = new Map<string, string[]>();
    for (const e of graph.edges) connected.set(e.toNode, [...(connected.get(e.toNode) ?? []), e.toSocket]);
    setNodes((prev) => {
      const byId = new Map(prev.map((n) => [n.id, n]));
      return graph.nodes.map((v) => toFlowNode(v, byKind[v.kind], connected.get(v.id) ?? [], byId.get(v.id)));
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
        onNodeDragStop={(_e, _node, dragged) =>
          void run(() => graphApi.moveNodes(dragged.map((n) => ({ id: n.id, position: { x: n.position.x, y: n.position.y } }))))
        }
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
          void run(async () => {
            await graphApi.disconnect(old.target, old.targetHandle!);
            if (next.sourceHandle && next.targetHandle) {
              await graphApi.connect({ fromNode: next.source, fromSocket: next.sourceHandle, toNode: next.target, toSocket: next.targetHandle });
            }
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
        <AddNodeMenu x={menu.screen.x} y={menu.screen.y} types={types} accepts={accepts} onPick={(k) => void addNode(k)} onClose={() => setMenu(null)} />
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
