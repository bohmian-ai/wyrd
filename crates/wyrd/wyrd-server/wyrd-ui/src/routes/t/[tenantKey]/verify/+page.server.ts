import {
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
import { verifySupport } from '$lib/server/support';
import { appliedRange, readState } from '$lib/observe-host';
import { readShown, verifyInputs } from '$lib/verify-host';
import type { PageServerLoad } from './$types';

/**
 * The Verify dashboard as this tenant's browser session: facet counts, one
 * page of executions and the two charts, every one reading the same derived
 * population under the same resolved range and WHERE clauses. The inputs pass
 * their catalog schemas first; a malformed range or refused input issues no
 * read. No execution's status or details is read per row. One bounded Card
 * list names the Verifiers; when it is refused their exact UIDs are shown.
 * Live refreshes by invalidating `wyrd:verify`. The first HTML carries every
 * section; a client navigation or refresh reads only the list, and the page
 * reads the facets, charts and names from `verify/support` when their
 * population changes or Live refreshes.
 */
export const load: PageServerLoad = async ({ locals, url, request, depends, isDataRequest }) => {
  depends('wyrd:verify');
  const session = locals.browserSession ?? reject('unauthenticated');
  const state = readState(url.searchParams);
  const applied = RangeSchema.safeParse(appliedRange(state));
  if (!applied.success) return { state, shown: readShown(url.searchParams), range: undefined, verify: { problem: inputProblem('Verify', applied.error.issues[0]) } };
  const range = resolveRange(applied.data);
  const context: LoadContext = { executor: nodeExecutor(session.client()), range, signal: request.signal };
  const shown = readShown(url.searchParams);
  const inputs = verifyInputs(state, shown);
  const rail = validatePanel('FacetRail', inputs.facets);
  if ('code' in rail) return { state, shown, range, verify: { problem: rail } };
  const listed = validatePanel('RecordTable', inputs.list);
  if ('code' in listed) return { state, shown, range, verify: { problem: listed } };
  // The first HTML reads support beside the list; a client navigation reads it from `verify/support`.
  const support = isDataRequest ? undefined : verifySupport(rail.input as FacetRailInput, context);
  const list = (await loadPanel('RecordTable', listed.input, context)) as RecordTableProps;
  return { state, shown, range, verify: { list, support: await support } };
};
