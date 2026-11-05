/**
 * Pure view-grid geometry: where each panel lands and how an edit moves it.
 * Rendering and editing in `WyrdView` read only these results.
 */
import type { Layout } from './schemas.js';

/** Grid columns. */
export const COLUMNS = 12;
/** Row height in CSS px. */
export const ROW = 40;
/** Gap between rows and columns in CSS px. */
export const GAP = 12;
/** Views narrower than this, in CSS px, stack panels and disable editing. */
export const NARROW = 720;

/** A resolved grid box; an `auto` panel has no layout, and `h` is the whole rows its content occupies. */
export interface Placement extends Layout {
  auto: boolean;
}

/** The whole rows, gaps included, that `px` of content needs; at least one. */
export const rowsFor = (px: number) => Math.max(1, Math.ceil((px + GAP) / (ROW + GAP)));

const overlaps = (a: Layout, b: Layout) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

/**
 * Places panels in document order. An unplaced panel spans every column
 * below all earlier panels, occupying the rows its measured content height
 * (`heights`, in px, by position; one row until measured) needs. A panel
 * overlapping an earlier one moves down until it is clear. The inputs are
 * never changed.
 */
export function place(layouts: (Layout | undefined)[], heights: readonly number[] = []): Placement[] {
  const placed: Placement[] = [];
  for (const [index, layout] of layouts.entries()) {
    const bottom = Math.max(0, ...placed.map((each) => each.y + each.h));
    const next: Placement = layout ? { ...layout, auto: false } : { x: 0, y: bottom, w: COLUMNS, h: rowsFor(heights[index] ?? 0), auto: true };
    for (let hit = placed.find((each) => overlaps(each, next)); hit; hit = placed.find((each) => overlaps(each, next))) next.y = hit.y + hit.h;
    placed.push(next);
  }
  return placed;
}

/** Moves (`resize` false) or resizes a layout by whole cells, kept inside the grid. */
export function edit(from: Layout, dx: number, dy: number, resize: boolean): Layout {
  const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));
  if (resize) return { ...from, w: clamp(from.w + dx, 1, COLUMNS - from.x), h: clamp(from.h + dy, 1, 48) };
  return { ...from, x: clamp(from.x + dx, 0, COLUMNS - from.w), y: Math.max(0, from.y + dy) };
}

/** The fixed CSS height of `h` rows, gaps included. */
export const rowsHeight = (h: number) => `${h * ROW + (h - 1) * GAP}px`;
