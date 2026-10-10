/** Channel source codes returned by `engine_get_universe_sources` (merge::ChannelSource). */
export const SOURCE_DEFAULT = 0;
export const SOURCE_PROGRAMMER = 1;
export const SOURCE_PLAYBACK = 2;
export const SOURCE_TRACKED = 3;
export const SOURCE_EFFECT = 4;

export type ChannelSourceName = "default" | "programmer" | "playback" | "tracked" | "effect";

const NAMES: ChannelSourceName[] = ["default", "programmer", "playback", "tracked", "effect"];

export function sourceName(code: number | undefined): ChannelSourceName {
  return NAMES[code ?? 0] ?? "default";
}

export const SOURCE_LABELS: Record<ChannelSourceName, string> = {
  default: "Default",
  programmer: "Programmer",
  playback: "Cuelist",
  tracked: "Tracked",
  effect: "Effect",
};

/** Cell text/background classes per source, using only design-system tokens. */
export const SOURCE_CLASSES: Record<ChannelSourceName, string> = {
  default: "text-muted-foreground",
  programmer: "bg-primary/25 text-foreground",
  playback: "bg-success/25 text-foreground",
  tracked: "bg-success/10 text-muted-foreground",
  effect: "bg-accent text-foreground ring-1 ring-inset ring-primary/60",
};
