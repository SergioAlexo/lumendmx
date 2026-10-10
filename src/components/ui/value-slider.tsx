import * as React from "react";
import { cn } from "../../lib/utils";

interface ValueSliderProps {
  value: number;
  min?: number;
  max: number;
  onChange: (value: number) => void;
  title?: string;
  className?: string;
  /** Width class for the numeric field. */
  fieldClassName?: string;
}

const BIG_STEP = 10;

/** The value a stepping key moves `base` to, or null for any other key.
 * Arrows step by 1 (Shift: 10), PageUp/PageDown by 10, Home/End jump. */
function steppedValue(e: React.KeyboardEvent, base: number, min: number, max: number): number | null {
  const step = e.shiftKey ? BIG_STEP : 1;
  switch (e.key) {
    case "ArrowUp":
    case "ArrowRight":
      return base + step;
    case "ArrowDown":
    case "ArrowLeft":
      return base - step;
    case "PageUp":
      return base + BIG_STEP;
    case "PageDown":
      return base - BIG_STEP;
    case "Home":
      return min;
    case "End":
      return max;
    default:
      return null;
  }
}

/** A range slider paired with an editable number field.
 *
 * Arrow keys / PageUp / PageDown / Home / End work on both. The field
 * commits on Enter or blur; Escape reverts.
 *
 * The slider focuses itself on pointer down because WebKit (Tauri's macOS
 * webview) doesn't focus range inputs on click, so arrow keys would
 * otherwise go nowhere after a drag. */
export function ValueSlider({ value, min = 0, max, onChange, title, className, fieldClassName }: ValueSliderProps) {
  const [draft, setDraft] = React.useState<string | null>(null);
  const clamp = (n: number) => Math.min(max, Math.max(min, Math.round(n)));

  const commit = (n: number) => {
    const next = clamp(n);
    if (next !== value) onChange(next);
    return next;
  };

  const commitDraft = () => {
    if (draft === null) return;
    const parsed = Number(draft.trim());
    if (draft.trim() !== "" && Number.isFinite(parsed)) commit(parsed);
    setDraft(null);
  };

  return (
    <div className={cn("flex min-w-0 items-center gap-2", className)}>
      <input
        type="range"
        min={min}
        max={max}
        value={value}
        title={title}
        onPointerDown={(e) => e.currentTarget.focus()}
        onChange={(e) => commit(Number(e.target.value))}
        onKeyDown={(e) => {
          const next = steppedValue(e, value, min, max);
          if (next === null) return;
          e.preventDefault();
          commit(next);
        }}
        className="h-1.5 min-w-0 flex-1 accent-primary"
      />
      <input
        type="text"
        inputMode="numeric"
        aria-label={title}
        value={draft ?? String(value)}
        onFocus={(e) => {
          setDraft(String(value));
          e.currentTarget.select();
        }}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commitDraft}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.currentTarget.blur(); // blur commits
          } else if (e.key === "Escape") {
            setDraft(null);
            e.currentTarget.blur();
          } else if (e.key !== "ArrowLeft" && e.key !== "ArrowRight" && e.key !== "Home" && e.key !== "End") {
            // Left/Right/Home/End keep moving the text cursor in the field.
            const typed = Number(draft);
            const base = draft !== null && Number.isFinite(typed) ? typed : value;
            const next = steppedValue(e, base, min, max);
            if (next === null) return;
            e.preventDefault();
            setDraft(String(commit(next)));
          }
        }}
        className={cn(
          "h-6 w-10 shrink-0 rounded border border-input bg-transparent px-1 text-right text-xs tabular-nums focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
          fieldClassName,
        )}
      />
    </div>
  );
}
