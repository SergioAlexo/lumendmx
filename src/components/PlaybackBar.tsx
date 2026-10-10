import { useEffect, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, Pause, Play, Square, Undo2, X, Zap } from "lucide-react";
import * as engineApi from "../lib/engine";
import type { ConsoleStatus, CuelistDto, FaderMode, PlaybackStatus } from "../lib/engine";
import { Button } from "./ui/button";
import { Select } from "./ui/input";
import { VerticalFader } from "./ui/vertical-fader";
import { cn } from "../lib/utils";

const POLL_MS = 250;
/** Cuelists are created/edited in another tab and there's no change event,
 * so re-fetch them too — less often, since each fetch carries every cue. */
const CUELIST_POLL_MS = 2000;
/** How long a locally-dragged fader value is trusted over the polled one. */
const DRAG_HOLD_MS = 600;

const MODE_ORDER: FaderMode["kind"][] = ["intensityMaster", "crossfade", "rate"];
const MODE_LABEL: Record<FaderMode["kind"], string> = { intensityMaster: "INT", crossfade: "XF", rate: "RATE" };
const MODE_HELP: Record<FaderMode["kind"], string> = {
  intensityMaster: "Intensity master: the fader scales dimmer channels",
  crossfade: "Crossfade: the fader is the position between the previous and current cue",
  rate: "Rate: the fader is this playback's speed (50% is normal, 100% is double)",
};

/** A fader whose value follows the engine, except for a moment after the
 * user moves it (so polling doesn't fight the drag). */
function useHeldValue(remote: number): [number, (v: number) => void] {
  const [local, setLocal] = useState<number | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const set = (v: number) => {
    setLocal(v);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setLocal(null), DRAG_HOLD_MS);
  };
  return [local ?? remote, set];
}

function faderLabel(status: PlaybackStatus, percent: number): string {
  return status.faderMode.kind === "rate" ? `${((percent / 100) * 2).toFixed(2)}x` : `${percent}%`;
}

