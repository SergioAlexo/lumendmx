import { useMemo } from "react";
import type { CurveMode } from "../../lib/curve";
import { fanOffsets, lfoValue, type FanSettings, type WaveSettings } from "../../lib/waves";
import type { Group, GraphView } from "../../lib/showfile/generated";

const W = 184;

/** Fixtures from first (bright) to last (dim), so a chase's direction reads. */
const dotColor = (i: number, count: number) => `hsl(200 80% ${Math.round(75 - (count > 1 ? i / (count - 1) : 0) * 40)}%)`;

/** Fixtures in a "1-8, 12" list. */
function listedFixtures(text: string): number {
  let count = 0;
  for (const part of text.split(",")) {
    const [a, b] = part.split("-").map((x) => Number(x.trim()));
    if (Number.isFinite(a) && part.trim() !== "") count += Number.isFinite(b) ? Math.max(0, b - a + 1) : 1;
  }
  return count;
}

/** How many fixtures the chain from `from` ends up driving: the first Output
 * it feeds, by its group or fixture list; 8 when it feeds none. */
export function targetFixtureCount(graph: GraphView | undefined, groups: Group[], from: string): number {
  if (!graph) return 8;
  const seen = new Set<string>();
  const queue = [from];
  while (queue.length > 0) {
    const id = queue.shift()!;
    if (seen.has(id)) continue;
    seen.add(id);
    const node = graph.nodes.find((n) => n.id === id);
    if (node?.kind === "output") {
      const group = groups.find((g) => g.id === Number(node.params.group ?? 0));
      const count = group ? group.fixtureNumbers.length : listedFixtures(String(node.params.fixtures ?? ""));
      if (count > 0) return count;
    }
    for (const e of graph.edges) if (e.fromNode === id) queue.push(e.toNode);
  }
  return 8;
}

/** The Fan feeding an LFO's phase directly, with its settings. */
export function fanFeeding(graph: GraphView | undefined, node: string): FanSettings | undefined {
  const edge = graph?.edges.find((e) => e.toNode === node && e.toSocket === "phase");
  const fan = edge && graph?.nodes.find((n) => n.id === edge.fromNode && n.kind === "fan");
  return fan ? fanSettings(fan.params) : undefined;
}

export function fanSettings(params: Record<string, unknown>): FanSettings {
  const num = (id: string, fallback: number) => (Number.isFinite(Number(params[id])) ? Number(params[id]) : fallback);
  return { spread: num("spread", 1), groups: num("groups", 0), blocks: num("blocks", 0), wings: num("wings", 0) };
}

export function waveSettings(params: Record<string, unknown>): WaveSettings {
  return {
    waveform: String(params.waveform ?? "sine"),
    width: Number(params.width ?? 50),
    curve: String(params.curve ?? ""),
    curveMode: String(params.curveMode ?? "linear") as CurveMode,
  };
}

/** One cycle of an LFO's wave, with a dot where it is now. With a Fan in
 * front, a dot per fixture, each a little ahead or behind: the chase. */
export function WavePreview({ wave, playhead, fan, count }: { wave: WaveSettings; playhead: number | undefined; fan?: FanSettings; count: number }) {
  const H = 44;
  const y = (v: number) => 3 + (1 - v) * (H - 6);
  const line = useMemo(() => {
    const points: string[] = [];
    for (let i = 0; i <= W; i++) {
      const v = lfoValue(wave, Math.min(i / W, 1 - 1e-9));
      if (v === undefined) return undefined;
      points.push(`${i},${y(v).toFixed(1)}`);
    }
    return "M" + points.join(" L");
  }, [wave.waveform, wave.width, wave.curve, wave.curveMode]); // eslint-disable-line react-hooks/exhaustive-deps
  const offsets = useMemo(() => (fan ? fanOffsets(fan, count) : [0]), [fan?.spread, fan?.groups, fan?.blocks, fan?.wings, count]); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <svg width={W} height={H} className="rounded bg-muted" aria-label="One cycle of the wave">
      <line x1={0} x2={W} y1={y(0)} y2={y(0)} stroke="currentColor" strokeOpacity={0.12} />
      <line x1={0} x2={W} y1={y(1)} y2={y(1)} stroke="currentColor" strokeOpacity={0.12} />
      {line ? (
        <path d={line} fill="none" stroke="#a1a1a1" strokeWidth={1.5} />
      ) : (
        <text x={W / 2} y={H / 2 + 3} textAnchor="middle" className="fill-muted-foreground text-[10px]">
          a new random value each cycle
        </text>
      )}
      {playhead !== undefined &&
        line &&
        offsets.map((o, i) => {
          const p = (((playhead + o) % 1) + 1) % 1;
          const v = lfoValue(wave, p) ?? 0;
          return <circle key={i} cx={p * W} cy={y(v)} r={offsets.length > 1 ? 2.5 : 3.5} fill={offsets.length > 1 ? dotColor(i, offsets.length) : "#8ab4ff"} />;
        })}
    </svg>
  );
}

/** A Fan, drawn: a row per fixture (first on top), a dot at its phase offset
 * in the cycle. A staircase down to the right is a chase from first to last. */
export function FanPreview({ fan, count }: { fan: FanSettings; count: number }) {
  const shown = Math.min(count, 64);
  const H = Math.min(96, Math.max(32, shown * 6));
  const offsets = useMemo(() => fanOffsets(fan, shown), [fan.spread, fan.groups, fan.blocks, fan.wings, shown]); // eslint-disable-line react-hooks/exhaustive-deps
  const direction = fan.spread === 0 ? "all fixtures together" : fan.spread > 0 ? "runs first → last" : "runs last → first";

  return (
    <div className="flex flex-col gap-0.5">
      <svg width={W} height={H} className="rounded bg-muted" aria-label="Phase offset of each fixture">
        {[0.25, 0.5, 0.75].map((x) => (
          <line key={x} x1={x * W} x2={x * W} y1={0} y2={H} stroke="currentColor" strokeOpacity={0.1} />
        ))}
        {offsets.map((o, i) => (
          <circle key={i} cx={3 + o * (W - 6)} cy={((i + 0.5) / shown) * H} r={Math.min(3, H / shown / 2 - 0.3)} fill={dotColor(i, shown)} />
        ))}
      </svg>
      <span className="text-[10px] text-muted-foreground">
        {count} fixture{count === 1 ? "" : "s"} · {direction}
      </span>
    </div>
  );
}
