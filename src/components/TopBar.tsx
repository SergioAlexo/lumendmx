import { Power, RefreshCw } from "lucide-react";
import { PRIMARY_UNIVERSE_ID, useDmxStore } from "../store/useDmxStore";
import { EMPTY_UNIVERSES, useShowStore } from "../store/useShowStore";
import { useUniverseStatusesStore } from "../store/useUniverseStatusesStore";
import { Button } from "./ui/button";
import { Select } from "./ui/input";
import { cn } from "../lib/utils";
import { ReliabilityBar } from "./ReliabilityBar";
import { ShowMenu } from "./ShowMenu";
import { TempoControl } from "./TempoControl";

function statusColor(connected: boolean, hasDriver: boolean): string {
  if (!hasDriver) return "bg-muted-foreground/40";
  return connected ? "bg-success" : "bg-destructive";
}

/** One dot per configured universe (green/amber-ish gray/red), per BUILD_PLAN
 * Phase 2's "per-universe status in the top bar". The primary universe's own
 * detailed connect/disconnect controls sit next to it; this row covers
 * whatever else Setup has added. */
function UniverseStatusDots() {
  const universes = useShowStore((s) => s.meta?.universes ?? EMPTY_UNIVERSES);
  const statuses = useUniverseStatusesStore((s) => s.statuses);
  const extra = universes.filter((u) => u.id !== PRIMARY_UNIVERSE_ID);
  if (extra.length === 0) return null;

  return (
    <div className="flex items-center gap-1.5">
      {extra.map((universe) => {
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

export function TopBar() {
  const { status, blackout, ports, connect, disconnect, toggleBlackout, refreshPorts } = useDmxStore();
  const primaryDriver = useShowStore((s) =>
    s.meta?.universes.find((u) => u.id === PRIMARY_UNIVERSE_ID)?.driver,
  );
  const primaryPort = primaryDriver?.kind === "ftdi" ? primaryDriver.port : null;

  return (
    <header className="flex items-center gap-4 border-b border-border bg-card px-4 py-3">
      <h1 className="text-lg font-semibold tracking-tight">
        Lumen<span className="text-primary">DMX</span>
      </h1>

      <ShowMenu />

      <div className="flex flex-1 items-center gap-3">
        <span
          className={cn("h-2.5 w-2.5 rounded-full", status.connected ? "bg-success" : "bg-destructive")}
          title={status.error ?? undefined}
        />
        <span className="text-sm text-muted-foreground">
          {status.connected
            ? `Connected: ${primaryPort ?? "Universe 1"}`
            : status.error
              ? `Error: ${status.error}`
              : "Not connected"}
        </span>

        {!status.connected && (
          <>
            <Select
              className="w-56"
              onFocus={() => void refreshPorts()}
              onChange={(e) => e.target.value && void connect(e.target.value)}
              value=""
            >
              <option value="" disabled>
                Select DMX interface…
              </option>
              {ports.map((port) => (
                <option key={port.port_name} value={port.port_name}>
                  {port.label}
                </option>
              ))}
            </Select>
            <Button variant="ghost" size="iconSm" onClick={() => void refreshPorts()} title="Refresh ports">
              <RefreshCw />
            </Button>
          </>
        )}
        {status.connected && (
          <Button variant="outline" size="sm" onClick={() => void disconnect()}>
            Disconnect
          </Button>
        )}

        <UniverseStatusDots />
      </div>

      <TempoControl />

      <ReliabilityBar />

      <Button
        variant={blackout ? "destructive" : "outline"}
        onClick={() => void toggleBlackout()}
        className={cn(
          "font-bold uppercase tracking-wider",
          blackout ? "shadow-lg shadow-destructive/40" : "text-destructive border-destructive/60 hover:bg-destructive/10 hover:text-destructive",
        )}
      >
        <Power />
        Blackout
      </Button>
    </header>
  );
}
