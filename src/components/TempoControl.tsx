import { useEffect, useState } from "react";
import * as engineApi from "../lib/engine";
import { Button } from "./ui/button";
import { cn } from "../lib/utils";

const POLL_MS = 50;
/** The beat light stays lit for this fraction of each beat. */
const BEAT_FLASH = 0.15;

/** The global tempo (BUILD_PLAN Phase 7): BPM field, TAP, and a light on
 * every beat. BPM-synced effects and pixel maps follow it. */
export function TempoControl() {
  const [bpm, setBpm] = useState(120);
  const [onBeat, setOnBeat] = useState(false);
  const [draft, setDraft] = useState<string | null>(null);

  useEffect(() => {
    const poll = () =>
      void engineApi
        .tempoStatus()
        .then((status) => {
          setBpm(status.bpm);
          setOnBeat(status.beat % 1 < BEAT_FLASH);
        })
        .catch(() => undefined);
    poll();
    const id = setInterval(poll, POLL_MS);
    return () => clearInterval(id);
  }, []);

  const commit = () => {
    if (draft === null) return;
    const value = Number(draft);
    if (draft.trim() !== "" && Number.isFinite(value)) void engineApi.setBpm(value);
    setDraft(null);
  };

  return (
    <div className="flex items-center gap-1.5" title="Show tempo: BPM-synced effects and pixel maps follow it">
      <span
        className={cn("h-2.5 w-2.5 rounded-full transition-colors", onBeat ? "bg-primary" : "bg-muted-foreground/30")}
      />
      <input
        type="text"
        inputMode="decimal"
        aria-label="BPM"
        value={draft ?? bpm.toFixed(1)}
        onFocus={(e) => {
          setDraft(bpm.toFixed(1));
          e.currentTarget.select();
        }}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          else if (e.key === "Escape") {
            setDraft(null);
            e.currentTarget.blur();
          }
        }}
        className="h-8 w-14 rounded border border-input bg-transparent px-1 text-right text-sm tabular-nums focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
      />
      <span className="text-xs text-muted-foreground">BPM</span>
      <Button
        size="sm"
        variant="outline"
        className="font-semibold"
        // pointerdown, not click: a tap should land when the finger lands.
        onPointerDown={() => void engineApi.tapTempo().then(setBpm).catch(() => undefined)}
      >
        TAP
      </Button>
    </div>
  );
}
