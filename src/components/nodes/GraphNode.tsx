import { memo, useState } from "react";
import { Handle, Position, type NodeProps, type Node as FlowNode } from "@xyflow/react";
import { AlertTriangle, Radio } from "lucide-react";
import * as graphApi from "../../lib/graph";
import * as timing from "../../lib/timing";
import { CATEGORY_COLORS, SOCKET_COLORS } from "../../lib/graph";
import type { Group, NodeTypeInfo, NodeView, ParamValue, SocketInfo } from "../../lib/showfile/generated";
import { cn } from "../../lib/utils";
import { useGraphStore } from "../../store/useGraphStore";
import { EMPTY_GROUPS, useShowStore } from "../../store/useShowStore";
import { ColorField, CurveEditor, ParamControl, ScrubNumber } from "./controls";
import type { CurveMode } from "../../lib/curve";

/** Params that only matter for some settings of the node: an LFO's curve
 * for Wave Custom, its width for the waves that use one. */
function paramShown(view: NodeView, id: string): boolean {
  if (view.kind !== "lfo") return true;
  const wave = String(view.params.waveform ?? "sine");
  if (id === "curve" || id === "curveMode") return wave === "custom";
  if (id === "width") return wave === "square" || wave === "pulse";
  return true;
}

export type GraphNodeData = {
  view: NodeView;
  info?: NodeTypeInfo;
  /** Input socket ids with a link into them. */
  connected: string[];
  /** What drives the Enabled input, if linked: a node and its output index. */
  enabledBy?: { node: string; socket: number };
};

/** The input every node has (registry.rs ENABLED), shown in the header. */
const ENABLED = "enabled";

export type GraphFlowNode = FlowNode<GraphNodeData, "graph" | "frame">;

function toHex(v: number[]): string {
  return `#${v
    .slice(0, 3)
    .map((c) => Math.round(Math.min(1, Math.max(0, c)) * 255).toString(16).padStart(2, "0"))
    .join("")}`;
}

/** A small live readout of an output's current value. */
function Preview({ socket, value }: { socket: SocketInfo; value: number[] | undefined }) {
  if (!value) return null;
  switch (socket.type) {
    case "color":
      return <span className="h-3 w-5 rounded-sm border border-white/20" style={{ backgroundColor: toHex(value) }} />;
    case "trigger":
      return <span className={cn("h-2 w-2 rounded-full", value[0] > 0.5 ? "bg-[#cc6699]" : "bg-muted")} />;
    case "position":
      return (
        <span className="relative h-3 w-3 rounded-sm bg-muted">
          <span
            className="absolute h-1 w-1 rounded-full bg-[#8a8aff]"
            style={{ left: `${Math.min(1, Math.max(0, value[0])) * 8}px`, top: `${(1 - Math.min(1, Math.max(0, value[1]))) * 8}px` }}
          />
        </span>
      );
    default:
      return (
        <span className="flex items-center gap-1">
          <span className="h-1.5 w-8 overflow-hidden rounded bg-muted">
            <span className="block h-full bg-[#a1a1a1]" style={{ width: `${Math.min(1, Math.max(0, value[0])) * 100}%` }} />
          </span>
          <span className="w-8 text-right tabular-nums text-muted-foreground">{value[0].toFixed(2)}</span>
        </span>
      );
  }
}

/** An unconnected input's inline value editor. */
function InputValue({ socket, value, onCommit }: { socket: SocketInfo; value: ParamValue | undefined; onCommit: (v: ParamValue) => void }) {
  const v = value ?? socket.default;
  if (v === null) return null;
  if (socket.type === "color") return <ColorField value={String(v)} onCommit={onCommit} />;
  if (socket.type === "trigger")
    return <input type="checkbox" className="nodrag" checked={Number(v) > 0.5} onChange={(e) => onCommit(e.target.checked ? 1 : 0)} />;
  return <ScrubNumber value={Number(v)} step={0.01} onCommit={onCommit} />;
}

/** Waits for the next MIDI controller/note and points the node at it. */
function MidiLearn({ nodeId, isNote }: { nodeId: string; isNote: boolean }) {
  const run = useShowStore((s) => s.run);
  const [learning, setLearning] = useState(false);

  const learn = async () => {
    setLearning(true);
    // Wait for a message newer than any already logged, not for a different
    // control: moving the knob you touched last must still count.
    const logged = await timing.midiEvents(0).catch(() => []);
    let after = logged.length > 0 ? logged[logged.length - 1].seq : 0;
    const started = Date.now();
    const poll = async () => {
      const events = await timing.midiEvents(after).catch(() => []);
      if (events.length > 0) after = events[events.length - 1].seq;
      const hit = events.find((e) => e.kind === (isNote ? "noteOn" : "cc"));
      if (hit) {
        await run(() => graphApi.setParams(nodeId, { channel: hit.channel, [isNote ? "note" : "controller"]: hit.number }));
        setLearning(false);
      } else if (Date.now() - started > 15_000) {
        setLearning(false);
      } else {
        setTimeout(() => void poll(), 100);
      }
    };
    void poll();
  };

  return (
    <button
      className={cn(
        "nodrag flex h-5 items-center justify-center gap-1 rounded text-[11px]",
        learning ? "animate-pulse bg-primary text-primary-foreground" : "bg-muted hover:bg-muted/70",
      )}
      onClick={() => void learn()}
      title={`Move a ${isNote ? "pad or key" : "knob or fader"} on your MIDI controller (a controller port chosen in Setup)`}
    >
      <Radio className="h-3 w-3" />
      {learning ? "Waiting for MIDI…" : "Learn"}
    </button>
  );
}

