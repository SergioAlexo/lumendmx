import type { Edge, NodeTypeInfo, NodeKind, NodeView, SocketType } from "../../lib/showfile/generated";

/** A socket of a node inside a collapsed frame, shown on the frame. Its
 * handle id names the real node and socket, so links stay on real nodes.
 * `exposed` is false for a socket shown only because a link crosses into
 * the frame without it having been exposed. */
export type FramePort = {
  handle: string;
  node: string;
  socket: string;
  label: string;
  type: SocketType;
  side: "in" | "out";
  exposed: boolean;
};

export type FrameLayout = {
  /** Nodes hidden in a collapsed frame → the visible frame that stands in for them. */
  owner: Map<string, string>;
  /** Each visible collapsed frame's sockets: inputs, then outputs. */
  ports: Map<string, FramePort[]>;
};

/** "in:rate" / "out:value": a socket of a node, as exposure keys it. */
export const socketKey = (side: "in" | "out", socket: string) => `${side}:${socket}`;

export const portHandle = (side: "in" | "out", node: string, socket: string) => `${side}:${node}:${socket}`;

/** A frame port's handle back to the real node and socket; anything else as is. */
export function resolveHandle(node: string, handle: string): { node: string; socket: string } {
  const m = /^(in|out):([^:]+):(.+)$/.exec(handle);
  return m ? { node: m[2], socket: m[3] } : { node, socket: handle };
}

/** A node belongs to a frame when its top-left corner lies inside the
 * frame's (expanded) box. */
export function inFrame(frame: { position: { x: number; y: number }; params: NodeView["params"] }, p: { x: number; y: number }): boolean {
  const w = Number(frame.params.width ?? 400);
  const h = Number(frame.params.height ?? 300);
  return p.x >= frame.position.x && p.y >= frame.position.y && p.x <= frame.position.x + w && p.y <= frame.position.y + h;
}

/** The node's exposed sockets, by key ("in:rate") → the name shown on the
 * frame ("" for the socket's own label). Stored as JSON in its "exposed"
 * param. */
export function exposedSockets(view: NodeView): Map<string, string> {
  try {
    const parsed: unknown = JSON.parse(String(view.params.exposed ?? "{}"));
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return new Map(Object.entries(parsed as Record<string, unknown>).map(([k, v]) => [k, typeof v === "string" ? v : ""]));
    }
  } catch {
    // Not JSON: nothing exposed.
  }
  return new Map();
}

/** The "exposed" param value for `map`. */
export function exposedParam(map: Map<string, string>): string {
  return JSON.stringify(Object.fromEntries(map));
}

/** Frames whose box contains node `id`. */
export function framesAround(nodes: NodeView[], id: string): NodeView[] {
  const node = nodes.find((n) => n.id === id);
  if (!node) return [];
  return nodes.filter((f) => f.kind === "frame" && f.id !== id && inFrame(f, node.position));
}

/** Frames a link from `from` to `to` crosses into or out of: around exactly one end. */
export function crossedFrames(nodes: NodeView[], from: string, to: string): { into: NodeView[]; outOf: NodeView[] } {
  const a = new Set(framesAround(nodes, from).map((f) => f.id));
  const b = new Set(framesAround(nodes, to).map((f) => f.id));
  return {
    into: framesAround(nodes, to).filter((f) => !a.has(f.id)),
    outOf: framesAround(nodes, from).filter((f) => !b.has(f.id)),
  };
}

export function frameLayout(nodes: NodeView[], edges: Edge[], byKind: Partial<Record<NodeKind, NodeTypeInfo>>): FrameLayout {
  const collapsed = nodes.filter((n) => n.kind === "frame" && n.params.collapsed === true);
  const owner = new Map<string, string>();
  const ports = new Map<string, FramePort[]>();
  if (collapsed.length === 0) return { owner, ports };

  // Hidden: inside any collapsed frame. Shown by the outermost one, i.e.
  // the containing collapsed frame that isn't hidden itself.
  const containing = new Map<string, NodeView[]>();
  for (const n of nodes) {
    const frames = collapsed.filter((f) => f.id !== n.id && inFrame(f, n.position));
    if (frames.length > 0) containing.set(n.id, frames);
  }
  for (const [id, frames] of containing) {
    owner.set(id, (frames.find((f) => !containing.has(f.id)) ?? frames[0]).id);
  }

  const views = new Map(nodes.map((n) => [n.id, n]));
  const nodeLabel = (v: NodeView) => String(v.params.label ?? byKind[v.kind]?.label ?? v.kind);
  const lists = new Map<string, { inputs: Map<string, FramePort>; outputs: Map<string, FramePort> }>();
  const add = (frame: string, side: "in" | "out", node: string, socket: string) => {
    const view = views.get(node);
    const info = (side === "in" ? view?.inputs : view?.outputs)?.find((s) => s.id === socket);
    if (!view || !info) return;
    const list = lists.get(frame) ?? { inputs: new Map(), outputs: new Map() };
    lists.set(frame, list);
    const handle = portHandle(side, node, socket);
    const name = exposedSockets(view).get(socketKey(side, socket));
    const exposed = name !== undefined;
    const label = exposed ? name || info.label : `${info.label} · ${nodeLabel(view)}`;
    (side === "in" ? list.inputs : list.outputs).set(handle, { handle, node, socket, label, type: info.type, side, exposed });
  };

  for (const e of edges) {
    const from = owner.get(e.fromNode);
    const to = owner.get(e.toNode);
    if (from === to) continue;
    if (to) add(to, "in", e.toNode, e.toSocket);
    if (from) add(from, "out", e.fromNode, e.fromSocket);
  }
  for (const [id, frame] of owner) {
    const view = views.get(id);
    if (!view) continue;
    for (const key of exposedSockets(view).keys()) {
      const [side, socket] = key.split(":");
      if ((side === "in" || side === "out") && socket) add(frame, side, id, socket);
    }
  }

  const order = (p: FramePort) => views.get(p.node)?.position.y ?? 0;
  for (const [frame, { inputs, outputs }] of lists) {
    ports.set(frame, [...[...inputs.values()].sort((a, b) => order(a) - order(b)), ...[...outputs.values()].sort((a, b) => order(a) - order(b))]);
  }
  return { owner, ports };
}
