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
 * series column is bound. Past {@link MAX_SERIES}, the largest totals keep
 * their own line and the rest are summed per instant as "other".
 */
export function timeSeries(result: QueryResult, x: string, ys: readonly string[], series?: string): Series[] {
  const xi = columnIndex(result, x);
  const si = series === undefined ? -1 : columnIndex(result, series);
  const lines = new Map<string, Map<number, number>>();
  for (const row of result.rows) {
    const at = parseInstant(row[xi]);
    for (const y of ys) {
      const value = row[columnIndex(result, y)];
      if (value === null || Number.isNaN(at)) continue;
      const label = si < 0 ? y : ys.length > 1 ? `${String(row[si])} ${y}` : String(row[si]);
      const points = lines.get(label) ?? new Map<number, number>();
      points.set(at, (points.get(at) ?? 0) + numeric(value));
      lines.set(label, points);
    }
  }
  let entries = [...lines];
  if (entries.length > MAX_SERIES) {
    const total = (points: Map<number, number>) => [...points.values()].reduce((sum, value) => sum + value, 0);
    entries.sort(([, a], [, b]) => total(b) - total(a));
    const other = new Map<number, number>();
    for (const [, points] of entries.slice(MAX_SERIES - 1))
      for (const [at, value] of points) other.set(at, (other.get(at) ?? 0) + value);
    entries = [...entries.slice(0, MAX_SERIES - 1), ['other', other]];
  }
  return entries.map(([label, points]) => ({
    label,
    points: [...points].sort(([a], [b]) => a - b).map(([x, y]) => ({ x, y }))
  }));
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