function GraphNodeImpl({ data, selected }: NodeProps<GraphFlowNode>) {
  const { view, info, connected, enabledBy } = data;
  const values = useGraphStore((s) => s.values[view.id]);
  const enabledLive = useGraphStore((s) => (enabledBy ? s.values[enabledBy.node]?.[enabledBy.socket]?.[0] : undefined));
  const enabledSocket = view.inputs.find((s) => s.id === ENABLED);
  const enabledParam = Number(view.params[ENABLED] ?? enabledSocket?.default ?? 1) > 0.5;
  const enabled = enabledBy ? (enabledLive ?? 1) > 0.5 : enabledParam;
  const groups: Group[] = useShowStore((s) => s.show?.groups ?? EMPTY_GROUPS);
  const run = useShowStore((s) => s.run);
  const setParam = (id: string, value: ParamValue) => void run(() => graphApi.setParams(view.id, { [id]: value }));
  const color = CATEGORY_COLORS[info?.category ?? ""] ?? "#555";
  const label = view.kind === "fader" || view.kind === "button" ? String(view.params.label ?? info?.label) : info?.label;

  return (
    <div
      className={cn(
        "w-52 rounded-md border bg-card/95 text-[11px] shadow-lg",
        selected ? "border-primary ring-1 ring-primary" : "border-black/40",
        view.issues.length > 0 && "border-destructive",
        !enabled && "opacity-50",
      )}
      title={info?.description}
    >
      <div className="relative flex items-center justify-between gap-2 rounded-t-md px-2 py-1 font-medium text-white" style={{ backgroundColor: color }}>
        {enabledSocket && (
          <Handle
            type="target"
            position={Position.Left}
            id={ENABLED}
            className="!h-2.5 !w-2.5 !border-black/60"
            style={{ background: SOCKET_COLORS[enabledSocket.type] }}
            title="Enabled: off makes everything this node feeds act as if unplugged"
          />
        )}
        <span className="flex min-w-0 items-center gap-1.5">
          {enabledSocket && !enabledBy && (
            <input
              type="checkbox"
              className="nodrag"
              checked={enabledParam}
              onChange={(e) => setParam(ENABLED, e.target.checked ? 1 : 0)}
              title="Enabled: off makes everything this node feeds act as if unplugged"
            />
          )}
          <span className="truncate">{label ?? view.kind}</span>
        </span>
        {view.issues.length > 0 && (
          <span title={view.issues.join("\n")}>
            <AlertTriangle className="h-3.5 w-3.5 text-amber-300" />
          </span>
        )}
      </div>

      <div className="flex flex-col gap-1 py-1.5">
        {view.outputs.map((socket, i) => (
          <div key={socket.id} className="relative flex h-5 items-center justify-end gap-2 px-3">
            <Preview socket={socket} value={values?.[i]} />
            <span>{socket.label}</span>
            <Handle
              type="source"
              position={Position.Right}
              id={socket.id}
              className="!h-2.5 !w-2.5 !border-black/60"
              style={{ background: SOCKET_COLORS[socket.type] }}
            />
          </div>
        ))}

        {info && info.params.length > 0 && (
          <div className="flex flex-col gap-1 px-3">
            {info.params
              .filter((p) => paramShown(view, p.id))
              .map((p) =>
                p.kind === "curve" ? (
                  <CurveEditor
                    key={p.id}
                    value={String(view.params[p.id] ?? p.default)}
                    mode={String(view.params.curveMode ?? "linear") as CurveMode}
                    playhead={values?.[1]?.[0]}
                    onCommit={(v) => setParam(p.id, v)}
                  />
                ) : (
                  <ParamControl key={p.id} info={p} value={view.params[p.id]} groups={groups} onCommit={(v) => setParam(p.id, v)} />
                ),
              )}
            {(view.kind === "midiCc" || view.kind === "midiNote") && <MidiLearn nodeId={view.id} isNote={view.kind === "midiNote"} />}
          </div>
        )}

        {view.inputs.filter((socket) => socket.id !== ENABLED).map((socket) => {
          const linked = connected.includes(socket.id);
          return (
            <div key={socket.id} className="relative flex min-h-5 items-center gap-2 px-3">
              <Handle
                type="target"
                position={Position.Left}
                id={socket.id}
                className="!h-2.5 !w-2.5 !border-black/60"
                style={{ background: SOCKET_COLORS[socket.type] }}
              />
              <span className={cn("shrink-0", linked || socket.default === null ? "" : "w-16 truncate text-muted-foreground")}>{socket.label}</span>
              {!linked && socket.default !== null && (
                <div className="min-w-0 flex-1">
                  <InputValue socket={socket} value={view.params[socket.id]} onCommit={(v) => setParam(socket.id, v)} />
                </div>
              )}
            </div>
          );
        })}
      </div>

      {view.issues.length > 0 && (
        <div className="border-t border-destructive/40 px-2 py-1 text-[10px] leading-snug text-destructive">{view.issues.join(" · ")}</div>
      )}
    </div>
  );
}

export const GraphNode = memo(GraphNodeImpl);
