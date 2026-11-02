// Groups ("blocks") on the playback strip. A control belongs to a group by name
// (its `panelGroup` param); the block's box is worked out from its members, so
// there is nothing to size, and moving the block moves them.

export type Rect = { x: number; y: number; w: number; h: number };

/** Space around the members, and the title row above them. */
export const GROUP_PAD = 12;
export const GROUP_HEADER_H = 24;

/** The group a control is in, if any. */
export function groupName(params: Record<string, unknown>): string | null {
  const name = params.panelGroup;
  return typeof name === "string" && name.trim() !== "" ? name.trim() : null;
}

/** How wide a folded block's title is: roughly its text plus the chevron and count. */
function titleWidth(name: string): number {
  return Math.max(96, name.length * 7 + 64);
}

/** Each group's box: its members' bounds plus padding and the title row. A folded group is just
 * its title, where the members' top-left was. */
export function groupBoxes(items: { name: string | null; rect: Rect }[], folded: ReadonlySet<string>): Map<string, Rect> {
  const bounds = new Map<string, { x0: number; y0: number; x1: number; y1: number }>();
  for (const { name, rect } of items) {
    if (name === null) continue;
    const b = bounds.get(name);
    const [x1, y1] = [rect.x + rect.w, rect.y + rect.h];
    if (b) bounds.set(name, { x0: Math.min(b.x0, rect.x), y0: Math.min(b.y0, rect.y), x1: Math.max(b.x1, x1), y1: Math.max(b.y1, y1) });
    else bounds.set(name, { x0: rect.x, y0: rect.y, x1, y1 });
  }
  const boxes = new Map<string, Rect>();
  for (const [name, b] of bounds) {
    const [x, y] = [b.x0 - GROUP_PAD, b.y0 - GROUP_PAD - GROUP_HEADER_H];
    if (folded.has(name)) boxes.set(name, { x, y, w: titleWidth(name), h: GROUP_HEADER_H });
    else boxes.set(name, { x, y, w: b.x1 - b.x0 + 2 * GROUP_PAD, h: b.y1 - b.y0 + 2 * GROUP_PAD + GROUP_HEADER_H });
  }
  return boxes;
}

/** The group whose box holds the middle of `rect`, if any. */
export function groupAt(boxes: ReadonlyMap<string, Rect>, rect: Rect): string | null {
  const [cx, cy] = [rect.x + rect.w / 2, rect.y + rect.h / 2];
  for (const [name, b] of boxes) if (cx >= b.x && cx <= b.x + b.w && cy >= b.y && cy <= b.y + b.h) return name;
  return null;
}
