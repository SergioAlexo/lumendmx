import { Power } from "lucide-react";
import { useEffect, useState } from "react";
import * as dmx from "../lib/dmx";
import { describeTrigger, isPress } from "../lib/midiLearn";
import * as timing from "../lib/timing";
import type { MidiTrigger, TimingStatus } from "../lib/showfile/generated";
import { MidiBindingMenu } from "./MidiBindingMenu";
import { useDmxStore } from "../store/useDmxStore";
import { EMPTY_UNIVERSES, useShowStore } from "../store/useShowStore";
import { useUniverseStatusesStore } from "../store/useUniverseStatusesStore";
import { Button } from "./ui/button";
import { cn } from "../lib/utils";
import { ShowMenu } from "./ShowMenu";
import { TempoControl } from "./TempoControl";

function statusColor(connected: boolean, hasDriver: boolean): string {
  if (!hasDriver) return "bg-muted-foreground/40";
  return connected ? "bg-success" : "bg-destructive";
}

/** One dot per universe: green when its controller is sending, red when it
 * failed, grey when it has no output. Controllers are set up in Setup. */
function UniverseStatusDots() {
  const universes = useShowStore((s) => s.show?.universes ?? EMPTY_UNIVERSES);
  const statuses = useUniverseStatusesStore((s) => s.statuses);

  return (
    <div className="flex items-center gap-1.5">
      {universes.map((universe) => {
        const status = statuses[universe.id];
        const hasDriver = universe.driver.kind !== "null";
        const title = `${universe.name} (${universe.driver.kind})${
          status ? ` — ${status.connected ? `${status.framesPerSec.toFixed(0)} fps` : status.error ?? "not connected"}` : ""
        }`;
        return (
          <span
            key={universe.id}
            className={cn("h-2.5 w-2.5 rounded-full", statusColor(status?.connected ?? false, hasDriver))}
            title={title}
          />
        );
      })}
    </div>
  );
}

function GrandMaster() {
  const grandMaster = useDmxStore((s) => s.grandMaster);
  const setGrandMaster = useDmxStore((s) => s.setGrandMaster);
  return (
    <label className="flex items-center gap-2 text-xs text-muted-foreground" title="Grand master: scales every intensity channel">
      GM
      <input
        type="range"
        min={0}
        max={100}
        value={grandMaster}
        onChange={(e) => setGrandMaster(Number(e.target.value))}
        className="w-24 accent-primary"
        aria-label="Grand master"
      />
      <span className="w-8 tabular-nums text-foreground">{grandMaster}%</span>
    </label>
  );
}

/** Blackout, with a right-click menu to bind it to a MIDI pad or button (saved
 * per machine with the timing settings, like TAP). A bound pad toggles it in
 * Rust, so the state is polled. */
function BlackoutButton() {
  const blackout = useDmxStore((s) => s.blackout);
  const toggleBlackout = useDmxStore((s) => s.toggleBlackout);
  const [status, setStatus] = useState<TimingStatus | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const binding = status?.settings.blackoutMidi ?? null;

  useEffect(() => {
    void timing.timingStatus().then(setStatus).catch(() => undefined);
    const poll = () => void dmx.getBlackout().then((b) => useDmxStore.setState({ blackout: b })).catch(() => undefined);
    const id = setInterval(poll, 300);
    return () => clearInterval(id);
  }, []);

  const bind = (blackoutMidi: MidiTrigger | null) => {
    if (!status) return;
    void timing
      .timingConfigure({ ...status.settings, blackoutMidi })
      .then(setStatus)
      .catch(() => undefined);
  };

  return (
    <>
      <Button
        variant={blackout ? "destructive" : "outline"}
        onClick={() => void toggleBlackout()}
        onContextMenu={(e) => {
          e.preventDefault();
          setMenu({ x: e.clientX, y: e.clientY });
        }}
        title={`Blackout${binding ? ` (also ${describeTrigger(binding)})` : ""}. Right-click to bind a MIDI pad.`}
        className={cn(
          "relative font-bold uppercase tracking-wider",
          blackout ? "shadow-lg shadow-destructive/40" : "text-destructive border-destructive/60 hover:bg-destructive/10 hover:text-destructive",
        )}
      >
        <Power />
        Blackout
        {binding && <span className="absolute right-1 top-1 h-1.5 w-1.5 rounded-full bg-current" />}
      </Button>
      {menu && (
        <MidiBindingMenu
          x={menu.x}
          y={menu.y}
          title="Blackout from MIDI"
          binding={binding}
          accept={isPress}
          hint="Press a pad or button…"
          noPorts={status?.settings.controllerPorts.length === 0}
          onBind={bind}
          onClose={() => setMenu(null)}
        />
      )}
    </>
  );
}

export function TopBar() {
  return (
    <header className="flex items-center gap-4 border-b border-border bg-card px-4 py-3">
      <h1 className="text-lg font-semibold tracking-tight">
        Lumen<span className="text-primary">DMX</span>
      </h1>

      <ShowMenu />

      <div className="flex flex-1 items-center gap-3">
        <UniverseStatusDots />
      </div>

      <TempoControl />

      <GrandMaster />

      <BlackoutButton />
    </header>
  );
}
