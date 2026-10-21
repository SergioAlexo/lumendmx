import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Edge, Node, NodeKind, NodeMove, NodePosition, NodePreset, NodeTypeInfo, ParamValue, SocketType } from "./showfile/generated";

// Wrappers for the graph commands (src-tauri/src/show/commands.rs). Edits
// are undoable show edits; they come back as `show://changed`.

/** A param change: a value sets it, null resets it to its default. */
export type ParamChanges = Record<string, ParamValue | null>;

export function nodeTypes(): Promise<NodeTypeInfo[]> {
  return invoke("graph_node_types");
}

export function addNode(kind: NodeKind, position: NodePosition, params?: ParamChanges): Promise<Node> {
  return invoke("graph_add_node", { kind, position, params: params ?? null });
}

/** Saved node presets, app-wide (not part of the show). */
export function nodePresets(): Promise<NodePreset[]> {
  return invoke("graph_node_presets");
}

/** Saves the nodes `ids` and the links between them as a preset. */
export function saveNodePreset(name: string, ids: string[]): Promise<NodePreset> {
  return invoke("graph_save_node_preset", { name, ids });
}

export function deleteNodePreset(id: string): Promise<void> {
  return invoke("graph_delete_node_preset", { id });
}

/** Adds a copy of a preset with its top-left node at `position`; resolves to the new node ids. */
export function spawnNodePreset(id: string, position: NodePosition): Promise<string[]> {
  return invoke("graph_spawn_node_preset", { id, position });
}

export function removeNodes(ids: string[]): Promise<void> {
  return invoke("graph_remove_nodes", { ids });
}

export function connect(edge: Edge): Promise<void> {
  return invoke("graph_connect", { edge });
}

export function disconnect(toNode: string, toSocket: string): Promise<void> {
  return invoke("graph_disconnect", { toNode, toSocket });
}

export function setParams(id: string, params: ParamChanges): Promise<Node> {
  return invoke("graph_set_params", { id, params });
}

export function moveNodes(moves: NodeMove[]): Promise<void> {
  return invoke("graph_move_nodes", { moves });
}

/** A playback-bar Fader/Button value, 0-1. Live, not an undoable edit. */
export function setControl(id: string, value: number): Promise<void> {
  return invoke("graph_set_control", { id, value });
}

export function controls(): Promise<Record<string, number>> {
  return invoke("graph_controls");
}

/** The last MIDI controller/note received: [isNote, channel 1-16, number]. */
export function lastMidi(): Promise<[boolean, number, number] | null> {
  return invoke("graph_last_midi");
}

/** Each node's live output values (~20Hz), by node id; each value is [x, y, z]. */
export function onGraphValues(cb: (values: Record<string, number[][]>) => void) {
  return listen<Record<string, number[][]>>("graph://values", (event) => cb(event.payload));
}

/** Mirrors `SocketType::feeds` in src-tauri/src/graph/registry.rs. */
export function feeds(from: SocketType, to: SocketType): boolean {
  if (from === to) return true;
  const pair = `${from}>${to}`;
  return ["number>trigger", "trigger>number", "number>color", "trigger>color", "number>position", "color>number"].includes(pair);
}

/** Socket colours, Blender-style: grey numbers, yellow colours, blue
 * positions, pink triggers. */
export const SOCKET_COLORS: Record<SocketType, string> = {
  number: "#a1a1a1",
  color: "#c7c729",
  position: "#6363c7",
  trigger: "#cc6699",
};

export const CATEGORY_COLORS: Record<string, string> = {
  Input: "#83314a",
  Fixtures: "#3b5998",
  Math: "#246283",
  Colour: "#6c6c2f",
  Position: "#4b3b83",
  Switching: "#3c6b3c",
  Output: "#a5492b",
  Layout: "#6b7280",
};
