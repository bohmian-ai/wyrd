import { json } from '@sveltejs/kit';
import { AbsoluteRangeSchema, inputProblem, loadMetricNames, validatePanel, type FacetRailInput, type Problem } from 'wyrd-ai-ui/loaders';
import { nodeExecutor } from 'wyrd-ai-ui/node';
import { reject } from '$lib/server/auth/session';
import { listInputs, readState } from '$lib/observe-host';
import type { RequestHandler } from './$types';

/**
 * The metric rail's names as this tenant's browser session, searched on the
 * server as the reader types. The query string is the Observe view's own,
 * with the page's resolved `from`/`to` and the search `q`, so the rail reads
 * the same range, WHERE and predicate as the view without reloading it. The
 * WHERE passes the facet rail's catalog schema first, as the view's own load
 * does, so only the signal's columns reach the SQL.
 */
export const GET: RequestHandler = async ({ locals, url, request }) => {
  const session = locals.browserSession ?? reject('unauthenticated');
  const range = AbsoluteRangeSchema.safeParse({ from: url.searchParams.get('from'), to: url.searchParams.get('to') });
  if (!range.success) {
    const [issue] = range.error.issues;
    return refuse(inputProblem('Metrics', { path: ['range', ...issue.path], message: issue.message }));
  }
  const search = url.searchParams.get('q') ?? undefined;
  if (search && search.length > 256) return refuse(inputProblem('Metrics', { path: ['q'], message: 'must be at most 256 characters, the longest metric name' }));
  const rail = validatePanel('FacetRail', listInputs(readState(url.searchParams)).facets);
  if ('code' in rail) return refuse(rail);
  const { where, where_sql } = rail.input as FacetRailInput;
  const loaded = await loadMetricNames(
    { where, where_sql, search },
    { executor: nodeExecutor(session.client()), range: range.data, signal: request.signal }
  );
  return loaded.state === 'error' ? refuse(loaded.problem) : json(loaded);
};

/** Answer a refused read with its stable problem. */
function refuse(problem: Problem): Response {
  return json(problem, { status: problem.status ?? 400, headers: { 'content-type': 'application/problem+json' } });
}
