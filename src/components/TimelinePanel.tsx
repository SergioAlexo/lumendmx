import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { FileAudio, Pause, Play, Plus, Square } from "lucide-react";
import * as engineApi from "../lib/engine";
import * as timing from "../lib/timing";
import type { CueDto, CuelistDto, TimecodeStatus } from "../lib/showfile/generated";
import { Button } from "./ui/button";
import { Select } from "./ui/input";
import { cn } from "../lib/utils";

const POLL_MS = 100;
const CUELIST_POLL_MS = 1500;
const DEFAULT_PPS = 40;
const MIN_LENGTH_MS = 60_000;
const TAIL_MS = 15_000;
const RULER_H = 24;
const WAVE_H = 96;
const CUE_H = 40;
/** The audio is re-seeked when it drifts this far from the timecode clock. */
const AUDIO_DRIFT_S = 0.2;
/** Cue times snap to this. */
const SNAP_MS = 10;
/** Waveform resolution. */
const PEAKS_PER_SEC = 100;

interface Waveform {
  peaks: Float32Array;
  durationMs: number;
}

async function decodeWaveform(file: File): Promise<Waveform> {
  const ctx = new AudioContext();
  try {
    const buffer = await ctx.decodeAudioData(await file.arrayBuffer());
    const data = buffer.getChannelData(0);
    const count = Math.max(1, Math.ceil(buffer.duration * PEAKS_PER_SEC));
    const per = Math.max(1, Math.floor(data.length / count));
    const peaks = new Float32Array(count);
    for (let i = 0; i < count; i++) {
      let max = 0;
      for (let j = i * per; j < Math.min(data.length, (i + 1) * per); j++) max = Math.max(max, Math.abs(data[j]));
      peaks[i] = max;
    }
    return { peaks, durationMs: buffer.duration * 1000 };
  } finally {
    void ctx.close();
  }
}

function WaveformCanvas({ waveform, width, className }: { waveform: Waveform | null; width: number; className?: string }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current;
    const g = canvas?.getContext("2d");
    if (!canvas || !g) return;
    g.clearRect(0, 0, canvas.width, canvas.height);
    if (!waveform) return;
    // Read the theme colour from CSS so light and dark both work.
    g.fillStyle = getComputedStyle(canvas).color;
    const mid = WAVE_H / 2;
    const columns = Math.min(width, 16000);
    const scale = waveform.peaks.length / columns;
    for (let x = 0; x < columns; x++) {
      let peak = 0;
      for (let i = Math.floor(x * scale); i < Math.min(waveform.peaks.length, Math.ceil((x + 1) * scale)); i++) {
        peak = Math.max(peak, waveform.peaks[i]);
      }
      const h = Math.max(1, peak * (WAVE_H - 4));
      g.fillRect((x / columns) * width, mid - h / 2, Math.max(1, width / columns), h);
    }
  }, [waveform, width]);
  return <canvas ref={ref} width={Math.min(width, 16000)} height={WAVE_H} style={{ width }} className={className} />;
}

/** The Show timeline (BUILD_PLAN Phase 8): an audio waveform, a Timecode
 * cuelist's cues as draggable blocks, and a playhead on the engine's
 * timecode clock. With the internal clock, Play also plays the audio. */
