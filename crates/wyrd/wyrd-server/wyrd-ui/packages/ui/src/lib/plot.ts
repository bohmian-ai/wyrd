/**
 * Pure shaping from a validated chart result to plot geometry. Rows are
 * positional, so a binding resolves to the first column with that name.
 */
import type { QueryResult } from './contract.js';
import { parseInstant } from './format.js';

/** One plotted series. `key` identifies it across reads; `label` only captions it. */
export interface Series {
  readonly key: string;
  readonly label: string;
  readonly points: readonly { readonly x: number; readonly y: number; readonly text?: string }[];
}

/** Series past this count fold into one summed "other" series (REQ-025). */
export const MAX_SERIES = 20;

/**
 * The series palette, in order: every chart line and waterfall service takes
 * the colour at its position modulo five, always beside a dash, marker or
 * label so colour never carries identity alone.
 */
export const SERIES_COLORS = ['var(--declare)', 'var(--observe)', 'var(--series-3)', 'var(--series-4)', 'var(--series-5)'] as const;

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
 * summed per instant as "other", or "other (aggregate)" when an authored
 * series is already called "other".
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
  // An authored series may already be called "other"; the aggregate's caption must still name only itself.
  if (other) plotted.push([labels.includes('other') ? 'other (aggregate)' : 'other', other]);
  return plotted.map(([label, points]) => ({
    key: label,
    label,
    points: [...points].sort(([a], [b]) => a - b).map(([x, y]) => ({ x, y }))
  }));
}

/** The typed identity of a row: the JSON of its `identity` cells, so null never meets the text "null" or "not set". */
export function identityKey(result: QueryResult, row: readonly unknown[], identity: readonly string[]): string {
  return JSON.stringify(identity.map((name) => row[columnIndex(result, name)] ?? null));
}

/**
 * Build one series per typed identity, in first-seen order, with every
 * recorded point kept: no instant is summed and no series is folded. Each
 * series is captioned by {@link identityCaptions}. A `text` column, when the
 * result has one, spells each point's value as the host qualified it.
 */
export function recordedSeries(result: QueryResult, x: string, y: string, identity: readonly string[]): Series[] {
  const [xi, yi, ti] = [columnIndex(result, x), columnIndex(result, y), columnIndex(result, 'text')];
  const lines = new Map<string, { cells: unknown[]; points: { x: number; y: number; text?: string }[] }>();
  for (const row of result.rows) {
    const at = parseInstant(row[xi]);
    if (row[yi] === null || Number.isNaN(at)) continue;
    const key = identityKey(result, row, identity);
    const line = lines.get(key) ?? { cells: identity.map((name) => row[columnIndex(result, name)] ?? null), points: [] };
    line.points.push({ x: at, y: numeric(row[yi]), ...(ti >= 0 && typeof row[ti] === 'string' ? { text: row[ti] } : {}) });
    lines.set(key, line);
  }
  const entries = [...lines];
  const labels = identityCaptions(entries.map(([, { cells }]) => cells));
  return entries.map(([key, { points }], index) => ({ key, label: labels[index], points: points.sort((a, b) => a.x - b.x) }));
}

/**
 * Captions for groups by their typed identity cells: each names only the
 * cells that differ between groups (`∅` for null); a single group is
 * captioned by all its non-null cells. Two captions that read alike fall
 * back to the cells' JSON spelling.
 */
export function identityCaptions(groups: readonly (readonly unknown[])[]): string[] {
  const varies = (groups[0] ?? []).map((_, index) => new Set(groups.map((cells) => JSON.stringify(cells[index] ?? null))).size > 1);
  const text = (cell: unknown) => (cell === null ? '∅' : String(cell));
  const labels = groups.map((cells) => (groups.length > 1 ? cells.filter((_, index) => varies[index]) : cells.filter((cell) => cell !== null)).map(text).join(' · '));
  return labels.map((label, index) => (labels.indexOf(label) === labels.lastIndexOf(label) ? label : JSON.stringify(groups[index])));
}

/**
 * Captions for plotted groups. Each reads as its text unless another group
 * reads the same; then it becomes its group's JSON spelling (value, plus `y`
 * when several are bound), which tells null from the text "null".
 */
function captions(groups: readonly { group: unknown; y: string }[], bySeries: boolean, byY: boolean): string[] {
  const labels = groups.map(({ group, y }) => (!bySeries ? y : byY ? `${String(group)} ${y}` : String(group)));
  return labels.map((label, index) => {
    if (labels.indexOf(label) === labels.lastIndexOf(label)) return label;
    const { group, y } = groups[index];
    return byY ? JSON.stringify([group, y]).slice(1, -1) : JSON.stringify(group);
  });
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
