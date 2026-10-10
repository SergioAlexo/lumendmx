import { Component, useEffect, useMemo, useRef, type ReactNode } from "react";
import { Canvas, useFrame } from "@react-three/fiber";
import { OrbitControls } from "@react-three/drei";
import * as THREE from "three";
import { useUniverseChannelsStore } from "../store/useUniverseChannelsStore";
import { useSelectionStore } from "../store/useSelectionStore";
import { deriveFixtureLook } from "../lib/visualisation/fixtureLook";
import {
  DEFAULT_FIXTURE_HEIGHT_M,
  layoutFixtures,
  STAGE_DEPTH_M,
  STAGE_HEIGHT_M,
  STAGE_WIDTH_M,
} from "../lib/visualisation/stage";
import { useVisualFixtures, type VisualFixture } from "../lib/visualisation/useVisualFixtures";

/** Resolves a design-system HSL token (e.g. `--background`) to a three.js colour. */
function tokenColor(name: string, fallback: string): THREE.Color {
  try {
    const raw = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    const [h, s, l] = raw.split(/\s+/).map((v) => parseFloat(v));
    if ([h, s, l].every(Number.isFinite)) return new THREE.Color().setHSL(h / 360, s / 100, l / 100);
  } catch {
    // fall through
  }
  return new THREE.Color(fallback);
}

// Unit cone, apex at the origin opening downwards (-Y): scaled per beam to its radius and length.
const BEAM_GEOMETRY = new THREE.ConeGeometry(1, 1, 32, 1, true).translate(0, -0.5, 0);
const POOL_GEOMETRY = new THREE.CircleGeometry(1, 32);

/** Stage coordinates (metres, y upstage, z up) to three.js (x right, y up, z towards the audience). */
function toScene(p: { x: number; y: number; z: number }): [number, number, number] {
  return [p.x - STAGE_WIDTH_M / 2, p.z, STAGE_DEPTH_M / 2 - p.y];
}

interface FixtureNodeProps {
  fixture: VisualFixture;
  position: { x: number; y: number; z: number };
  selected: boolean;
}

/** One fixture: head, beam cone and floor pool. Updates its materials and
 * rotation imperatively from the latest engine frame - no React re-render per frame. */
function FixtureNode({ fixture, position, selected }: FixtureNodeProps) {
  const { instance, def, mode } = fixture;
  const group = useRef<THREE.Group>(null);
  const headMat = useRef<THREE.MeshBasicMaterial>(null);
  const beam = useRef<THREE.Mesh>(null);
  const beamMat = useRef<THREE.MeshBasicMaterial>(null);
  const pool = useRef<THREE.Mesh>(null);
  const poolMat = useRef<THREE.MeshBasicMaterial>(null);
  const lastFrame = useRef<Uint8Array | undefined>(undefined);
  const stale = useRef(true);
  const length = Math.max(position.z, 0.5) * 1.1;
  const color = useMemo(() => new THREE.Color(), []);

  // Re-render the look when the fixture itself changes (re-patched, moved, new height).
  useEffect(() => {
    stale.current = true;
  }, [length, def, mode, instance]);

  useFrame(() => {
    const frame = useUniverseChannelsStore.getState().channels[instance.universe];
    // Frames arrive at ~20Hz; skip the work on render frames in between.
    if (frame === lastFrame.current && !stale.current) return;
    lastFrame.current = frame;
    stale.current = false;
    const look = deriveFixtureLook(def, mode, instance, (_u, ch) => frame?.[ch - 1] ?? 0);
    color.setRGB(look.color.r / 255, look.color.g / 255, look.color.b / 255);
    if (headMat.current) headMat.current.color.copy(color).multiplyScalar(0.15 + 0.85 * look.intensity);
    if (beamMat.current) {
      beamMat.current.color.copy(color);
      beamMat.current.opacity = look.intensity * 0.28;
    }
    if (poolMat.current) {
      poolMat.current.color.copy(color);
      poolMat.current.opacity = look.intensity * 0.5;
    }
    const radius = Math.tan((look.beamAngleDeg * Math.PI) / 360) * length;
    if (beam.current) {
      beam.current.scale.set(radius, length, radius);
      beam.current.visible = look.intensity > 0.01;
    }
    if (pool.current) {
      pool.current.scale.set(radius, radius, 1);
      pool.current.visible = look.intensity > 0.01;
    }
    if (group.current) {
      group.current.rotation.set(((look.tiltDeg ?? 0) * Math.PI) / 180, ((look.panDeg ?? 0) * Math.PI) / 180, 0, "YXZ");
    }
  });

  const select = useSelectionStore.getState();
  return (
    <group position={toScene(position)}>
      <mesh
        onClick={(e) => {
          e.stopPropagation();
          if (e.shiftKey || e.metaKey || e.ctrlKey) select.toggle(instance.fixtureNumber);
          else select.select([instance.fixtureNumber]);
        }}
      >
        <boxGeometry args={[0.35, 0.3, 0.35]} />
        <meshBasicMaterial ref={headMat} color="#222" />
      </mesh>
      {selected && (
        <mesh position={[0, 0, 0]}>
          <boxGeometry args={[0.45, 0.4, 0.45]} />
          <meshBasicMaterial color={tokenColor("--primary", "#e11d48")} wireframe />
        </mesh>
      )}
      <group ref={group}>
        <mesh ref={beam} geometry={BEAM_GEOMETRY} raycast={() => null}>
          <meshBasicMaterial
            ref={beamMat}
            transparent
            opacity={0}
            depthWrite={false}
            blending={THREE.AdditiveBlending}
            side={THREE.DoubleSide}
          />
        </mesh>
        <mesh ref={pool} geometry={POOL_GEOMETRY} position={[0, -length, 0]} rotation={[Math.PI / 2, 0, 0]} raycast={() => null}>
          <meshBasicMaterial
            ref={poolMat}
            transparent
            opacity={0}
            depthWrite={false}
            blending={THREE.AdditiveBlending}
            side={THREE.DoubleSide}
          />
        </mesh>
      </group>
    </group>
  );
}