function Column({
  index,
  status,
  cuelist,
  cuelists,
  page,
  selected,
  onSelect,
  onChanged,
}: {
  index: number;
  status: PlaybackStatus | undefined;
  cuelist: CuelistDto | undefined;
  cuelists: CuelistDto[];
  page: number;
  selected: boolean;
  onSelect: () => void;
  onChanged: () => void;
}) {
  const preFlashFader = useRef<number | null>(null);
  const [fader, setFader] = useHeldValue(Math.round((status?.fader ?? 0) * 100));

  if (!status) {
    return (
      <div className="flex w-28 shrink-0 flex-col gap-1 rounded-md border border-dashed border-border p-2">
        <span className="text-xs text-muted-foreground">Playback {index + 1}</span>
        <Select
          className="h-11 text-xs"
          value=""
          aria-label={`Assign a cuelist to playback ${index + 1}`}
          onChange={(e) => {
            if (e.target.value) void engineApi.assignPlayback(Number(e.target.value), page, index).then(onChanged);
          }}
        >
          <option value="">Assign…</option>
          {cuelists.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </Select>
      </div>
    );
  }

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
  const nextMode = MODE_ORDER[(MODE_ORDER.indexOf(status.faderMode.kind) + 1) % MODE_ORDER.length];

  return (
    <div
      className={cn(
        "flex w-28 shrink-0 flex-col gap-1 rounded-md border p-2",
        selected ? "border-primary" : "border-border",
        status.active && "bg-primary/5",
      )}
    >
      <div className="flex items-start justify-between gap-1">
        <button className="min-w-0 flex-1 text-left" onClick={onSelect} title="Select as the main playback">
          <span className="block truncate text-xs font-medium" title={cuelist?.name}>
            {cuelist?.name ?? `Cuelist ${status.cuelistId}`}
          </span>
          <span className="block text-[11px] tabular-nums text-muted-foreground">
            Cue {status.currentCueNumber ?? "--"}
          </span>
        </button>
        <button
          className="shrink-0 rounded p-0.5 text-muted-foreground hover:bg-accent"
          title="Remove this playback"
          aria-label="Remove playback"
          onClick={() => void engineApi.deletePlayback(status.id).then(onChanged)}
        >
          <X className="size-3" />
        </button>
      </div>

      <Button size="touch" className="w-full" title="GO" onClick={() => void engineApi.playbackGo(status.id).then(onChanged)}>
        GO
      </Button>
      <div className="grid grid-cols-2 gap-1">
        <Button size="touch" variant="secondary" title="Back" onClick={() => void engineApi.playbackGoBack(status.id).then(onChanged)}>
          <Undo2 />
        </Button>
        <Button
          size="touch"
          variant={status.paused ? "default" : "secondary"}
          title={status.paused ? "Resume" : "Pause"}
          onClick={() => void engineApi.playbackSetPaused(status.id, !status.paused).then(onChanged)}
        >
          {status.paused ? <Play /> : <Pause />}
        </Button>
        <Button
          size="touch"
          variant="secondary"
          title="Flash (hold to bump the fader to full, release to restore it)"
          onPointerDown={startFlash}
          onPointerUp={endFlash}
          onPointerLeave={endFlash}
          onPointerCancel={endFlash}
        >
          <Zap />
        </Button>
        <Button size="touch" variant="secondary" title="Release" onClick={() => void engineApi.playbackRelease(status.id).then(onChanged)}>
          <Square />
        </Button>
      </div>
      <div className="flex items-stretch gap-1">
        <VerticalFader
          className="h-24 flex-1"
          value={fader}
          label={faderLabel(status, fader)}
          title={`Fader — ${MODE_HELP[status.faderMode.kind]}`}
          onChange={(v) => {
            setFader(v);
            void engineApi.playbackSetFader(status.id, v);
          }}
        />
        <button
          className="w-9 shrink-0 rounded border border-border text-[10px] font-semibold uppercase hover:bg-accent"
          title={`${MODE_HELP[status.faderMode.kind]}. Click for ${MODE_LABEL[nextMode]}.`}
          onClick={() => void engineApi.playbackSetFaderMode(status.id, { kind: nextMode }).then(onChanged)}
        >
          {MODE_LABEL[status.faderMode.kind]}
        </button>
      </div>
    </div>
  );
}

function MasterFader({
  label,
  title,
  percent,
  max,
  format,
  onChange,
}: {
  label: string;
  title: string;
  percent: number;
  max: number;
  format: (percent: number) => string;
  onChange: (percent: number) => void;
}) {
  const [value, setValue] = useHeldValue(Math.round(percent));
  return (
    <div className="flex w-14 shrink-0 flex-col items-center gap-1">
      <span className="text-[10px] font-semibold uppercase text-muted-foreground" title={title}>
        {label}
      </span>
      <VerticalFader
        className="h-40"
        value={value}
        max={max}
        label={format(value)}
        title={title}
        onChange={(v) => {
          setValue(v);
          onChange(v);
        }}
      />
    </div>
  );
}

/** The playback bar (BUILD_PLAN Phase 6, section 2): a page selector, the
 * main playback (GO / BACK / PAUSE / RELEASE ALL acting on the selected
 * column), the grand master and rate master, and ten playback columns for
 * the current page. Playback state lives in the Rust engine; this polls it
 * (see DECISIONS.md) and only sends commands. */
export function PlaybackBar() {
  const [statuses, setStatuses] = useState<PlaybackStatus[]>([]);
  const [cuelists, setCuelists] = useState<CuelistDto[]>([]);
  const [consoleState, setConsole] = useState<ConsoleStatus | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const refresh = () => {
    void engineApi.listPlaybacks().then(setStatuses).catch(() => undefined);
    void engineApi.consoleStatus().then(setConsole).catch(() => undefined);
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

  const page = consoleState?.page ?? 0;
  const pageCount = consoleState?.pageCount ?? 40;
  const perPage = consoleState?.playbacksPerPage ?? 10;
  const onPage = statuses.filter((s) => s.page === page);
  const main = onPage.find((s) => s.id === selectedId) ?? onPage[0];
  const gotoPage = (next: number) => {
    const clamped = Math.min(pageCount - 1, Math.max(0, next));
    void engineApi.setPage(clamped).then(refresh);
  };

  return (
    <div className="flex items-stretch gap-3 overflow-x-auto border-t border-border bg-card p-2">
      <div className="flex w-60 shrink-0 flex-col gap-2">
        <div className="flex items-center gap-1">
          <Button size="touch" variant="secondary" aria-label="Previous page" disabled={page <= 0} onClick={() => gotoPage(page - 1)}>
            <ChevronLeft />
          </Button>
          <span className="flex-1 text-center text-sm font-semibold tabular-nums">
            Page {page + 1}
            <span className="text-xs font-normal text-muted-foreground"> / {pageCount}</span>
          </span>
          <Button size="touch" variant="secondary" aria-label="Next page" disabled={page >= pageCount - 1} onClick={() => gotoPage(page + 1)}>
            <ChevronRight />
          </Button>
        </div>
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          <input
            type="checkbox"
            checked={consoleState?.releaseOnPageChange ?? false}
            onChange={(e) => void engineApi.setReleaseOnPageChange(e.target.checked).then(refresh)}
          />
          Release on page change
        </label>

        <div className="rounded-md border border-border p-2">
          <div className="mb-1 truncate text-[11px] font-semibold uppercase text-muted-foreground">
            Main playback{main ? ` · ${cuelists.find((c) => c.id === main.cuelistId)?.name ?? ""}` : ""}
          </div>
          <div className="grid grid-cols-2 gap-1">
            <Button
              size="touchLg"
              className="col-span-2"
              disabled={!main}
              title="GO the selected playback"
              onClick={() => main && void engineApi.playbackGo(main.id).then(refresh)}
            >
              GO
            </Button>
            <Button
              size="touch"
              variant="secondary"
              disabled={!main}
              title="BACK: previous cue on the selected playback"
              onClick={() => main && void engineApi.playbackGoBack(main.id).then(refresh)}
            >
              BACK
            </Button>
            <Button
              size="touch"
              variant={main?.paused ? "default" : "secondary"}
              disabled={!main}
              title="PAUSE / resume the selected playback"
              onClick={() => main && void engineApi.playbackSetPaused(main.id, !main.paused).then(refresh)}
            >
              {main?.paused ? "RESUME" : "PAUSE"}
            </Button>
            <Button
              size="touch"
              variant="destructive"
              className="col-span-2"
              title="Release every playback on every page"
              onClick={() => void engineApi.playbackReleaseAll().then(refresh)}
            >
              RELEASE ALL
            </Button>
          </div>
        </div>
      </div>

      <MasterFader
        label="GM"
        title="Grand master: scales every intensity channel of the output"
        percent={consoleState?.grandMaster ?? 100}
        max={100}
        format={(v) => `${v}%`}
        onChange={(v) => void engineApi.setGrandMaster(v)}
      />
      <MasterFader
        label="Rate"
        title="Rate master: speeds up or slows down chases, effects and pixel maps (100% is normal)"
        percent={consoleState?.rateMaster ?? 100}
        max={200}
        format={(v) => `${(v / 100).toFixed(2)}x`}
        onChange={(v) => void engineApi.setRateMaster(v)}
      />

      {Array.from({ length: perPage }, (_, i) => {
        const status = onPage.find((s) => s.column === i);
        return (
          <Column
            key={`${page}-${i}`}
            index={i}
            status={status}
            cuelist={status && cuelists.find((c) => c.id === status.cuelistId)}
            cuelists={cuelists}
            page={page}
            selected={status !== undefined && status.id === main?.id}
            onSelect={() => status && setSelectedId(status.id)}
            onChanged={refresh}
          />
        );
      })}
    </div>
  );
}
