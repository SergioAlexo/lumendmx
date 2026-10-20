import { useEffect, useState } from "react";
import { Plus, RefreshCw, Search, Trash2 } from "lucide-react";
import * as dmx from "../lib/dmx";
import * as showApi from "../lib/show";
import type { DetectedController, DriverConfig } from "../lib/showfile/generated";
import { cn } from "../lib/utils";
import { EMPTY_UNIVERSES, useShowStore } from "../store/useShowStore";
import { useUniverseStatusesStore } from "../store/useUniverseStatusesStore";
import { Button } from "./ui/button";
import { Input, Select } from "./ui/input";
import { Section } from "./ui/section";
import { AppUpdateCard } from "./AppUpdateCard";
import { TimingSettingsPanel } from "./TimingSettings";
import { McpSettingsPanel } from "./McpSettings";

type DriverKind = DriverConfig["kind"];

function defaultDriverFor(kind: DriverKind, rates: { serial: number; network: number }): DriverConfig {
  switch (kind) {
    case "null":
      return { kind: "null" };
    case "ftdi":
      return { kind: "ftdi", port: null, rateHz: rates.serial };
    case "enttecPro":
      return { kind: "enttecPro", port: null, universeIndex: 0, rateHz: rates.serial };
    case "artNet":
      return {
        kind: "artNet",
        destination: "255.255.255.255",
        net: 0,
        subnet: 0,
        universe: 0,
        rateHz: rates.network,
      };
    case "sacn":
      return {
        kind: "sacn",
        destination: { kind: "multicast" },
        universe: 1,
        priority: 100,
        rateHz: rates.network,
      };
  }
}

function statusColor(connected: boolean, hasDriver: boolean): string {
  if (!hasDriver) return "bg-muted-foreground/40";
  return connected ? "bg-success" : "bg-destructive";
}

/** Two drivers that would drive the same physical output. */
function sameController(a: DriverConfig, b: DriverConfig): boolean {
  if ((a.kind === "ftdi" || a.kind === "enttecPro") && (b.kind === "ftdi" || b.kind === "enttecPro")) {
    return a.port !== null && a.port === b.port;
  }
  if (a.kind === "artNet" && b.kind === "artNet") return a.destination === b.destination;
  return false;
}

/** The Setup tab: universes and the controller each one outputs through,
 * controller detection, then the tempo and input sources and app updates.
 * One universe is one controller output, so fixtures on different
 * controllers live in different universes. */
export function SetupPanel() {
  const universes = useShowStore((s) => s.show?.universes ?? EMPTY_UNIVERSES);
  const patch = useShowStore((s) => s.show?.patch);
  const run = useShowStore((s) => s.run);
  const statuses = useUniverseStatusesStore((s) => s.statuses);

  const [ports, setPorts] = useState<dmx.SerialPortDescriptor[]>([]);
  const [rates, setRates] = useState({ serial: 30, network: 40 });
  const [detected, setDetected] = useState<DetectedController[] | null>(null);
  const [detecting, setDetecting] = useState(false);

  const refreshPorts = () =>
    void dmx
      .listSerialPorts()
      .then(setPorts)
      .catch(() => undefined);

  useEffect(() => {
    refreshPorts();
    void dmx
      .getDriverDefaults()
      .then((d) => setRates({ serial: d.serialRateHz, network: d.networkRateHz }))
      .catch(() => undefined);
  }, []);

  const detect = async () => {
    setDetecting(true);
    try {
      setDetected(await dmx.detectControllers());
      refreshPorts();
    } catch (err) {
      console.warn("Controller detection failed:", err);
      setDetected([]);
    } finally {
      setDetecting(false);
    }
  };

  const setDriver = (id: number, driver: DriverConfig) => void run(() => showApi.updateUniverse(id, { driver }));
  const unused = (detected ?? []).filter((c) => !universes.some((u) => sameController(u.driver, c.driver)));

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-4">
      <Section
        title="Universes & controllers"
        actions={
          <Button size="sm" onClick={() => void run(() => showApi.addUniverse({ kind: "null" }))}>
            <Plus />
            Add universe
          </Button>
        }
      >
        {universes.map((universe) => {
          const status = statuses[universe.id];
          const hasDriver = universe.driver.kind !== "null";
          const fixtureCount = patch?.filter((f) => f.universe === universe.id).length ?? 0;
          return (
            <div key={universe.id} className="rounded-md border border-border p-3">
              <div className="flex flex-wrap items-center gap-3">
                <span
                  className={cn("h-2.5 w-2.5 shrink-0 rounded-full", statusColor(status?.connected ?? false, hasDriver))}
                  title={status?.error ?? undefined}
                />
                <span className="w-8 shrink-0 text-xs text-muted-foreground">U{universe.id}</span>
                <Input
                  className="w-40"
                  defaultValue={universe.name}
                  key={universe.name}
                  aria-label="Universe name"
                  onBlur={(e) => {
                    const name = e.target.value.trim();
                    if (name && name !== universe.name) void run(() => showApi.updateUniverse(universe.id, { name }));
                  }}
                />

                <Select
                  className="w-40"
                  value={universe.driver.kind}
                  onChange={(e) => setDriver(universe.id, defaultDriverFor(e.target.value as DriverKind, rates))}
                >
                  <option value="null">No output</option>
                  <option value="ftdi">FTDI Open DMX</option>
                  <option value="enttecPro">Enttec DMX USB Pro</option>
                  <option value="artNet">Art-Net</option>
                  <option value="sacn">sACN</option>
                </Select>

                <DriverFields driver={universe.driver} ports={ports} onChange={(driver) => setDriver(universe.id, driver)} />

                {status && (
                  <span className="text-xs text-muted-foreground">
                    {status.connected ? `${status.framesPerSec.toFixed(0)} fps` : status.error ?? "—"}
                  </span>
                )}

                <div className="flex-1" />
                <span className="text-xs text-muted-foreground">{fixtureCount} fixtures</span>
                <Button
                  variant="ghost"
                  size="iconSm"
                  title={fixtureCount > 0 ? "Move or remove its fixtures first" : "Remove universe"}
                  disabled={fixtureCount > 0}
                  onClick={() => void run(() => showApi.removeUniverse(universe.id))}
                >
                  <Trash2 />
                </Button>
              </div>
            </div>
          );
        })}
        {universes.length === 0 && <p className="text-sm text-muted-foreground">No universes yet — add one or detect your controllers.</p>}
      </Section>

      <Section
        title="Detect controllers"
        actions={
          <Button size="sm" variant="outline" onClick={() => void detect()} disabled={detecting}>
            {detecting ? <RefreshCw className="animate-spin" /> : <Search />}
            {detecting ? "Looking…" : "Detect"}
          </Button>
        }
      >
        <p className="text-xs text-muted-foreground">
          Finds USB DMX interfaces (Enttec Pro, FTDI Open DMX) and Art-Net nodes on the network. Each controller becomes
          its own universe.
        </p>
        {detected && (
          <ul className="flex flex-col gap-1.5 text-sm">
            {detected.length === 0 && <li className="text-muted-foreground">Nothing found.</li>}
            {detected.length > 0 && unused.length === 0 && (
              <li className="text-muted-foreground">Every controller found already has a universe.</li>
            )}
            {unused.map((c) => (
              <li key={c.label} className="flex items-center justify-between gap-3 rounded border border-border px-3 py-2">
                <span className="truncate">{c.label}</span>
                <Button size="sm" variant="secondary" onClick={() => void run(() => showApi.addUniverse(c.driver))}>
                  <Plus />
                  Add as universe
                </Button>
              </li>
            ))}
          </ul>
        )}
      </Section>

      <TimingSettingsPanel />

      <McpSettingsPanel />

      <AppUpdateCard />
    </div>
  );
}