function StageSet() {
  const floor = useMemo(() => tokenColor("--card", "#111"), []);
  const grid = useMemo(() => tokenColor("--border", "#333"), []);
  const truss = useMemo(() => tokenColor("--muted-foreground", "#888"), []);
  const trussY = DEFAULT_FIXTURE_HEIGHT_M + 0.25;
  const postX = STAGE_WIDTH_M / 2;
  return (
    <group>
      <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, -0.01, 0]}>
        <planeGeometry args={[STAGE_WIDTH_M, STAGE_DEPTH_M]} />
        <meshBasicMaterial color={floor} />
      </mesh>
      <gridHelper args={[Math.max(STAGE_WIDTH_M, STAGE_DEPTH_M), Math.max(STAGE_WIDTH_M, STAGE_DEPTH_M), grid, grid]} position={[0, 0, 0]} />
      {/* Overhead truss: a front and a rear pipe plus four posts. */}
      {[STAGE_DEPTH_M / 2 - 1, -STAGE_DEPTH_M / 2 + 1].map((z) => (
        <mesh key={z} position={[0, trussY, z]} rotation={[0, 0, Math.PI / 2]}>
          <cylinderGeometry args={[0.05, 0.05, STAGE_WIDTH_M, 8]} />
          <meshBasicMaterial color={truss} />
        </mesh>
      ))}
      {[-postX, postX].flatMap((x) =>
        [STAGE_DEPTH_M / 2 - 1, -STAGE_DEPTH_M / 2 + 1].map((z) => (
          <mesh key={`${x}${z}`} position={[x, Math.min(trussY, STAGE_HEIGHT_M) / 2, z]}>
            <cylinderGeometry args={[0.05, 0.05, trussY, 8]} />
            <meshBasicMaterial color={truss} />
          </mesh>
        )),
      )}
    </group>
  );
}

function Scene() {
  const fixtures = useVisualFixtures();
  const selected = useSelectionStore((s) => s.selected);
  const layout = useMemo(() => layoutFixtures(fixtures.map((f) => f.instance)), [fixtures]);
  return (
    <>
      <StageSet />
      {fixtures.map((f) => {
        const entry = layout.get(f.instance.id);
        if (!entry) return null;
        return (
          <FixtureNode
            key={f.instance.id}
            fixture={f}
            position={entry.position}
            selected={selected.includes(f.instance.fixtureNumber)}
          />
        );
      })}
      <OrbitControls target={[0, 1.5, 0]} maxPolarAngle={Math.PI / 2 - 0.02} minDistance={3} maxDistance={40} makeDefault />
    </>
  );
}

class WebGlBoundary extends Component<{ children: ReactNode }, { error: string | null }> {
  state = { error: null as string | null };
  static getDerivedStateFromError(err: unknown) {
    return { error: err instanceof Error ? err.message : String(err) };
  }
  render() {
    if (this.state.error) {
      return (
        <div className="flex h-full items-center justify-center p-8 text-sm text-muted-foreground">
          The 3D view could not start ({this.state.error}). WebGL may be unavailable in this environment.
        </div>
      );
    }
    return this.props.children;
  }
}

/** Orbitable 3D stage with live beams. Loaded lazily (see App.tsx) so three.js
 * stays out of the main bundle. */
export default function Visualizer3D() {
  const bg = useMemo(() => tokenColor("--background", "#0b0b0f"), []);
  const empty = useVisualFixtures().length === 0;
  return (
    <div className="relative h-full w-full">
      <WebGlBoundary>
        <Canvas camera={{ position: [0, 6, STAGE_DEPTH_M + 4], fov: 50 }} dpr={[1, 2]} gl={{ antialias: true }}>
          <color attach="background" args={[bg]} />
          <Scene />
        </Canvas>
      </WebGlBoundary>
      <div className="pointer-events-none absolute left-4 top-3 text-xs text-muted-foreground">
        {empty ? "No fixtures patched." : "Drag to orbit, scroll to zoom, click a fixture to select. Arrange fixtures in the Stage plot."}
      </div>
    </div>
  );
}
