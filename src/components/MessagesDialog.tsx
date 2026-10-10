import { useMemo, useState } from "react";
import { Download, Trash2, X } from "lucide-react";
import * as reliability from "../lib/reliability";
import { assembleCurrentShow } from "../store/useShowStore";
import { useMessagesStore } from "../store/useMessagesStore";
import { cn } from "../lib/utils";
import { Button } from "./ui/button";
import { Select } from "./ui/input";

type Filter = "all" | "problems" | "error";

const LEVEL_STYLE: Record<string, string> = {
  error: "text-destructive",
  warn: "text-primary",
  info: "text-muted-foreground",
};

/** The in-app Messages window: recent log entries from the engine, outputs
 * and UI, newest last, plus diagnostics export. */
export function MessagesDialog() {
  const open = useMessagesStore((s) => s.open);
  const entries = useMessagesStore((s) => s.entries);
  const setOpen = useMessagesStore((s) => s.setOpen);
  const clear = useMessagesStore((s) => s.clear);
  const [filter, setFilter] = useState<Filter>("all");
  const [status, setStatus] = useState<string | null>(null);

  const shown = useMemo(
    () =>
      entries.filter((e) =>
        filter === "all" ? true : filter === "error" ? e.level === "error" : e.level !== "info",
      ),
    [entries, filter],
  );

  if (!open) return null;

  const exportDiagnostics = async () => {
    setStatus("Exporting…");
    try {
      const path = await reliability.diagnosticsExport(await assembleCurrentShow());
      setStatus(path ? `Saved ${path}` : null);
    } catch (err) {
      setStatus(`Export failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center bg-background/70 p-4 sm:items-center">
      <div className="flex max-h-[80vh] w-full max-w-3xl flex-col rounded-lg border border-border bg-card shadow-xl">
        <div className="flex items-center gap-2 border-b border-border px-4 py-3">
          <h2 className="text-sm font-semibold uppercase tracking-wide">Messages</h2>
          <Select className="ml-2 w-36" value={filter} onChange={(e) => setFilter(e.target.value as Filter)}>
            <option value="all">All</option>
            <option value="problems">Warnings and errors</option>
            <option value="error">Errors only</option>
          </Select>
          <div className="flex-1" />
          <Button size="sm" variant="outline" onClick={() => void exportDiagnostics()}>
            <Download />
            Export diagnostics
          </Button>
          <Button size="iconSm" variant="ghost" title="Clear messages" onClick={() => void clear()}>
            <Trash2 />
          </Button>
          <Button size="iconSm" variant="ghost" title="Close" onClick={() => setOpen(false)}>
            <X />
          </Button>
        </div>
        {status && <p className="border-b border-border px-4 py-1.5 text-xs text-muted-foreground">{status}</p>}
        <div className="flex-1 overflow-y-auto px-4 py-2 font-mono text-xs">
          {shown.length === 0 && <p className="py-6 text-center text-muted-foreground">Nothing to show.</p>}
          {shown.map((e) => (
            <div key={e.id} className="flex gap-3 py-0.5">
              <span className="shrink-0 text-muted-foreground">{e.timestamp.slice(11, 19)}</span>
              <span className={cn("w-10 shrink-0 uppercase", LEVEL_STYLE[e.level])}>{e.level}</span>
              <span className="shrink-0 text-muted-foreground">{e.source}</span>
              <span className="break-all">{e.message}</span>
            </div>
          ))}
        </div>
        <p className="border-t border-border px-4 py-2 text-xs text-muted-foreground">
          Logs rotate daily and the last seven days are kept in the app data folder.
        </p>
      </div>
    </div>
  );
}