function DriverFields({
  driver,
  ports,
  onChange,
}: {
  driver: DriverConfig;
  ports: dmx.SerialPortDescriptor[];
  onChange: (driver: DriverConfig) => void;
}) {
  if (driver.kind === "null") return null;

  if (driver.kind === "ftdi" || driver.kind === "enttecPro") {
    return (
      <Select className="w-56" value={driver.port ?? ""} onChange={(e) => onChange({ ...driver, port: e.target.value || null })}>
        <option value="">Select serial port…</option>
        {ports.map((p) => (
          <option key={p.port_name} value={p.port_name}>
            {p.label}
          </option>
        ))}
      </Select>
    );
  }

  if (driver.kind === "artNet") {
    return (
      <div className="flex items-center gap-1.5">
        <Input
          className="w-36"
          value={driver.destination}
          onChange={(e) => onChange({ ...driver, destination: e.target.value })}
          placeholder="255.255.255.255"
        />
        <NumberField label="Net" value={driver.net} max={127} onChange={(v) => onChange({ ...driver, net: v })} />
        <NumberField label="Sub" value={driver.subnet} max={15} onChange={(v) => onChange({ ...driver, subnet: v })} />
        <NumberField label="Uni" value={driver.universe} max={15} onChange={(v) => onChange({ ...driver, universe: v })} />
      </div>
    );
  }

  // sACN
  return (
    <div className="flex items-center gap-1.5">
      <Select
        className="w-28"
        value={driver.destination.kind}
        onChange={(e) =>
          onChange({
            ...driver,
            destination: e.target.value === "multicast" ? { kind: "multicast" } : { kind: "unicast", address: "" },
          })
        }
      >
        <option value="multicast">Multicast</option>
        <option value="unicast">Unicast</option>
      </Select>
      {driver.destination.kind === "unicast" && (
        <Input
          className="w-32"
          value={driver.destination.address}
          onChange={(e) => onChange({ ...driver, destination: { kind: "unicast", address: e.target.value } })}
          placeholder="IP address"
        />
      )}
      <NumberField label="Uni" value={driver.universe} max={63999} onChange={(v) => onChange({ ...driver, universe: v })} />
      <NumberField label="Pri" value={driver.priority} max={200} onChange={(v) => onChange({ ...driver, priority: v })} />
    </div>
  );
}

function NumberField({
  label,
  value,
  max,
  onChange,
}: {
  label: string;
  value: number;
  max: number;
  onChange: (v: number) => void;
}) {
  return (
    <label className="flex items-center gap-1 text-xs text-muted-foreground">
      {label}
      <Input
        type="number"
        min={0}
        max={max}
        value={value}
        onChange={(e) => onChange(Math.max(0, Math.min(max, Number(e.target.value) || 0)))}
        className="w-16"
      />
    </label>
  );
}