export function TimelinePanel() {
  const [cuelists, setCuelists] = useState<CuelistDto[]>([]);
  const [listId, setListId] = useState<number | null>(null);
  const [tc, setTc] = useState<TimecodeStatus | null>(null);
  const [pps, setPps] = useState(DEFAULT_PPS);
  const [waveform, setWaveform] = useState<Waveform | null>(null);
  const [audioName, setAudioName] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [drag, setDrag] = useState<{ number: number; ms: number } | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const audioUrl = useRef<string | null>(null);
  const trackRef = useRef<HTMLDivElement>(null);

  const refreshLists = useCallback(() => {
    void engineApi
      .listCuelists()
      .then((lists) => {
        const timed = lists.filter((l) => l.kind.kind === "timecode");
        setCuelists(timed);
        setListId((current) => (current !== null && timed.some((l) => l.id === current) ? current : (timed[0]?.id ?? null)));
      })
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    refreshLists();
    const id = setInterval(refreshLists, CUELIST_POLL_MS);
    return () => clearInterval(id);
  }, [refreshLists]);

  useEffect(() => {
    const poll = () => void timing.timingStatus().then((s) => setTc(s.timecode)).catch(() => undefined);
    poll();
    const id = setInterval(poll, POLL_MS);
    return () => clearInterval(id);
  }, []);

  // Keep the audio element on the timecode clock.
  useEffect(() => {
    const audio = audioRef.current;
    if (!audio || !tc) return;
    const target = tc.positionMs / 1000;
    if (tc.active) {
      if (Math.abs(audio.currentTime - target) > AUDIO_DRIFT_S) audio.currentTime = target;
      if (audio.paused) void audio.play().catch(() => undefined);
    } else {
      if (!audio.paused) audio.pause();
      if (Math.abs(audio.currentTime - target) > 0.01) audio.currentTime = target;
    }
  }, [tc]);

  useEffect(
    () => () => {
      audioRef.current?.pause();
      if (audioUrl.current) URL.revokeObjectURL(audioUrl.current);
    },
    [],
  );

  const loadAudio = async (file: File) => {
    setError(null);
    try {
      setWaveform(await decodeWaveform(file));
      if (audioUrl.current) URL.revokeObjectURL(audioUrl.current);
      audioUrl.current = URL.createObjectURL(file);
      audioRef.current?.pause();
      audioRef.current = new Audio(audioUrl.current);
      setAudioName(file.name);
    } catch (e) {
      setError(`Couldn't read that audio file: ${String(e)}`);
    }
  };

  const list = cuelists.find((l) => l.id === listId) ?? null;
  const timeOf = (cue: CueDto) => (drag?.number === cue.number ? drag.ms : cue.timecodeMs);
  const lastCue = Math.max(0, ...(list?.cues ?? []).map((c) => c.timecodeMs ?? 0));
  const lengthMs = Math.max(MIN_LENGTH_MS, waveform?.durationMs ?? 0, lastCue + TAIL_MS, (tc?.positionMs ?? 0) + TAIL_MS);
  const width = Math.ceil((lengthMs / 1000) * pps);
  const internal = tc?.source === "internal";

  const ticks = useMemo(() => {
    const step = pps >= 80 ? 1 : pps >= 30 ? 5 : pps >= 12 ? 10 : 30;
    return Array.from({ length: Math.floor(lengthMs / 1000 / step) + 1 }, (_, i) => i * step);
  }, [pps, lengthMs]);

  const msAt = (clientX: number) => {
    const rect = trackRef.current?.getBoundingClientRect();
    if (!rect) return 0;
    return Math.max(0, Math.round(((clientX - rect.left) / pps) * 1000));
  };

  const seek = (ms: number) => {
    if (!internal) return;
    void timing.timecodeSeek(ms);
  };

  const place = (cue: CueDto, ms: number | null) => {
    if (!list) return;
    void timing.setCueTimecode(list.id, cue.number, ms === null ? null : Math.round(ms / SNAP_MS) * SNAP_MS).then(refreshLists);
  };

  const createList = () =>
    void engineApi
      .createCuelist("Timeline", { kind: "timecode" }, false)
      .then((id) => {
        setListId(id);
        refreshLists();
      })
      .catch((e) => setError(String(e)));

  const unplaced = (list?.cues ?? []).filter((c) => c.timecodeMs === null);

  return (
    <div className="flex h-full flex-col gap-3 overflow-hidden p-4">
      <div className="flex flex-wrap items-center gap-2">
        <Button size="iconSm" variant="outline" title="Play" disabled={!internal} onClick={() => void timing.timecodePlay()}>
          <Play />
        </Button>
        <Button size="iconSm" variant="outline" title="Pause" disabled={!internal} onClick={() => void timing.timecodePause()}>
          <Pause />
        </Button>
        <Button size="iconSm" variant="outline" title="Stop and rewind" disabled={!internal} onClick={() => void timing.timecodeStop()}>
          <Square />
        </Button>
        <span className="w-32 font-mono text-lg tabular-nums">{timing.formatTimecode(tc?.positionMs ?? 0, tc?.fps ?? 25)}</span>
        {!internal && (
          <span className="text-xs text-muted-foreground">
            Following {tc?.source === "mtc" ? "MIDI timecode" : "audio timecode"} ({tc?.active ? "running" : "no signal"}). Change the
            source in the Timing tab to use the transport.
          </span>
        )}
        <div className="flex-1" />
        <Select className="w-48" value={listId ?? ""} onChange={(e) => setListId(Number(e.target.value))} disabled={cuelists.length === 0}>
          {cuelists.length === 0 && <option value="">No Timecode cuelist</option>}
          {cuelists.map((l) => (
            <option key={l.id} value={l.id}>
              {l.name}
            </option>
          ))}
        </Select>
        <Button size="sm" variant="outline" onClick={createList}>
          <Plus /> Timecode cuelist
        </Button>
        <label className="inline-flex cursor-pointer">
          <input
            type="file"
            accept="audio/*"
            className="sr-only"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) void loadAudio(file);
              e.target.value = "";
            }}
          />
          <span className="inline-flex h-8 items-center gap-2 rounded-md border border-input px-3 text-xs font-medium shadow-sm hover:bg-accent">
            <FileAudio className="size-4" /> {audioName ?? "Load audio…"}
          </span>
        </label>
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          Zoom
          <input type="range" min={5} max={200} value={pps} onChange={(e) => setPps(Number(e.target.value))} />
        </label>
      </div>
      {error && <div className="text-sm text-destructive">{error}</div>}

      <div className="relative flex-1 overflow-auto rounded-md border border-border bg-card">
        <div ref={trackRef} className="relative select-none" style={{ width, height: RULER_H + WAVE_H + CUE_H + 24 }}>
          {/* Ruler: click to scrub. */}
          <div
            className="absolute inset-x-0 top-0 cursor-pointer border-b border-border bg-muted/50"
            style={{ height: RULER_H }}
            onPointerDown={(e) => seek(msAt(e.clientX))}
          >
            {ticks.map((t) => (
              <div key={t} className="absolute top-0 h-full border-l border-border pl-1 text-[10px] text-muted-foreground" style={{ left: t * pps }}>
                {timing.formatTimecode(t * 1000, 1).slice(0, 8)}
              </div>
            ))}
          </div>

          {/* Audio track. */}
          <div
            className="absolute inset-x-0 cursor-pointer border-b border-border text-primary/70"
            style={{ top: RULER_H, height: WAVE_H }}
            onPointerDown={(e) => seek(msAt(e.clientX))}
          >
            {waveform ? (
              <WaveformCanvas waveform={waveform} width={Math.ceil((waveform.durationMs / 1000) * pps)} />
            ) : (
              <div className="p-3 text-xs text-muted-foreground">Load an audio file to see its waveform.</div>
            )}
          </div>

          {/* Cue track. */}
          <div className="absolute inset-x-0" style={{ top: RULER_H + WAVE_H + 8, height: CUE_H }}>
            {(list?.cues ?? []).map((cue) => {
              const t = timeOf(cue);
              if (t === null) return null;
              return (
                <div
                  key={cue.number}
                  className={cn(
                    "absolute top-0 flex h-full min-w-10 cursor-ew-resize items-center overflow-hidden rounded border border-primary bg-primary/20 px-1.5 text-xs",
                    drag?.number === cue.number && "bg-primary/40",
                  )}
                  style={{ left: (t / 1000) * pps }}
                  title={`Cue ${cue.number} ${cue.name} @ ${timing.formatTimecode(t)} (double-click to clear)`}
                  onDoubleClick={() => place(cue, null)}
                  onPointerDown={(e) => {
                    e.stopPropagation();
                    e.currentTarget.setPointerCapture(e.pointerId);
                    setDrag({ number: cue.number, ms: t });
                  }}
                  onPointerMove={(e) => {
                    if (drag?.number === cue.number) setDrag({ number: cue.number, ms: Math.round(msAt(e.clientX) / SNAP_MS) * SNAP_MS });
                  }}
                  onPointerUp={() => {
                    if (drag?.number === cue.number) place(cue, drag.ms);
                    setDrag(null);
                  }}
                >
                  <span className="truncate">
                    <strong>{cue.number}</strong> {cue.name}
                  </span>
                </div>
              );
            })}
            {list && list.cues.length === 0 && (
              <div className="p-2 text-xs text-muted-foreground">
                No cues yet: record cues into this cuelist from the Cuelists tab, then place them here.
              </div>
            )}
          </div>

          {/* Playhead. */}
          <div
            className="pointer-events-none absolute top-0 h-full w-px bg-destructive"
            style={{ left: ((tc?.positionMs ?? 0) / 1000) * pps }}
          />
        </div>
      </div>

      {unplaced.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <span className="text-muted-foreground">Not on the timeline (place at the playhead):</span>
          {unplaced.map((c) => (
            <Button key={c.number} size="sm" variant="outline" onClick={() => place(c, tc?.positionMs ?? 0)}>
              {c.number} {c.name}
            </Button>
          ))}
        </div>
      )}
    </div>
  );
}
