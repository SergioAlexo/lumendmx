import { useMemo, useRef, useState } from "react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { usePatchStore } from "../store/usePatchStore";
import { useSelectionStore } from "../store/useSelectionStore";
import { useUniverseChannelsStore } from "../store/useUniverseChannelsStore";
import { deriveFixtureLook, litColor, rgbToCss } from "../lib/visualisation/fixtureLook";
import {
  clampToStage,
  DEFAULT_FIXTURE_HEIGHT_M,
  layoutFixtures,
  metresToPx,
  pxToMetres,
  snap,
  STAGE_DEPTH_M,
  STAGE_HEIGHT_M,
  STAGE_WIDTH_M,
} from "../lib/visualisation/stage";
import { useVisualFixtures } from "../lib/visualisation/useVisualFixtures";
import type { StagePosition } from "../lib/showfile/generated";

/** SVG user units per metre. */
const PX_PER_M = 100;
const VIEW_W = STAGE_WIDTH_M * PX_PER_M;
const VIEW_H = STAGE_DEPTH_M * PX_PER_M;
const FIXTURE_RADIUS = 26;

interface DragState {
  pointerId: number;
  /** Pointer position in metres when the drag began. */
  startX: number;
  startY: number;
  /** Live offset in metres applied to every dragged fixture. */
  dx: number;
  dy: number;
  moved: boolean;
  /** Fixture ids moving together (the whole selection if the grabbed one is in it). */
  ids: string[];
}

/** Top-down stage plot: drag fixtures into position (saved in the show file),
 * live colour/intensity from the engine's output frames, click to select. */
