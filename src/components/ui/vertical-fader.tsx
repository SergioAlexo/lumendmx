import * as React from "react";
import { cn } from "../../lib/utils";

interface VerticalFaderProps {
  /** 0..max */
  value: number;
  max?: number;
  onChange: (value: number) => void;
  title?: string;
  className?: string;
  /** Shown above the cap while dragging / always, e.g. "75%". */
  label?: string;
  disabled?: boolean;
  /** "#rrggbb" for the fill and cap; the theme colour when omitted. */
  color?: string;
}

const STEP = 1;
const BIG_STEP = 10;

/** A touch-friendly vertical fader: drag anywhere on the track (44px wide),
 * or focus it and use the arrow / PageUp / PageDown / Home / End keys.
 * Built on pointer events rather than a rotated range input because
 * WebKit and Chromium disagree about vertical range inputs. */
export function VerticalFader({ value, max = 100, onChange, title, className, label, disabled, color }: VerticalFaderProps) {
  const trackRef = React.useRef<HTMLDivElement>(null);
  const clamped = Math.min(max, Math.max(0, value));

  const fromPointer = (clientY: number) => {
    const rect = trackRef.current?.getBoundingClientRect();
    if (!rect || rect.height === 0) return;
    const ratio = 1 - (clientY - rect.top) / rect.height;
    onChange(Math.round(Math.min(1, Math.max(0, ratio)) * max));
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const step = e.shiftKey ? BIG_STEP : STEP;
    let next: number | null = null;
    switch (e.key) {
      case "ArrowUp":
      case "ArrowRight":
        next = clamped + step;
        break;
      case "ArrowDown":
      case "ArrowLeft":
        next = clamped - step;
        break;
      case "PageUp":
        next = clamped + BIG_STEP;
        break;
      case "PageDown":
        next = clamped - BIG_STEP;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = max;
        break;
    }
    if (next === null) return;
    e.preventDefault();
    onChange(Math.min(max, Math.max(0, next)));
  };

  return (
    <div
      ref={trackRef}
      role="slider"
      tabIndex={disabled ? -1 : 0}
      aria-label={title}
      aria-orientation="vertical"
      aria-valuemin={0}
      aria-valuemax={max}
      aria-valuenow={Math.round(clamped)}
      aria-disabled={disabled}
      title={title}
      className={cn(
        "relative h-32 w-11 shrink-0 touch-none select-none rounded-md border border-border bg-muted focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
        disabled && "opacity-50",
        className,
      )}
      onPointerDown={(e) => {
        if (disabled) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        e.currentTarget.focus();
        fromPointer(e.clientY);
      }}
      onPointerMove={(e) => {
        if (!disabled && e.currentTarget.hasPointerCapture(e.pointerId)) fromPointer(e.clientY);
      }}
      onKeyDown={disabled ? undefined : onKeyDown}
    >
      <div
        className={cn("absolute inset-x-0 bottom-0 rounded-b-md", !color && "bg-primary/30")}
        style={{ height: `${(clamped / max) * 100}%`, ...(color && { backgroundColor: `${color}4d` }) }}
      />
      <div
        className={cn("absolute inset-x-0.5 h-3 -translate-y-1/2 rounded-sm shadow", !color && "bg-primary")}
        style={{ top: `${100 - (clamped / max) * 100}%`, ...(color && { backgroundColor: color }) }}
      />
      {label !== undefined && (
        <span className="pointer-events-none absolute inset-x-0 top-0.5 text-center text-[10px] tabular-nums text-muted-foreground">
          {label}
        </span>
      )}
    </div>
  );
}
