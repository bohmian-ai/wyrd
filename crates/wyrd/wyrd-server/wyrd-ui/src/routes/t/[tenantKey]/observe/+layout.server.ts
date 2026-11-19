import {
  loadMetric,
  loadPanel,
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
import { observeSupport } from '$lib/server/support';
import { appliedRange, closed, listInputs, listParams, readState } from '$lib/observe-host';
import type { LayoutServerLoad } from './$types';

/**
 * The main Observe view as this tenant's browser session: the signal's
 * overview, facets and list, and on Metrics the metric rail and the selected
 * metric's series; Metrics reads its recorded points only while the reader
 * has the points inspector open. The range resolves once, so every read of one refresh
 * shares its bounds and predicate; Live refreshes by invalidating
 * `wyrd:observe`. The open record's params are never read here, so opening
 * one never reloads the list. The facet and list inputs pass their catalog
 * schemas first, which checks every WHERE column against the signal; a
 * malformed range or refused input issues no read. The first HTML carries
 * every section; a client navigation or refresh reads only the list and
 * series, and the board reads the overview, timeline, facets and metric names
 * from `observe/support` when their population changes or Live refreshes.
 */
export const load: LayoutServerLoad = async ({ locals, url, request, depends, isDataRequest }) => {
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
  // The first HTML reads support beside the list; a client navigation reads it from `observe/support`.
  const support = isDataRequest ? undefined : observeSupport(state, rail.input as FacetRailInput, context);
  const [list, series] = await Promise.all([
    state.signal !== 'metrics' || state.points ? (loadPanel('RecordTable', listed.input, context) as Promise<RecordTableProps>) : undefined,
    inputs.series ? loadMetric(inputs.series, context) : undefined
  ]);
  return { range, observe: { list, series, support: await support } };
};
