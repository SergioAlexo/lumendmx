import { useEffect } from "react";
import { FilePlus, FolderOpen, Redo2, Save, Undo2 } from "lucide-react";
import * as showApi from "../lib/show";
import { useShowStore } from "../store/useShowStore";
import { Button } from "./ui/button";
import { Select } from "./ui/input";

function isEditable(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName));
}

/** Cmd/Ctrl+S saves, Cmd/Ctrl+Z / Shift+Cmd/Ctrl+Z undo and redo. Used by
 * every window, popped-out tabs included. */
export function useShowShortcuts() {
  const run = useShowStore((s) => s.run);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
      const key = e.key.toLowerCase();
      if (key === "s") {
        e.preventDefault();
        void run(() => showApi.saveShow(e.shiftKey));
      } else if (key === "z" && !isEditable(e.target)) {
        e.preventDefault();
        void run(e.shiftKey ? showApi.redo : showApi.undo);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [run]);
}

/** File (New/Open/Save/Save As/Recent) and Undo/Redo controls for the top
 * bar. */
export function ShowMenu() {
  const show = useShowStore((s) => s.show);
  const recent = useShowStore((s) => s.recent);
  const run = useShowStore((s) => s.run);

  return (
    <div className="flex items-center gap-2">
      <Button variant="ghost" size="iconSm" title="New show" onClick={() => void run(showApi.newShow)}>
        <FilePlus />
      </Button>
      <Button variant="ghost" size="iconSm" title="Open show…" onClick={() => void run(() => showApi.openShow())}>
        <FolderOpen />
      </Button>
      <Button variant="ghost" size="iconSm" title="Save (⌘S)" onClick={() => void run(() => showApi.saveShow())}>
        <Save />
      </Button>
      {recent.length > 0 && (
        <Select
          className="w-40"
          title="Recent shows"
          value=""
          onChange={(e) => {
            const path = e.target.value;
            if (path) void run(() => showApi.openShow(path));
          }}
        >
          <option value="" disabled>
            Recent…
          </option>
          {recent.map((entry) => (
            <option key={entry.path} value={entry.path}>
              {entry.name}
            </option>
          ))}
        </Select>
      )}

      <div className="mx-1 h-5 w-px bg-border" />

      <Button
        variant="ghost"
        size="iconSm"
        title={show?.undoLabel ? `Undo ${show.undoLabel} (⌘Z)` : "Undo"}
        disabled={!show?.undoLabel}
        onClick={() => void run(showApi.undo)}
      >
        <Undo2 />
      </Button>
      <Button
        variant="ghost"
        size="iconSm"
        title={show?.redoLabel ? `Redo ${show.redoLabel} (⇧⌘Z)` : "Redo"}
        disabled={!show?.redoLabel}
        onClick={() => void run(showApi.redo)}
      >
        <Redo2 />
      </Button>

      <span className="text-sm text-muted-foreground" title={show?.path ?? "Not saved yet"}>
        {show?.name ?? "Untitled Show"}
        {show?.dirty ? " •" : ""}
      </span>

      <Button variant="ghost" size="sm" onClick={() => void run(() => showApi.saveShow(true))}>
        Save As…
      </Button>
    </div>
  );
}
