/**
 * Pure shaping from a validated chart result to plot geometry. Rows are
 * positional, so a binding resolves to the first column with that name.
 */
import type { QueryResult } from './contract.js';
import { parseInstant } from './format.js';

/** One plotted series. */
export interface Series {
  readonly label: string;
  readonly points: readonly { readonly x: number; readonly y: number }[];
}

/** Series past this count fold into one summed "other" series (REQ-025). */
export const MAX_SERIES = 20;

/** Position of the first column named `name`. */
export function columnIndex(result: QueryResult, name: string): number {
  return result.columns.findIndex((column) => column.name === name);
}

/** A cell as a number; Arrow JSON encodes 64-bit and decimal values as strings. */
export function numeric(value: unknown): number {
  return typeof value === 'number' ? value : Number(value);
}

/**
 * Build time series: one per `y` column, or per `series` value × `y` when a
 * series column is bound. Groups are keyed by the typed (value, `y`) pair, so
 * distinct groups never merge however their text reads. Past
 * {@link MAX_SERIES}, the largest totals keep their own line and the rest are
 * summed per instant as "other", qualified when an authored series already
 * has that caption.
 */
export function timeSeries(result: QueryResult, x: string, ys: readonly string[], series?: string): Series[] {
  const xi = columnIndex(result, x);
  const si = series === undefined ? -1 : columnIndex(result, series);
  const lines = new Map<string, { group: unknown; y: string; points: Map<number, number> }>();
  for (const row of result.rows) {
    const at = parseInstant(row[xi]);
    for (const y of ys) {
      const value = row[columnIndex(result, y)];
      if (value === null || Number.isNaN(at)) continue;
      const key = si < 0 ? y : JSON.stringify([row[si], y]);
      const line = lines.get(key) ?? { group: si < 0 ? undefined : row[si], y, points: new Map<number, number>() };
      line.points.set(at, (line.points.get(at) ?? 0) + numeric(value));
      lines.set(key, line);
    }
  }
  let entries = [...lines.values()];
  let other: Map<number, number> | undefined;
  if (entries.length > MAX_SERIES) {
    const total = (points: Map<number, number>) => [...points.values()].reduce((sum, value) => sum + value, 0);
    entries.sort((a, b) => total(b.points) - total(a.points));
    other = new Map<number, number>();
    for (const { points } of entries.slice(MAX_SERIES - 1))
      for (const [at, value] of points) other.set(at, (other.get(at) ?? 0) + value);
    entries = entries.slice(0, MAX_SERIES - 1);
  }
  const labels = captions(entries, si >= 0, ys.length > 1);
  const plotted = entries.map(({ points }, index) => [labels[index], points] as const);
  if (other) {
    // An authored series may already be called "other"; the aggregate's caption must still name only itself.
    let label = 'other';
    for (let n = 1; labels.includes(label); n++) label = n === 1 ? 'other (aggregate)' : `other (aggregate ${n})`;
    plotted.push([label, other]);
  }
  return plotted.map(([label, points]) => ({
    label,
    points: [...points].sort(([a], [b]) => a - b).map(([x, y]) => ({ x, y }))
  }));
}

/**
 * Distinct captions for plotted groups. Each reads as before unless it
 * matches another; then it becomes its group's JSON spelling (value, plus `y`
 * when several are bound), which is unique per group and tells null from the
 * text "null". Repeats until no caption matches another, since a JSON spelling
 * can equal some other group's authored text.
 */
function captions(groups: readonly { group: unknown; y: string }[], bySeries: boolean, byY: boolean): string[] {
  const labels = groups.map(({ group, y }) => (!bySeries ? y : byY ? `${String(group)} ${y}` : String(group)));
  const qualified = new Set<number>();
  for (;;) {
    const clash = labels.flatMap((label, index) => (!qualified.has(index) && labels.some((other, at) => at !== index && other === label) ? [index] : []));
    if (clash.length === 0) return labels;
    for (const index of clash) {
      const { group, y } = groups[index];
      labels[index] = byY ? JSON.stringify([group, y]).slice(1, -1) : JSON.stringify(group);
      qualified.add(index);
    }
  }
}

/** Bar rows: one category label with a value per `y`. */
export function barRows(result: QueryResult, x: string, ys: readonly string[]): { label: string; values: (number | null)[] }[] {
  const xi = columnIndex(result, x);
  const yi = ys.map((y) => columnIndex(result, y));
  return result.rows.map((row) => ({
    label: String(row[xi] ?? '∅'),
    values: yi.map((index) => (row[index] === null ? null : numeric(row[index])))
  }));
}
