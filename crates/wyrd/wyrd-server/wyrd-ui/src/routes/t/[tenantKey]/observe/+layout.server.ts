import {
  loadFacetRail,
  loadLogTimeline,
  loadMetric,
  loadMetricNames,
  loadPanel,
  loadTraceOverview,
  RangeSchema,
  inputProblem,
  resolveRange,
  validatePanel,
  type FacetRailInput,
  type LoadContext,
  type RecordTableProps
} from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import { appliedRange, closed, listInputs, listParams, readState } from '$lib/observe-host';
import type { LayoutServerLoad } from './$types';

/**
 * The main Observe view as this tenant's browser session: the signal's
 * overview, facets and list, and on Metrics the metric rail and the selected
 * metric's series. The range resolves once, so every read of one refresh
 * shares its bounds and predicate; Live refreshes by invalidating
 * `wyrd:observe`. The open record's params are never read here, so opening
 * one never reloads the list. The facet and list inputs pass their catalog
 * schemas first, which checks every WHERE column against the signal; a
 * malformed range or refused input issues no read.
 */
export const load: LayoutServerLoad = async ({ locals, url, request, depends }) => {
  depends('wyrd:observe');
  const session = locals.browserSession ?? reject('unauthenticated');
  const state = closed(readState(listParams(url.searchParams)));
  const applied = RangeSchema.safeParse(appliedRange(state));
  if (!applied.success) return { range: undefined, observe: { problem: inputProblem('Observe', applied.error.issues[0]) } };
  const range = resolveRange(applied.data);
  const context: LoadContext = { executor: nodeExecutor(session.client()), range, signal: request.signal };
  const inputs = listInputs(state);
  const rail = validatePanel('FacetRail', inputs.facets);
  if ('code' in rail) return { range, observe: { problem: rail } };
  const listed = validatePanel('RecordTable', inputs.list);
  if ('code' in listed) return { range, observe: { problem: listed } };
  const checked = rail.input as FacetRailInput;
  const filter = { where: checked.where, where_sql: checked.where_sql };
  const [overview, timeline, facets, list, metrics, series] = await Promise.all([
    state.signal === 'traces' ? loadTraceOverview(filter, context) : undefined,
    state.signal === 'logs' ? loadLogTimeline(filter, context) : undefined,
    loadFacetRail(checked, context),
    loadPanel('RecordTable', listed.input, context) as Promise<RecordTableProps>,
    state.signal === 'metrics' ? loadMetricNames(filter, context) : undefined,
    inputs.series ? loadMetric(inputs.series, context) : undefined
  ]);
  return { range, observe: { overview, timeline, facets, list, metrics, series } };
};
