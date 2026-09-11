/**
 * The one URL-backed time/correlation filter contract for Observe.
 *
 * Every Observe page reads its scope from the URL and writes it back into
 * every cross-signal link, so the canonical signal routes stay unfiltered
 * homes and Service/Card/Run/status/time stay removable filters, never route
 * hierarchies. Other features (Query, Cards) may consume this module without
 * editing the Observe feature.
 */

/** Time ranges the Observe scope accepts, shortest first. */
export const RANGES = ['15m', '1h', '6h', '24h', '7d', '30d'] as const;

/** Milliseconds each accepted range spans — shared by projections and charts. */
export const RANGE_MS: Record<string, number> = {
  '15m': 15 * 60_000,
  '1h': 3_600_000,
  '6h': 6 * 3_600_000,
  '24h': 24 * 3_600_000,
  '7d': 7 * 24 * 3_600_000,
  '30d': 30 * 24 * 3_600_000
};

/** One removable filter rendered as a chip: remove keeps every other filter. */
export type ScopeChip = { label: string; value: string; removeHref: string };

/** The shared correlation scope: `service` and `range` travel across signals. */
export type Scope = { service: string; range: string };

/** Signal-local filter keys each Observe page may add on top of the shared scope. */
export const SIGNAL_KEYS = [
  'level',
  'status',
  'metric',
  'q',
  'trace',
  'span',
  'record',
  'task',
  'feature',
  'driftCard',
  'evalCard',
  'origin',
  'folder',
  'tag',
  'selected'
] as const;

/**
 * Read the shared scope from URL search params, defaulting the range.
 *
 * An unknown range collapses to the default rather than fabricating a window
 * the server never computed.
 */
export function readScope(params: URLSearchParams, defaultRange = '1h'): Scope {
  const range = params.get('range') ?? '';
  return {
    service: params.get('service') ?? '',
    range: (RANGES as readonly string[]).includes(range) ? range : defaultRange
  };
}

/**
 * Serialize a scope plus signal-local params into a query string.
 *
 * Empty values are omitted so removing a filter is a plain link, and the
 * default range is still written out — restored URLs are explicit, not
 * dependent on per-page defaults.
 */
export function scopeQuery(scope: Scope, extra: Record<string, string> = {}): string {
  const params = new URLSearchParams();
  if (scope.service) params.set('service', scope.service);
  if (scope.range) params.set('range', scope.range);
  for (const [key, value] of Object.entries(extra)) if (value) params.set(key, value);
  const text = params.toString();
  return text ? `?${text}` : '';
}

/** Build a same-feature link that carries the scope into the target signal. */
export function withScope(path: string, scope: Scope, extra: Record<string, string> = {}): string {
  return `${path}${scopeQuery(scope, extra)}`;
}

/**
 * Project the current filters as removable chips.
 *
 * Each chip's remove link re-renders the same path with only that filter
 * dropped; the URL stays the single source of restoration.
 */
export function filterChips(
  path: string,
  filters: Record<string, string>,
  labels: Record<string, string> = {}
): ScopeChip[] {
  const active = Object.entries(filters).filter(([, value]) => value);
  return active.map(([key, value]) => {
    const rest = Object.fromEntries(active.filter(([other]) => other !== key));
    const params = new URLSearchParams(rest);
    const text = params.toString();
    return {
      label: labels[key] ?? key,
      value,
      removeHref: `${path}${text ? `?${text}` : ''}`
    };
  });
}
