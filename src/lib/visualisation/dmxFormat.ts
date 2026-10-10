export type ValueFormat = "raw" | "percent" | "hex";

/** A 0-255 DMX value as text in the chosen format. */
export function formatDmxValue(value: number, format: ValueFormat): string {
  const v = Math.max(0, Math.min(255, Math.round(value)));
  switch (format) {
    case "percent":
      return String(Math.round((v / 255) * 100));
    case "hex":
      return v.toString(16).toUpperCase().padStart(2, "0");
    default:
      return String(v);
  }
}
