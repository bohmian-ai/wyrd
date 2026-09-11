/**
 * URL state for the Service Card operational workspace (TASK-007).
 *
 * The workspace's entire local state lives in query parameters —
 * `view`, `version`, `range`, `sel` and the mock-only `state` — so any page
 * can be restored from a pasted URL (REQ-123/REQ-125). Parsing collapses
 * unknown values to truthful defaults instead of fabricating a scope the
 * server never projected.
 */
import { RANGES, RANGE_MS } from '$lib/features/observe/core/filter-state';

/** The exact Service-local primary subnavigation, Overview first (REQ-123). */
export const SERVICE_VIEWS = ['overview', 'composition', 'definition'] as const;
/** One of the three Service-local views. */
export type ServiceView = (typeof SERVICE_VIEWS)[number];

/**
 * Mock-only Overview scenario selectors the fixture projection carries.
 * These pick which server-authored state variant renders (S-01/S-04/S-05/
 * S-06); the browser never computes a health state itself. Absent means the
 * projection's default state.
 */
export const MOCK_STATES = ['healthy', 'stale', 'partial', 'failed'] as const;

/** The parsed workspace scope; empty strings mean "not set". */
export type ServiceScope = { view: ServiceView; range: string; state: string; sel: string };

/**
 * Read the workspace scope from URL search params.
 *
 * Unknown views fall back to Overview (the default and dominant view),
 * unknown ranges to `1h`, and unknown mock states to the projection default.
 */
export function readServiceScope(params: URLSearchParams): ServiceScope {
  const view = params.get('view') ?? '';
  const range = params.get('range') ?? '';
  const state = params.get('state') ?? '';
  return {
    view: (SERVICE_VIEWS as readonly string[]).includes(view) ? (view as ServiceView) : 'overview',
    range: (RANGES as readonly string[]).includes(range) ? range : '1h',
    state: (MOCK_STATES as readonly string[]).includes(state) ? state : '',
    sel: params.get('sel') ?? ''
  };
}

/**
 * Build a same-Card link that patches part of the workspace scope.
 *
 * The link always states `view` and `version` explicitly; `range` renders
 * only on the operational Overview (composition and definition are not
 * time-scoped); `sel` renders only on Composition; a set mock `state`
 * survives so a scenario stays inspectable — patch it to `''` to clear it
 * (the S-06 Retry affordance).
 */
export function serviceHref(
  path: string,
  version: string,
  scope: ServiceScope,
  patch: Partial<ServiceScope>
): string {
  const next = { ...scope, ...patch };
  const params = new URLSearchParams({ view: next.view, version });
  if (next.view === 'overview' && next.range) params.set('range', next.range);
  if (next.state) params.set('state', next.state);
  if (next.view === 'composition' && next.sel) params.set('sel', next.sel);
  return `${path}?${params}`;
}

/** Stamp the visible range into a canonical Observe link template. */
export function observeHref(template: string, range: string): string {
  return template.replace('{range}', range);
}

/** One machine-readable axis stamp: UTC clock label plus ISO instant. */
export type WindowStamp = { label: string; at: string };

/** UTC hh:mm for an epoch — matches every other timestamp the UI shows. */
function clock(ms: number): string {
  return new Date(ms).toISOString().slice(11, 16);
}

/** Epoch → stamp pair for the chart frame's `<time>` semantics. */
function stamp(ms: number): WindowStamp {
  return { label: clock(ms), at: new Date(ms).toISOString() };
}

/**
 * Derive the shared chart window for the selected range ending at the
 * projection's observation clock (`nowIso`). `endsAtIso` truthfully shortens
 * the window's end where observations stop (the S-04 gap) — the axis then
 * states the observed window instead of stretching stale data to now.
 */
export function chartWindow(
  range: string,
  nowIso: string,
  endsAtIso?: string
): { from: WindowStamp; mid: WindowStamp; to: WindowStamp } {
  const rangeMs = RANGE_MS[range] ?? RANGE_MS['1h'];
  const now = Date.parse(nowIso);
  const from = now - rangeMs;
  const end = endsAtIso ? Date.parse(endsAtIso) : now - 60_000;
  return { from: stamp(from), mid: stamp(from + rangeMs / 2), to: stamp(end) };
}
