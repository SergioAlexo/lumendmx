import { useEffect, useRef, useState } from "react";
import { Pause, Play, Redo2, Square, Trash2, Undo2, Zap } from "lucide-react";
import * as engineApi from "../lib/engine";
import type { CuelistDto, PlaybackStatus } from "../lib/engine";
import { Button } from "./ui/button";
import { ValueSlider } from "./ui/value-slider";
import { Select } from "./ui/input";
import { cn } from "../lib/utils";

const POLL_MS = 250;
/** Cuelists are created/edited in another tab and there's no change event,
 * so re-fetch them too — less often, since each fetch carries every cue. */
const CUELIST_POLL_MS = 2000;

function Slot({
  status,
  cuelist,
  onChanged,
}: {
  status: PlaybackStatus;
  cuelist: CuelistDto | undefined;
  onChanged: () => void;
}) {
  const preFlashFader = useRef<number | null>(null);

  const startFlash = () => {
    if (preFlashFader.current !== null) return;
    preFlashFader.current = status.fader;
    void engineApi.playbackSetFader(status.id, 100).then(onChanged);
  };
  const endFlash = () => {
    if (preFlashFader.current === null) return;
    void engineApi.playbackSetFader(status.id, Math.round(preFlashFader.current * 100)).then(onChanged);
    preFlashFader.current = null;
  };

  return (
    <div className="flex w-56 shrink-0 flex-col gap-1 rounded-md border border-border p-2">
      <div className="flex items-center justify-between text-xs">
        <span className="truncate font-medium" title={cuelist?.name}>
          {cuelist?.name ?? `Cuelist ${status.cuelistId}`}
        </span>
        <Button size="iconSm" variant="ghost" onClick={() => void engineApi.deletePlayback(status.id).then(onChanged)}>
          <Trash2 />
        </Button>
      </div>
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span>Cue {status.currentCueNumber ?? "--"}</span>
        <button
          className={cn("rounded border border-border px-1 uppercase hover:bg-accent")}
          title="Fader mode: Intensity Master scales only dimmer channels; Crossfade drives the position between cues"
          onClick={() =>
            void engineApi
              .playbackSetFaderMode(
                status.id,
                status.faderMode.kind === "intensityMaster" ? { kind: "crossfade" } : { kind: "intensityMaster" },
              )
              .then(onChanged)
          }
        >
          {status.faderMode.kind === "intensityMaster" ? "IM" : "XF"}
        </button>
      </div>
      <div className="flex items-center gap-1">
        <Button size="iconSm" variant="secondary" title="Back" onClick={() => void engineApi.playbackGoBack(status.id).then(onChanged)}>
          <Undo2 />
        </Button>
        <Button size="sm" className="flex-1" title="GO" onClick={() => void engineApi.playbackGo(status.id).then(onChanged)}>
          GO
        </Button>
        <Button
          size="iconSm"
          variant="secondary"
          title={status.paused ? "Resume" : "Pause"}
          onClick={() => void engineApi.playbackSetPaused(status.id, !status.paused).then(onChanged)}
        >
          {status.paused ? <Play /> : <Pause />}
        </Button>
        <Button size="iconSm" variant="secondary" title="Release" onClick={() => void engineApi.playbackRelease(status.id).then(onChanged)}>
          <Square />
        </Button>
        <Button
          size="iconSm"
          variant="secondary"
          title="Flash (hold to bump the fader to full, release to restore it)"
          onMouseDown={startFlash}
          onMouseUp={endFlash}
          onMouseLeave={endFlash}
        >
          <Zap />
        </Button>
      </div>
      <ValueSlider
        max={100}
        value={Math.round(status.fader * 100)}
        onChange={(v) => void engineApi.playbackSetFader(status.id, v).then(onChanged)}
        title="Fader"
      />
    </div>
  );
}

/** A persistent row of running cue playbacks (BUILD_PLAN Phase 6), docked
 * below the main view so GO/Back/Pause/Release/Flash/fader/fader-mode stay
 * reachable regardless of which tab is open — a real console doesn't hide
 * its faders behind a cue-editing screen. Polls `listPlaybacks` on an
 * interval rather than a push event: unlike DMX frames there's no existing
 * `engine://...`-style event for playback status, and Phase 6 doesn't need
 * sub-poll-interval UI latency to be useful (see DECISIONS.md).
 *
 * A single scrollable row rather than paged banks: BUILD_PLAN asks for
 * "pages (at least 20)" and a distinct "main playback" with GO/BACK/PAUSE/
 * RELEASE ALL — a real windowed paging system and a master-playback concept
 * are more UI infrastructure than this phase's "does cue playback actually
 * work" goal needs; see DECISIONS.md. */
export function PlaybackBar() {
  const [statuses, setStatuses] = useState<PlaybackStatus[]>([]);
  const [cuelists, setCuelists] = useState<CuelistDto[]>([]);
  const [newCuelistId, setNewCuelistId] = useState<number | null>(null);

  const refresh = () => {
    void engineApi.listPlaybacks().then(setStatuses).catch(() => undefined);
  };
  const refreshCuelists = () => void engineApi.listCuelists().then(setCuelists).catch(() => undefined);

  useEffect(() => {
    refresh();
    refreshCuelists();
    const id = setInterval(refresh, POLL_MS);
    const cuelistId = setInterval(refreshCuelists, CUELIST_POLL_MS);
    return () => {
      clearInterval(id);
      clearInterval(cuelistId);
    };
  }, []);

  const addPlayback = () => {
    if (newCuelistId === null) return;
    void engineApi.createPlayback(newCuelistId).then(refresh);
  };

  return (
    <div className="flex items-center gap-2 overflow-x-auto border-t border-border bg-card p-2">
      <div className="flex shrink-0 items-center gap-1">
        <Select
          className="w-40"
          value={newCuelistId ?? ""}
          onFocus={refreshCuelists}
          onChange={(e) => setNewCuelistId(e.target.value ? Number(e.target.value) : null)}
        >
          <option value="">Add playback…</option>
          {cuelists.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </Select>
        <Button size="sm" disabled={newCuelistId === null} onClick={addPlayback}>
          <Redo2 /> Add
        </Button>
      </div>
      {statuses.map((s) => (
        <Slot key={s.id} status={s} cuelist={cuelists.find((c) => c.id === s.cuelistId)} onChanged={refresh} />
      ))}
      {statuses.length === 0 && <p className="text-xs text-muted-foreground">No active playbacks.</p>}
    </div>
  );
}
