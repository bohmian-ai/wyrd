import { reject } from '$lib/server/auth/session';
import { safeProblem } from '$lib/server/problem';
import { readScope } from '$lib/features/observe/core/filter-state';
import type { PageServerLoad } from './$types';

/**
 * O-09 — GenAI call search. Filters echo the URL contract: model, operation
 * and free text on trace/conversation ids ride alongside the shared scope,
 * and `limit` carries the load-more depth like the traces search.
 */
export const load: PageServerLoad = ({ locals, url }) => {
  if (!locals.wyrd) reject('unauthenticated');
  const scope = readScope(url.searchParams);
  const filters = {
    ...scope,
    model: url.searchParams.get('model') ?? '',
    operation: url.searchParams.get('operation') ?? '',
    q: url.searchParams.get('q') ?? '',
    limit: url.searchParams.get('limit') ?? ''
  };
  try {
    return { view: locals.wyrd.observeGenAi(filters), scope, filters, problem: null };
  } catch (cause) {
    return { view: null, scope, filters, problem: safeProblem(cause) };
  }
};
