import { Power } from "lucide-react";
import { useEffect, useState } from "react";
import * as dmx from "../lib/dmx";
import * as engine from "../lib/engine";
import { describeTrigger, isPress } from "../lib/midiLearn";
import * as timing from "../lib/timing";
import type { BlackoutMode, GrandMasterMidi, MidiTrigger, TimingStatus } from "../lib/showfile/generated";
import { MidiBindingMenu, SlotTabs, TravelField } from "./MidiBindingMenu";
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

type GmSlot = "knob" | "up" | "down" | "sweep";
const GM_SLOTS: Record<GmSlot, string> = { knob: "Knob", up: "Up", down: "Down", sweep: "Sweep" };

/** Grand master, with a right-click menu to bind it like a Fader: a knob (CC), Up and Down buttons
 * held to move it, or one Sweep pad. Saved per machine with the timing settings. A bound control
 * moves it in Rust, so the value is polled. */
function GrandMaster() {
  const grandMaster = useDmxStore((s) => s.grandMaster);
  const setGrandMaster = useDmxStore((s) => s.setGrandMaster);
  const [status, setStatus] = useState<TimingStatus | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; slot: GmSlot } | null>(null);
  const bindings = status?.settings.grandMasterMidi;

  useEffect(() => {
    void timing.timingStatus().then(setStatus).catch(() => undefined);
    const poll = () => void engine.grandMaster().then((gm) => useDmxStore.setState({ grandMaster: Math.round(gm) })).catch(() => undefined);
    const id = setInterval(poll, 50);
    return () => clearInterval(id);
  }, []);

  // Saves on top of the settings as they are now, not as this component last saw them.
  const configureGm = (changes: Partial<GrandMasterMidi>) =>
    void timing
      .timingStatus()
      .then(({ settings }) => timing.timingConfigure({ ...settings, grandMasterMidi: { ...settings.grandMasterMidi, ...changes } }))
      .then(setStatus)
      .catch(() => undefined);
  const described = bindings ? (Object.keys(GM_SLOTS) as GmSlot[]).flatMap((slot) => (bindings[slot] ? [`${GM_SLOTS[slot].toLowerCase()} ${describeTrigger(bindings[slot]!)}`] : [])) : [];

  return (
    <>
      <label
        className="relative flex items-center gap-2 text-xs text-muted-foreground"
        title={`Grand master: scales every intensity channel${described.length > 0 ? ` (${described.join(", ")})` : ""}. Right-click to bind MIDI.`}
        onContextMenu={(e) => {
          e.preventDefault();
          setMenu({ x: e.clientX, y: e.clientY, slot: "knob" });
        }}
      >
        GM
        {described.length > 0 && <span className="absolute -left-2 top-0 h-1.5 w-1.5 rounded-full bg-primary" />}
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
      {menu && (
        <MidiBindingMenu
          key={menu.slot}
          x={menu.x}
          y={menu.y}
          title="Grand master · MIDI"
          binding={bindings?.[menu.slot] ?? null}
          accept={menu.slot === "knob" ? (e) => e.kind === "cc" : isPress}
          hint={menu.slot === "knob" ? "Move a knob or fader…" : "Press a pad or button…"}
          noPorts={status?.settings.controllerPorts.length === 0}
          header={
            <SlotTabs
              slots={GM_SLOTS}
              value={menu.slot}
              onChange={(slot) => setMenu({ ...menu, slot })}
              title="Bind a knob, two buttons that move it up and down while held, or one sweep pad that moves it the opposite way each press"
            />
          }
          footer={menu.slot !== "knob" && (
              <TravelField
                seconds={bindings?.travel ?? 2}
                onCommit={(travel) => configureGm({ travel })}
                velocity={bindings?.velocity ?? false}
                onVelocity={(velocity) => configureGm({ velocity })}
              />
            )}
          onBind={(trigger) => configureGm({ [menu.slot]: trigger })}
          onClose={() => setMenu(null)}
        />
      )}
    </>
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

  const mode = status?.settings.blackoutMode ?? "toggle";

  // Saves on top of the settings as they are now, not as this component last saw them.
  const configure = (changes: { blackoutMidi?: MidiTrigger | null; blackoutMode?: BlackoutMode }) =>
    void timing
      .timingStatus()
      .then(({ settings }) => timing.timingConfigure({ ...settings, ...changes }))
      .then(setStatus)
      .catch(() => undefined);
  const bind = (blackoutMidi: MidiTrigger | null) => configure({ blackoutMidi });

  return (
    <>
      <Button
        variant={blackout ? "destructive" : "outline"}
        onClick={() => void toggleBlackout()}
        onContextMenu={(e) => {
          e.preventDefault();
          setMenu({ x: e.clientX, y: e.clientY });
        }}
        title={`Blackout${binding ? ` (also ${describeTrigger(binding)}, ${mode === "flash" ? "on while held" : "toggles"})` : ""}. Right-click to bind a MIDI pad.`}
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
          header={
            <div className="flex gap-1" title="Toggle: each press flips blackout. Flash: blackout is on only while the pad is held.">
              {(["toggle", "flash"] as BlackoutMode[]).map((m) => (
                <Button
                  key={m}
                  size="sm"
                  variant={mode === m ? "default" : "ghost"}
                  className="h-6 flex-1 px-2 text-xs capitalize"
                  onClick={() => configure({ blackoutMode: m })}
                >
                  {m}
                </Button>
              ))}
            </div>
          }
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
