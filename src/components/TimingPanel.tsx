import { useCallback, useEffect, useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import * as engineApi from "../lib/engine";
import * as timing from "../lib/timing";
import type {
  AudioDeviceInfo,
  AudioTrigger,
  Band,
  EffectInstance,
  MidiPortInfo,
  PlaybackStatus,
  TimingSettings,
  TimingStatus,
} from "../lib/showfile/generated";
import { Button } from "./ui/button";
import { Select } from "./ui/input";
import { ValueSlider } from "./ui/value-slider";
import { cn } from "../lib/utils";

const STATUS_MS = 100;
const BANDS: { id: Band; label: string }[] = [
  { id: "low", label: "Low" },
  { id: "mid", label: "Mid" },
  { id: "high", label: "High" },
];

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="rounded-md border border-border bg-card p-4">
      <h3 className="mb-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground">{title}</h3>
      <div className="flex flex-col gap-3">{children}</div>
    </section>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="grid grid-cols-[9rem_1fr] items-center gap-3 text-sm">
      <span className="text-muted-foreground">{label}</span>
      {children}
    </label>
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

function TriggerRow({
  trigger,
  playbacks,
  effects,
  onChange,
  onDelete,
}: {
  trigger: AudioTrigger;
  playbacks: PlaybackStatus[];
  effects: EffectInstance[];
  onChange: (t: AudioTrigger) => void;
  onDelete: () => void;
}) {
  const targetValue = trigger.target.kind === "flash" ? `p${trigger.target.playbackId}` : `e${trigger.target.effectId}`;
  return (
    <div className="grid grid-cols-[auto_6rem_1fr_1fr_auto] items-center gap-3 rounded-md border border-border p-2 text-sm">
      <input
        type="checkbox"
        aria-label="Enabled"
        checked={trigger.enabled}
        onChange={(e) => onChange({ ...trigger, enabled: e.target.checked })}
      />
      <Select value={trigger.band} onChange={(e) => onChange({ ...trigger, band: e.target.value as Band })}>
        {BANDS.map((b) => (
          <option key={b.id} value={b.id}>
            {b.label}
          </option>
        ))}
      </Select>
      <Select
        value={targetValue}
        onChange={(e) => {
          const v = e.target.value;
          onChange({
            ...trigger,
            target:
              v[0] === "p"
                ? { kind: "flash", playbackId: Number(v.slice(1)) }
                : { kind: "effectSize", effectId: Number(v.slice(1)) },
          });
        }}
      >
        <optgroup label="Flash playback">
          {playbacks.map((p) => (
            <option key={`p${p.id}`} value={`p${p.id}`}>
              Playback {p.id + 1}
            </option>
          ))}
        </optgroup>
        <optgroup label="Effect size">
          {effects.map((e) => (
            <option key={`e${e.id}`} value={`e${e.id}`}>
              Effect #{e.id} {e.def.target.kind === "attribute" ? e.def.target.attribute : "position"}
            </option>
          ))}
        </optgroup>
      </Select>
      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          Sens
          <ValueSlider
            value={Math.round(trigger.sensitivity * 100)}
            max={100}
            onChange={(v) => onChange({ ...trigger, sensitivity: v / 100 })}
            title="Sensitivity"
          />
        </div>
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          Gate
          <ValueSlider
            value={Math.round(trigger.gate * 100)}
            max={100}
            onChange={(v) => onChange({ ...trigger, gate: v / 100 })}
            title="Levels below the gate count as silence"
          />
        </div>
      </div>
      <Button size="iconSm" variant="ghost" title="Delete trigger" onClick={onDelete}>
        <Trash2 />
      </Button>
    </div>
  );
}

/** Timing sources (BUILD_PLAN Phase 8): where the tempo and timecode come
 * from, and audio-reactive triggers. */
export function TimingPanel() {
  const [status, setStatus] = useState<TimingStatus | null>(null);
  const [audioInputs, setAudioInputs] = useState<AudioDeviceInfo[]>([]);
  const [midiInputs, setMidiInputs] = useState<MidiPortInfo[]>([]);
  const [triggers, setTriggers] = useState<AudioTrigger[]>([]);
  const [playbacks, setPlaybacks] = useState<PlaybackStatus[]>([]);
  const [effects, setEffects] = useState<EffectInstance[]>([]);

  const refreshTargets = useCallback(() => {
    void timing.listTriggers().then(setTriggers).catch(() => undefined);
    void engineApi.listPlaybacks().then(setPlaybacks).catch(() => undefined);
    void engineApi.listEffects().then(setEffects).catch(() => undefined);
  }, []);

  const refreshDevices = useCallback(() => {
    void timing.listAudioInputs().then(setAudioInputs).catch(() => undefined);
    void timing.listMidiInputs().then(setMidiInputs).catch(() => undefined);
  }, []);

  useEffect(() => {
    refreshDevices();
    refreshTargets();
    const poll = () => void timing.timingStatus().then(setStatus).catch(() => undefined);
    poll();
    const id = setInterval(poll, STATUS_MS);
    return () => clearInterval(id);
  }, [refreshDevices, refreshTargets]);

  if (!status) return <div className="p-4 text-sm text-muted-foreground">Loading…</div>;
  const s = status.settings;

  const apply = (patch: Partial<TimingSettings>) =>
    void timing.timingConfigure({ ...s, ...patch }).then(setStatus).catch(() => undefined);

  const saveTrigger = (t: AudioTrigger) => {
    setTriggers((list) => list.map((x) => (x.id === t.id ? t : x)));
    void timing.upsertTrigger(t).catch(() => undefined);
  };

  const addTrigger = () => {
    const target: AudioTrigger["target"] = playbacks[0]
      ? { kind: "flash", playbackId: playbacks[0].id }
      : { kind: "effectSize", effectId: effects[0]?.id ?? 0 };
    void timing
      .upsertTrigger({ id: 0, band: "low", sensitivity: 0.5, gate: 0.3, releaseMs: 150, target, enabled: true })
      .then(refreshTargets);
  };

  const audioNeeded = s.audioAnalysis || s.tempoSource === "audio";
  const deviceLabel = (id: string | null, list: AudioDeviceInfo[]) => list.find((d) => d.id === id)?.name;

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-4">
      <Section title="Tempo source">
        <Field label="Drive the BPM from">
          <Select value={s.tempoSource} onChange={(e) => apply({ tempoSource: e.target.value as TimingSettings["tempoSource"] })}>
            {timing.TEMPO_SOURCES.map((t) => (
              <option key={t.id} value={t.id}>
                {t.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Chases">
          <span className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={s.chasesFollowTempo}
              onChange={(e) => apply({ chasesFollowTempo: e.target.checked })}
            />
            Run at the show tempo instead of their own BPM
          </span>
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
            Listen to the input (needed for audio triggers)
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
        {(s.tempoSource === "midiClock" || s.timecodeSource === "mtc") && (
          <div className="rounded-md bg-muted/50 p-3 text-sm">
            {status.midi.error ? (
              <span className="text-destructive">{status.midi.error}</span>
            ) : status.midi.connected ? (
              <span>
                Connected. MIDI clock:{" "}
                <strong className="tabular-nums">{status.midi.clockBpm ? status.midi.clockBpm.toFixed(1) : "no clock yet"}</strong>
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
            {status.timecode.active ? "running" : s.timecodeSource === "internal" ? "stopped" : "no signal"}
          </span>
        </div>
        <Field label="Learn">
          <span className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={status.timecodeLearn}
              onChange={(e) => void timing.setTimecodeLearn(e.target.checked)}
            />
            GO on a Timecode cuelist records that cue&apos;s time from the running clock
          </span>
        </Field>
      </Section>

      <Section title="Audio triggers">
        {!audioNeeded && (
          <p className="text-xs text-muted-foreground">Turn on Analysis above so the audio input is listened to.</p>
        )}
        {triggers.map((t) => (
          <TriggerRow
            key={t.id}
            trigger={t}
            playbacks={playbacks}
            effects={effects}
            onChange={saveTrigger}
            onDelete={() => void timing.deleteTrigger(t.id).then(refreshTargets)}
          />
        ))}
        <div className="flex items-center gap-2">
          <Button size="sm" variant="outline" onClick={addTrigger}>
            <Plus /> Add trigger
          </Button>
          <Button size="sm" variant="ghost" onClick={refreshTargets}>
            Refresh targets
          </Button>
        </div>
        <p className="text-xs text-muted-foreground">
          A band&apos;s level above the gate flashes the playback or sets the effect&apos;s size. Playback and effect
          ids belong to this session; triggers aren&apos;t saved with the show.
        </p>
      </Section>
    </div>
  );
}