export function StagePlot() {
  const fixtures = useVisualFixtures();
  const frames = useUniverseChannelsStore((s) => s.channels);
  const selected = useSelectionStore((s) => s.selected);
  const select = useSelectionStore((s) => s.select);
  const toggle = useSelectionStore((s) => s.toggle);
  const clear = useSelectionStore((s) => s.clear);
  const setStagePositions = usePatchStore((s) => s.setStagePositions);
  const svgRef = useRef<SVGSVGElement>(null);
  const [drag, setDrag] = useState<DragState | null>(null);

  const layout = useMemo(() => layoutFixtures(fixtures.map((f) => f.instance)), [fixtures]);

  const pointerMetres = (e: React.PointerEvent) => {
    const rect = svgRef.current?.getBoundingClientRect();
    if (!rect || rect.width === 0) return { x: 0, y: 0 };
    return pxToMetres({ x: e.clientX - rect.left, y: e.clientY - rect.top }, rect.width, rect.height);
  };

  const onFixturePointerDown = (e: React.PointerEvent, id: string, fixtureNumber: number) => {
    e.stopPropagation();
    e.currentTarget.setPointerCapture(e.pointerId);
    const additive = e.shiftKey || e.metaKey || e.ctrlKey;
    let ids = selected.includes(fixtureNumber) ? fixtures.filter((f) => selected.includes(f.instance.fixtureNumber)).map((f) => f.instance.id) : [id];
    if (additive) {
      toggle(fixtureNumber);
      ids = [id];
    } else if (!selected.includes(fixtureNumber)) {
      select([fixtureNumber]);
    }
    const p = pointerMetres(e);
    setDrag({ pointerId: e.pointerId, startX: p.x, startY: p.y, dx: 0, dy: 0, moved: false, ids });
  };

  const onPointerMove = (e: React.PointerEvent) => {
    if (!drag || e.pointerId !== drag.pointerId) return;
    const p = pointerMetres(e);
    const dx = p.x - drag.startX;
    const dy = p.y - drag.startY;
    setDrag({ ...drag, dx, dy, moved: drag.moved || Math.hypot(dx, dy) > 0.1 });
  };

  const onPointerUp = (e: React.PointerEvent) => {
    if (!drag || e.pointerId !== drag.pointerId) return;
    if (drag.moved) {
      const positions: Record<string, StagePosition> = {};
      for (const id of drag.ids) {
        const base = layout.get(id)?.position;
        if (!base) continue;
        const moved = clampToStage({ x: snap(base.x + drag.dx), y: snap(base.y + drag.dy), z: base.z });
        positions[id] = moved;
      }
      setStagePositions(positions);
    }
    setDrag(null);
  };

  const selectedFixtures = fixtures.filter((f) => selected.includes(f.instance.fixtureNumber));
  const selectedHeight = selectedFixtures.length
    ? (layout.get(selectedFixtures[0].instance.id)?.position.z ?? DEFAULT_FIXTURE_HEIGHT_M)
    : DEFAULT_FIXTURE_HEIGHT_M;

  const setSelectedHeight = (z: number) => {
    const positions: Record<string, StagePosition> = {};
    for (const f of selectedFixtures) {
      const base = layout.get(f.instance.id)?.position;
      if (base) positions[f.instance.id] = clampToStage({ ...base, z });
    }
    setStagePositions(positions);
  };

  const placeAll = () => {
    const positions: Record<string, StagePosition> = {};
    for (const [id, { position }] of layout) positions[id] = position;
    setStagePositions(positions);
  };

  if (fixtures.length === 0) {
    return (
      <div className="flex h-full items-center justify-center p-8 text-sm text-muted-foreground">
        No fixtures patched. Patch fixtures in Fixtures or Setup, then arrange them here.
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="flex flex-wrap items-center gap-3 border-b border-border px-4 py-2">
        <Button size="sm" className="h-11" variant="outline" onClick={() => select(fixtures.map((f) => f.instance.fixtureNumber))}>
          Select all
        </Button>
        <Button size="sm" className="h-11" variant="outline" onClick={clear} disabled={selected.length === 0}>
          Deselect
        </Button>
        <Button size="sm" className="h-11" variant="outline" onClick={placeAll} title="Save the automatic layout as these fixtures' positions">
          Save layout
        </Button>
        <label className="flex items-center gap-2 text-xs text-muted-foreground">
          Height (m)
          <Input
            key={selectedFixtures.map((f) => f.instance.id).join(",")}
            type="number"
            className="h-11 w-24"
            min={0}
            max={STAGE_HEIGHT_M}
            step={0.25}
            disabled={selectedFixtures.length === 0}
            defaultValue={selectedHeight}
            onChange={(e) => {
              const z = parseFloat(e.target.value);
              if (Number.isFinite(z)) setSelectedHeight(z);
            }}
          />
        </label>
        <span className="ml-auto text-xs text-muted-foreground">
          {selected.length > 0 ? `${selected.length} selected` : "Click a fixture to select, drag to move"}
        </span>
      </div>
      <div className="flex flex-1 items-center justify-center overflow-hidden p-4">
        <svg
          ref={svgRef}
          viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
          className="max-h-full max-w-full touch-none select-none rounded-md border border-border bg-card"
          style={{ aspectRatio: `${STAGE_WIDTH_M} / ${STAGE_DEPTH_M}`, width: "100%" }}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={() => setDrag(null)}
          onPointerDown={clear}
        >
          {Array.from({ length: STAGE_WIDTH_M + 1 }, (_, i) => (
            <line key={`v${i}`} x1={i * PX_PER_M} x2={i * PX_PER_M} y1={0} y2={VIEW_H} className="stroke-border" strokeWidth={1} />
          ))}
          {Array.from({ length: STAGE_DEPTH_M + 1 }, (_, i) => (
            <line key={`h${i}`} y1={i * PX_PER_M} y2={i * PX_PER_M} x1={0} x2={VIEW_W} className="stroke-border" strokeWidth={1} />
          ))}
          <text x={VIEW_W / 2} y={VIEW_H - 8} textAnchor="middle" className="fill-muted-foreground text-[14px] uppercase tracking-widest">
            Front of stage
          </text>
          {fixtures.map(({ instance, def, mode }) => {
            const entry = layout.get(instance.id);
            if (!entry) return null;
            const dragging = drag?.moved && drag.ids.includes(instance.id);
            const pos = dragging
              ? clampToStage({ x: entry.position.x + drag.dx, y: entry.position.y + drag.dy, z: entry.position.z })
              : entry.position;
            const px = metresToPx(pos, VIEW_W, VIEW_H);
            const frame = frames[instance.universe];
            const look = deriveFixtureLook(def, mode, instance, (_u, ch) => frame?.[ch - 1] ?? 0);
            const isSelected = selected.includes(instance.fixtureNumber);
            const lit = litColor(look);
            const pan = look.panDeg ?? 0;
            return (
              <g
                key={instance.id}
                transform={`translate(${px.x} ${px.y})`}
                className="cursor-grab active:cursor-grabbing"
                onPointerDown={(e) => onFixturePointerDown(e, instance.id, instance.fixtureNumber)}
              >
                <title>{`#${instance.fixtureNumber} ${instance.name} — ${def.manufacturer} ${def.model}`}</title>
                <circle r={FIXTURE_RADIUS * 1.9} fill={rgbToCss(look.color)} opacity={look.intensity * 0.35} />
                {look.moving && look.panDeg !== null && (
                  <line
                    x1={0}
                    y1={0}
                    x2={Math.sin((pan * Math.PI) / 180) * FIXTURE_RADIUS * 1.7}
                    y2={-Math.cos((pan * Math.PI) / 180) * FIXTURE_RADIUS * 1.7}
                    stroke={rgbToCss(look.color)}
                    strokeWidth={4}
                    strokeLinecap="round"
                    opacity={Math.max(0.3, look.intensity)}
                  />
                )}
                <circle
                  r={FIXTURE_RADIUS}
                  fill={rgbToCss(lit)}
                  className={isSelected ? "stroke-primary" : "stroke-muted-foreground"}
                  strokeWidth={isSelected ? 4 : 2}
                />
                {/* An extra hit area so dark fixtures are easy to grab with a finger. */}
                <circle r={FIXTURE_RADIUS + 10} fill="transparent" />
                <text y={FIXTURE_RADIUS + 18} textAnchor="middle" className="fill-foreground text-[14px]">
                  {instance.fixtureNumber}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
    </div>
  );
}
