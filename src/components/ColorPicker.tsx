import { useRef } from "react";
import { hexToRgb, hsvToRgb, rgbToHex, rgbToHsv, type Rgb } from "../lib/color";
import { Input } from "./ui/input";
import { ValueSlider } from "./ui/value-slider";

const WHEEL_SIZE = 160;

/** HSI-style wheel (hue = angle, saturation = radius) plus a brightness
 * slider and RGB/hex fields (BUILD_PLAN Phase 5's colour picker). Uses HSV
 * under the hood -- see src/lib/color.ts's module docs for why. CMY faders
 * specifically weren't added: redundant with RGB for this app's channel
 * model, and would just add conversion surface for no functional gain. */
export function ColorPicker({ value, onChange }: { value: Rgb; onChange: (rgb: Rgb) => void }) {
  const wheelRef = useRef<HTMLDivElement>(null);
  const hsv = rgbToHsv(value);

  const setFromWheelEvent = (e: { clientX: number; clientY: number }) => {
    const rect = wheelRef.current?.getBoundingClientRect();
    if (!rect) return;
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    const dx = e.clientX - cx;
    const dy = e.clientY - cy;
    const radius = Math.min(1, Math.hypot(dx, dy) / (rect.width / 2));
    let angle = (Math.atan2(dy, dx) * 180) / Math.PI;
    if (angle < 0) angle += 360;
    onChange(hsvToRgb({ h: angle, s: radius * 100, v: hsv.v }));
  };

  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    setFromWheelEvent(e);
  };

  const handleDrag = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.buttons !== 1) return;
    setFromWheelEvent(e);
  };

  const handleAngle = (hsv.h * Math.PI) / 180;
  const handleRadius = (hsv.s / 100) * (WHEEL_SIZE / 2);
  const handleX = WHEEL_SIZE / 2 + Math.cos(handleAngle) * handleRadius;
  const handleY = WHEEL_SIZE / 2 + Math.sin(handleAngle) * handleRadius;

  return (
    <div className="flex flex-col items-center gap-3">
      <div
        ref={wheelRef}
        onPointerDown={handlePointerDown}
        onPointerMove={handleDrag}
        className="relative shrink-0 cursor-crosshair rounded-full"
        style={{
          width: WHEEL_SIZE,
          height: WHEEL_SIZE,
          background:
            "radial-gradient(circle, white 0%, transparent 100%), conic-gradient(from 0deg, red, yellow, lime, cyan, blue, magenta, red)",
          filter: `brightness(${Math.max(0.15, hsv.v / 100)})`,
        }}
      >
        <div
          className="absolute h-3 w-3 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow"
          style={{ left: handleX, top: handleY, backgroundColor: rgbToHex(value) }}
        />
      </div>

      <div className="flex w-full items-center gap-2 text-xs text-muted-foreground">
        Brightness
        <ValueSlider
          className="flex-1"
          max={100}
          value={Math.round(hsv.v)}
          onChange={(v) => onChange(hsvToRgb({ ...hsv, v }))}
          title="Brightness"
        />
      </div>

      <div className="flex w-full flex-col gap-1.5">
        {(["r", "g", "b"] as const).map((channel) => (
          <div key={channel} className="flex items-center gap-2 text-xs text-muted-foreground">
            <span className="w-4 uppercase">{channel}</span>
            <ValueSlider
              className="flex-1"
              max={255}
              value={value[channel]}
              onChange={(v) => onChange({ ...value, [channel]: v })}
              title={channel.toUpperCase()}
            />
          </div>
        ))}
      </div>

      <Input
        className="w-28 text-center font-mono text-xs"
        value={rgbToHex(value)}
        onChange={(e) => {
          const rgb = hexToRgb(e.target.value);
          if (rgb) onChange(rgb);
        }}
      />
    </div>
  );
}
