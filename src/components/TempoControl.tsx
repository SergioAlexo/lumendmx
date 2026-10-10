import { useEffect, useState } from "react";
import * as engineApi from "../lib/engine";
import * as timing from "../lib/timing";
import type { TimingStatus } from "../lib/showfile/generated";
import { Button } from "./ui/button";
import { cn } from "../lib/utils";

const POLL_MS = 50;
const TIMING_POLL_MS = 500;
/** The beat light stays lit for this fraction of each beat. */
const BEAT_FLASH = 0.15;

/** The global tempo (BUILD_PLAN Phase 7): BPM field, TAP, and a light on
 * every beat. BPM-synced effects and pixel maps follow it. */
export function TempoControl() {
  const [bpm, setBpm] = useState(120);
  const [onBeat, setOnBeat] = useState(false);
  const [draft, setDraft] = useState<string | null>(null);
  const [timingStatus, setTimingStatus] = useState<TimingStatus | null>(null);

  useEffect(() => {
    const poll = () => void timing.timingStatus().then(setTimingStatus).catch(() => undefined);
    poll();
    const id = setInterval(poll, TIMING_POLL_MS);
    return () => clearInterval(id);
  }, []);

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
      {timingStatus && (
        <select
          aria-label="Tempo source"
          title={
            timingStatus.settings.tempoSource === "audio"
              ? timingStatus.audio.error ?? (timingStatus.audio.locked ? "Locked to the audio input" : "Listening for a beat…")
              : "Where the tempo comes from (more in the Timing tab)"
          }
          value={timingStatus.settings.tempoSource}
          onChange={(e) =>
            void timing
              .timingConfigure({ ...timingStatus.settings, tempoSource: e.target.value as TimingStatus["settings"]["tempoSource"] })
              .then(setTimingStatus)
              .catch(() => undefined)
          }
          className={cn(
            "h-8 rounded border border-input bg-transparent px-1 text-xs focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
            timingStatus.settings.tempoSource === "audio" && timingStatus.audio.locked && "border-success text-success",
          )}
        >
          {timing.TEMPO_SOURCES.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
            </option>
          ))}
        </select>
      )}
    </div>
  );
}
