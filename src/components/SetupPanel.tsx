import { useEffect, useState } from "react";
import { Plus, RefreshCw, Trash2, Wifi } from "lucide-react";
import * as dmx from "../lib/dmx";
import type { ArtNetNode, DriverConfig } from "../lib/showfile/generated";
import { cn } from "../lib/utils";
import { EMPTY_UNIVERSES, useShowStore } from "../store/useShowStore";
import { useUniverseStatusesStore } from "../store/useUniverseStatusesStore";
import { Button } from "./ui/button";
import { Input, Select } from "./ui/input";
import { AppUpdateCard } from "./AppUpdateCard";
import { TimingSettingsPanel } from "./TimingSettings";

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

/** The Setup tab: universes and the controller each one outputs through
 * (add/remove, driver settings, live status, Art-Net node discovery), then
 * the tempo and input sources and app updates. */
export function SetupPanel() {
  const universes = useShowStore((s) => s.meta?.universes ?? EMPTY_UNIVERSES);
  const setUniverseDriver = useShowStore((s) => s.setUniverseDriver);
  const addUniverse = useShowStore((s) => s.addUniverse);
  const removeUniverseEntirely = useShowStore((s) => s.removeUniverseEntirely);
  const statuses = useUniverseStatusesStore((s) => s.statuses);

  const [ports, setPorts] = useState<dmx.SerialPortDescriptor[]>([]);
  const [rates, setRates] = useState({ serial: 30, network: 40 });
  const [nodes, setNodes] = useState<ArtNetNode[] | null>(null);
  const [discovering, setDiscovering] = useState(false);

  useEffect(() => {
    void dmx
      .listSerialPorts()
      .then(setPorts)
      .catch(() => undefined);
    void dmx
      .getDriverDefaults()
      .then((d) => setRates({ serial: d.serialRateHz, network: d.networkRateHz }))
      .catch(() => undefined);
  }, []);

  const discover = async () => {
    setDiscovering(true);
    try {
      setNodes(await dmx.discoverArtNetNodes());
    } catch (err) {
      console.warn("Art-Net discovery failed:", err);
      setNodes([]);
    } finally {
      setDiscovering(false);
    }
  };

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-4">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold uppercase tracking-wide text-muted-foreground">
          Universes &amp; Output
        </h2>
        <Button size="sm" onClick={() => addUniverse(`Universe ${universes.length + 1}`)}>
          <Plus />
          Add universe
        </Button>
      </div>

      <div className="flex flex-col gap-3">
        {universes.map((universe) => {
          const status = statuses[universe.id];
          const hasDriver = universe.driver.kind !== "null";
          return (
            <div key={universe.id} className="rounded-md border border-border bg-card p-3">
              <div className="flex flex-wrap items-center gap-3">
                <span
                  className={cn("h-2.5 w-2.5 shrink-0 rounded-full", statusColor(status?.connected ?? false, hasDriver))}
                  title={status?.error ?? undefined}
                />
                <span className="w-28 shrink-0 truncate text-sm font-medium">{universe.name}</span>

                <Select
                  className="w-40"
                  value={universe.driver.kind}
                  onChange={(e) =>
                    void setUniverseDriver(universe.id, defaultDriverFor(e.target.value as DriverKind, rates))
                  }
                >
                  <option value="null">No output</option>
                  <option value="ftdi">FTDI Open DMX</option>
                  <option value="enttecPro">Enttec DMX USB Pro</option>
                  <option value="artNet">Art-Net</option>
                  <option value="sacn">sACN</option>
                </Select>

                <DriverFields
                  driver={universe.driver}
                  ports={ports}
                  onChange={(driver) => void setUniverseDriver(universe.id, driver)}
                />

                {status && (
                  <span className="text-xs text-muted-foreground">
                    {status.connected ? `${status.framesPerSec.toFixed(0)} fps` : status.error ?? "—"}
                  </span>
                )}

                <div className="flex-1" />
                <Button
                  variant="ghost"
                  size="iconSm"
                  title="Remove universe"
                  onClick={() => removeUniverseEntirely(universe.id)}
                >
                  <Trash2 />
                </Button>
              </div>
            </div>
          );
        })}
        {universes.length === 0 && <p className="text-sm text-muted-foreground">No universes yet — add one above.</p>}
      </div>

      <div className="rounded-md border border-border bg-card p-3">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-medium">Art-Net node discovery</h3>
          <Button size="sm" variant="outline" onClick={() => void discover()} disabled={discovering}>
            {discovering ? <RefreshCw className="animate-spin" /> : <Wifi />}
            {discovering ? "Discovering…" : "Discover"}
          </Button>
        </div>
        {nodes && (
          <ul className="mt-2 space-y-1 text-sm text-muted-foreground">
            {nodes.length === 0 && <li>No nodes replied.</li>}
            {nodes.map((n) => (
              <li key={n.ip}>
                {n.ip} — {n.longName || n.shortName || "Unnamed node"}
              </li>
            ))}
          </ul>
        )}
      </div>

      <TimingSettingsPanel />

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
