import { useCallback, useEffect, useState } from "react";
import * as showApi from "../lib/show";
import * as timing from "../lib/timing";
import { useShowStore } from "../store/useShowStore";
import type { AudioDeviceInfo, MidiPortInfo, TimingSettings, TimingStatus } from "../lib/showfile/generated";
import { Button } from "./ui/button";
import { Input, Select } from "./ui/input";
import { Section } from "./ui/section";
import { cn } from "../lib/utils";

const STATUS_MS = 100;

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="grid grid-cols-[9rem_1fr] items-center gap-3 text-sm">
      <span className="text-muted-foreground">{label}</span>
      {children}
    </label>
  );
}

/** The show's beat offset in ms; commits on blur or Enter. */
function BeatOffset() {
  const offset = useShowStore((s) => s.show?.beatOffsetMs ?? 0);
  const run = useShowStore((s) => s.run);
  const [draft, setDraft] = useState<string | null>(null);
  const commit = () => {
    if (draft === null) return;
    const ms = Number(draft);
    if (draft.trim() !== "" && Number.isFinite(ms) && ms !== offset) void run(() => showApi.setBeatOffset(ms));
    setDraft(null);
  };
  return (
    <span className="flex items-center gap-2">
      <Input
        type="text"
        inputMode="decimal"
        aria-label="Beat offset in milliseconds"
        className="w-20 text-right tabular-nums"
        value={draft ?? String(offset)}
        onFocus={(e) => {
          setDraft(String(offset));
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
      />
      <span className="text-muted-foreground">ms — positive fires effects earlier, to make up for output delay</span>
    </span>
  );
}

function Meter({ label, value }: { label: string; value: number }) {
  return (
    <div className="flex items-center gap-2 text-xs">
      <span className="w-10 text-muted-foreground">{label}</span>
      <div className="h-2 flex-1 overflow-hidden rounded bg-muted">
        <div className="h-full bg-primary" style={{ width: `${Math.round(value * 100)}%` }} />
      </div>
    </div>
  );
}

/** Where the tempo and timecode come from, and the audio/MIDI inputs the
 * graph's Audio and MIDI nodes read. */
export function TimingSettingsPanel() {
  const [status, setStatus] = useState<TimingStatus | null>(null);
  const [audioInputs, setAudioInputs] = useState<AudioDeviceInfo[]>([]);
  const [midiInputs, setMidiInputs] = useState<MidiPortInfo[]>([]);

  const refreshDevices = useCallback(() => {
    void timing.listAudioInputs().then(setAudioInputs).catch(() => undefined);
    void timing.listMidiInputs().then(setMidiInputs).catch(() => undefined);
  }, []);

  useEffect(() => {
    refreshDevices();
    const poll = () => void timing.timingStatus().then(setStatus).catch(() => undefined);
    poll();
    const id = setInterval(poll, STATUS_MS);
    return () => clearInterval(id);
  }, [refreshDevices]);

  if (!status) return <div className="p-4 text-sm text-muted-foreground">Loading…</div>;
  const s = status.settings;

  const apply = (patch: Partial<TimingSettings>) =>
    void timing.timingConfigure({ ...s, ...patch }).then(setStatus).catch(() => undefined);

  const audioNeeded = s.audioAnalysis || s.tempoSource === "audio";
  const deviceLabel = (id: string | null, list: AudioDeviceInfo[]) => list.find((d) => d.id === id)?.name;

  return (
    <>
      <Section title="Tempo & inputs">
        <Field label="Drive the BPM from">
          <Select value={s.tempoSource} onChange={(e) => apply({ tempoSource: e.target.value as TimingSettings["tempoSource"] })}>
            {timing.TEMPO_SOURCES.map((t) => (
              <option key={t.id} value={t.id}>
                {t.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Beat offset">
          <BeatOffset />
        </Field>
        <Field label="Audio input">
          <div className="flex gap-2">
            <Select
              value={s.audioDevice ?? ""}
              onChange={(e) => apply({ audioDevice: e.target.value || null })}
              title={deviceLabel(s.audioDevice, audioInputs)}
            >
              <option value="">System default</option>
              {audioInputs.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                </option>
              ))}
            </Select>
            <Select
              className="w-28"
              value={s.audioChannel}
              onChange={(e) => apply({ audioChannel: Number(e.target.value) })}
              title="Channel of the device"
            >
              {Array.from({ length: Math.max(2, audioInputs.find((d) => d.id === s.audioDevice)?.channels ?? 2) }, (_, i) => (
                <option key={i} value={i}>
                  Ch {i + 1}
                </option>
              ))}
            </Select>
          </div>
        </Field>
        <Field label="Analysis">
          <span className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={s.audioAnalysis}
              disabled={s.tempoSource === "audio"}
              onChange={(e) => apply({ audioAnalysis: e.target.checked })}
            />
            Listen to the input (needed by Audio nodes)
          </span>
        </Field>
        {audioNeeded && (
          <div className="flex flex-col gap-2 rounded-md bg-muted/50 p-3">
            {status.audio.error ? (
              <span className="text-sm text-destructive">{status.audio.error}</span>
            ) : (
              <>
                <div className="flex items-center justify-between text-sm">
                  <span>
                    Detected{" "}
                    <strong className="tabular-nums">{status.audio.bpm ? status.audio.bpm.toFixed(1) : "—"}</strong> BPM
                  </span>
                  <span
                    className={cn(
                      "rounded px-2 py-0.5 text-xs font-medium",
                      status.audio.locked ? "bg-success text-success-foreground" : "bg-muted text-muted-foreground",
                    )}
                  >
                    {status.audio.locked ? "Locked" : status.audio.running ? "Listening…" : "Off"}
                  </span>
                </div>
                <Meter label="Low" value={status.audio.levels.low} />
                <Meter label="Mid" value={status.audio.levels.mid} />
                <Meter label="High" value={status.audio.levels.high} />
                <Meter label="Loud" value={status.audio.levels.loudness} />
              </>
            )}
          </div>
        )}

        <Field label="MIDI port">
          <Select value={s.midiPort ?? ""} onChange={(e) => apply({ midiPort: e.target.value || null })}>
            <option value="">None</option>
            {midiInputs.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </Select>
        </Field>
        {(s.tempoSource === "midiClock" || s.tempoSource === "mixxx" || s.timecodeSource === "mtc") && (
          <div className="rounded-md bg-muted/50 p-3 text-sm">
            {status.midi.error ? (
              <span className="text-destructive">{status.midi.error}</span>
            ) : status.midi.connected ? (
              <span>
                Connected. {s.tempoSource === "mixxx" ? "Mixxx BPM" : "MIDI clock"}:{" "}
                <strong className="tabular-nums">{status.midi.clockBpm ? status.midi.clockBpm.toFixed(1) : (s.tempoSource === "mixxx" ? "waiting for Mixxx" : "no clock yet")}</strong>
              </span>
            ) : (
              <span className="text-muted-foreground">Not connected</span>
            )}
          </div>
        )}
        <div>
          <Button size="sm" variant="outline" onClick={refreshDevices}>
            Rescan devices
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">
          Ableton Link and Pioneer Pro DJ Link aren&apos;t offered: see DECISIONS.md.
        </p>
      </Section>

      <Section title="Timecode">
        <Field label="Source">
          <Select
            value={s.timecodeSource}
            onChange={(e) => apply({ timecodeSource: e.target.value as TimingSettings["timecodeSource"] })}
          >
            {timing.TIMECODE_SOURCES.map((t) => (
              <option key={t.id} value={t.id}>
                {t.label}
              </option>
            ))}
          </Select>
        </Field>
        {s.timecodeSource === "ltc" && (
          <Field label="LTC input">
            <div className="flex gap-2">
              <Select value={s.ltcDevice ?? ""} onChange={(e) => apply({ ltcDevice: e.target.value || null })}>
                <option value="">System default</option>
                {audioInputs.map((d) => (
                  <option key={d.id} value={d.id}>
                    {d.name}
                  </option>
                ))}
              </Select>
              <Select className="w-28" value={s.ltcChannel} onChange={(e) => apply({ ltcChannel: Number(e.target.value) })}>
                {Array.from({ length: Math.max(2, audioInputs.find((d) => d.id === s.ltcDevice)?.channels ?? 2) }, (_, i) => (
                  <option key={i} value={i}>
                    Ch {i + 1}
                  </option>
                ))}
              </Select>
            </div>
          </Field>
        )}
        {status.ltcError && <span className="text-sm text-destructive">{status.ltcError}</span>}
        <div className="flex items-center gap-3 text-sm">
          <span className="font-mono text-lg tabular-nums">{timing.formatTimecode(status.timecode.positionMs, status.timecode.fps)}</span>
          <span className={cn("text-xs", status.timecode.active ? "text-success" : "text-muted-foreground")}>
            {status.timecode.active ? "running" : s.timecodeSource === "none" ? "off" : "no signal"}
          </span>
        </div>
      </Section>

    </>
  );
}
