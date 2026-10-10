import type { Preferences } from "./showfile/generated";

export const BASE_FONT_PX = 16;
export const MIN_UI_SCALE = 0.75;
export const MAX_UI_SCALE = 1.5;

/** The root font size for a UI scale. Everything in the layout is in rem,
 * so scaling the root scales the whole interface. */
export function rootFontSizePx(scale: number): number {
  if (!Number.isFinite(scale)) return BASE_FONT_PX;
  return Math.round(BASE_FONT_PX * Math.min(MAX_UI_SCALE, Math.max(MIN_UI_SCALE, scale)) * 100) / 100;
}

interface RootLike {
  style: { fontSize: string };
  classList: { toggle(name: string, force?: boolean): boolean };
}

/** Applies the display-affecting preferences to a document root element. */
export function applyPreferencesToRoot(root: RootLike, prefs: Pick<Preferences, "uiScale" | "touchMode">): void {
  root.style.fontSize = `${rootFontSizePx(prefs.uiScale)}px`;
  root.classList.toggle("touch", prefs.touchMode);
}
