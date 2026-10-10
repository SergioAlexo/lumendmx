import { useEffect, useState } from "react";
import { Maximize2, Monitor } from "lucide-react";
import * as reliability from "../lib/reliability";
import type { MonitorInfo, Preferences } from "../lib/showfile/generated";
import { usePrefsStore } from "../store/usePrefsStore";
import { Button } from "./ui/button";
import { Input } from "./ui/input";

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="flex items-center justify-between gap-4 py-1.5">
      <span className="flex flex-col">
        <span className="text-sm">{label}</span>
        {hint && <span className="text-xs text-muted-foreground">{hint}</span>}
      </span>
      {children}
    </label>
  );
}

function NumberPref({
  value,
  min,
  max,
  step = 1,
  suffix,
  onChange,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  suffix?: string;
  onChange: (value: number) => void;
}) {
  return (
    <span className="flex items-center gap-1.5">
      <Input
        type="number"
        className="w-24"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => {
          const n = Number(e.target.value);
          if (Number.isFinite(n)) onChange(Math.min(max, Math.max(min, n)));
        }}
      />
      {suffix && <span className="w-8 text-xs text-muted-foreground">{suffix}</span>}
    </span>
  );
}

function Toggle({ checked, onChange }: { checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <input
      type="checkbox"
      className="h-4 w-4 accent-[hsl(var(--primary))]"
      checked={checked}
      onChange={(e) => onChange(e.target.checked)}
    />
  );
}

/** Machine-level preferences (kept outside the show file) for output rates,
 * default fades, GO behaviour, confirmation, display scale and touch mode,
 * plus main-window monitor placement. */
export function PreferencesPanel() {
  const prefs = usePrefsStore((s) => s.prefs);
  const error = usePrefsStore((s) => s.error);
  const update = usePrefsStore((s) => s.update);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);

  useEffect(() => {
    void reliability
      .windowMonitors()
      .then(setMonitors)
      .catch(() => undefined);
  }, []);

  const set = <K extends keyof Preferences>(key: K, value: Preferences[K]) => void update({ [key]: value });

  return (
    <div className="rounded-md border border-border bg-card p-3">
      <h3 className="text-sm font-medium">Preferences</h3>
      <p className="mb-2 text-xs text-muted-foreground">Saved on this computer, not in the show file.</p>
      <div className="divide-y divide-border">
        <Row label="New serial output rate" hint="FTDI and Enttec Pro; applies to outputs you add">
          <NumberPref value={prefs.serialRateHz} min={1} max={44} suffix="Hz" onChange={(v) => set("serialRateHz", v)} />
        </Row>
        <Row label="New network output rate" hint="Art-Net and sACN; applies to outputs you add">
          <NumberPref
            value={prefs.networkRateHz}
            min={1}
            max={44}
            suffix="Hz"
            onChange={(v) => set("networkRateHz", v)}
          />
        </Row>
        <Row label="Live display rate" hint="How often the DMX display refreshes; output is unaffected">
          <NumberPref value={prefs.displayHz} min={5} max={60} suffix="Hz" onChange={(v) => set("displayHz", v)} />
        </Row>
        <Row label="Default fade in" hint="Starting value when recording a cue">
          <NumberPref
            value={prefs.defaultFadeInMs}
            min={0}
            max={600000}
            step={100}
            suffix="ms"
            onChange={(v) => set("defaultFadeInMs", v)}
          />
        </Row>
        <Row label="Default fade out">
          <NumberPref
            value={prefs.defaultFadeOutMs}
            min={0}
            max={600000}
            step={100}
            suffix="ms"
            onChange={(v) => set("defaultFadeOutMs", v)}
          />
        </Row>
        <Row label="Ignore repeated GO within" hint="Guards against double presses; 0 turns it off">
          <NumberPref
            value={prefs.goDebounceMs}
            min={0}
            max={5000}
            step={50}
            suffix="ms"
            onChange={(v) => set("goDebounceMs", v)}
          />
        </Row>
        <Row label="GO resumes a paused playback" hint="Otherwise GO steps while it stays paused">
          <Toggle checked={prefs.goResumesPaused} onChange={(v) => set("goResumesPaused", v)} />
        </Row>
        <Row label="Confirm before deleting">
          <Toggle checked={prefs.confirmOnDelete} onChange={(v) => set("confirmOnDelete", v)} />
        </Row>
        <Row label="Autosave every" hint="While a show has unsaved changes">
          <NumberPref
            value={prefs.autosaveIntervalS}
            min={10}
            max={3600}
            step={10}
            suffix="s"
            onChange={(v) => set("autosaveIntervalS", v)}
          />
        </Row>
        <Row label="Interface scale" hint="0.75 to 1.5">
          <NumberPref value={prefs.uiScale} min={0.75} max={1.5} step={0.05} onChange={(v) => set("uiScale", v)} />
        </Row>
        <Row label="Touch mode" hint="Larger buttons and fields for a touchscreen">
          <Toggle checked={prefs.touchMode} onChange={(v) => set("touchMode", v)} />
        </Row>
      </div>

      <div className="mt-3 border-t border-border pt-3">
        <h4 className="flex items-center gap-2 text-sm font-medium">
          <Monitor className="size-4" />
          Window placement
        </h4>
        <p className="mb-2 text-xs text-muted-foreground">
          The window reopens where you left it, if that screen is still connected. Moving it between screens works on
          Windows and macOS; some Linux compositors ignore requested positions.
        </p>
        <div className="flex flex-wrap gap-2">
          {monitors.map((m) => (
            <Button
              key={m.index}
              size="sm"
              variant={m.current ? "default" : "outline"}
              onClick={() =>
                void reliability
                  .windowMoveToMonitor(m.index)
                  .then(() => reliability.windowMonitors())
                  .then(setMonitors)
              }
            >
              {m.name} ({m.width}x{m.height})
            </Button>
          ))}
          <Button size="sm" variant="outline" onClick={() => void reliability.windowSetFullscreen(true)}>
            <Maximize2 />
            Fullscreen
          </Button>
          <Button size="sm" variant="ghost" onClick={() => void reliability.windowSetFullscreen(false)}>
            Exit fullscreen
          </Button>
        </div>
      </div>
      {error && <p className="mt-2 text-xs text-destructive">{error}</p>}
    </div>
  );
}
