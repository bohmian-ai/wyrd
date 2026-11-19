import { json } from '@sveltejs/kit';
import { RangeSchema, inputProblem, resolveRange, validatePanel, type FacetRailInput, type Problem } from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import { observeSupport, untilDeparted } from '$lib/server/support';
import { appliedRange, listInputs, population, readState } from '$lib/observe-host';
import type { RequestHandler } from './$types';

/**
 * Observe's supporting reads for the population the search params name, as
 * this tenant's browser session: the board reads them here after its list on
 * a client navigation or refresh. Only the population's params are read, so
 * list choices never change the answer; the browser cancels the reads by
 * abandoning the request. A malformed range or refused input reads nothing.
 */
export const GET: RequestHandler = async (event) => {
  const session = event.locals.browserSession ?? reject('unauthenticated');
  const state = population(readState(event.url.searchParams));
  const applied = RangeSchema.safeParse(appliedRange(state));
  if (!applied.success) return refuse(inputProblem('Observe', applied.error.issues[0]));
  const rail = validatePanel('FacetRail', listInputs(state).facets);
  if ('code' in rail) return refuse(rail);
  const range = resolveRange(applied.data);
  return json(await untilDeparted(event, (signal) => observeSupport(state, rail.input as FacetRailInput, { executor: nodeExecutor(session.client()), range, signal })));
};

/** Answer a refused population with its stable problem. */
function refuse(problem: Problem): Response {
  return json(problem, { status: problem.status ?? 400, headers: { 'content-type': 'application/problem+json' } });
}
