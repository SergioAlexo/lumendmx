import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import * as graphApi from "../../lib/graph";
import { useShowStore } from "../../store/useShowStore";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { exposedParam, exposedSockets, socketKey } from "./frames";

export type ExposeTarget = { node: string; side: "in" | "out"; socket: string; x: number; y: number };

/** The right-click popup for a socket of a node inside a frame: expose it
 * on the frame under a name, rename it, or stop exposing it. */
export function ExposeMenu({ target, onClose }: { target: ExposeTarget; onClose: () => void }) {
  const run = useShowStore((s) => s.run);
  const view = useShowStore((s) => s.show?.graph.nodes.find((n) => n.id === target.node));
  const ref = useRef<HTMLFormElement>(null);
  const key = socketKey(target.side, target.socket);
  const socket = (target.side === "in" ? view?.inputs : view?.outputs)?.find((s) => s.id === target.socket);
  const current = view ? exposedSockets(view).get(key) : undefined;
  const exposed = current !== undefined;
  const [name, setName] = useState(current || socket?.label || target.socket);

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    window.addEventListener("pointerdown", onDown);
    return () => window.removeEventListener("pointerdown", onDown);
  }, [onClose]);

  const save = (next: string | null) => {
    if (!view) return;
    const map = exposedSockets(view);
    if (next === null) map.delete(key);
    // The socket's own label is stored as "" so it follows the socket.
    else map.set(key, next.trim() === socket?.label ? "" : next.trim());
    void run(() => graphApi.setParams(target.node, { exposed: exposedParam(map) }));
    onClose();
  };

  // Portalled: inside the canvas, `fixed` would follow its zoom transform.
  return createPortal(
    <form
      ref={ref}
      className="nodrag nopan fixed z-50 flex w-64 flex-col gap-2 rounded-md border border-border bg-card p-3 text-sm shadow-2xl"
      style={{ left: Math.min(target.x, window.innerWidth - 270), top: Math.min(target.y, window.innerHeight - 160) }}
      onSubmit={(e) => {
        e.preventDefault();
        save(name);
      }}
      onContextMenu={(e) => e.preventDefault()}
    >
      <span className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
        {exposed ? "Exposed on the frame" : "Expose on the frame"}
      </span>
      <span className="text-xs text-muted-foreground">
        {target.side === "in" ? "Input" : "Output"} “{socket?.label ?? target.socket}”. Name on the collapsed frame:
      </span>
      <Input
        autoFocus
        className="h-8"
        value={name}
        onFocus={(e) => e.currentTarget.select()}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
        }}
      />
      <div className="flex justify-end gap-2">
        {exposed && (
          <Button type="button" size="sm" variant="ghost" className="mr-auto text-destructive" onClick={() => save(null)}>
            Stop exposing
          </Button>
        )}
        <Button type="button" size="sm" variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button type="submit" size="sm">
          {exposed ? "Save" : "Expose"}
        </Button>
      </div>
    </form>,
    document.body,
  );
}
