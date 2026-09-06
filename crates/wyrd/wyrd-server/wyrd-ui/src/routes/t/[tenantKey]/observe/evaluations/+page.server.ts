import { reject } from '$lib/server/auth/session';
import { safeProblem } from '$lib/server/problem';
import { readScope } from '$lib/features/observe/core/filter-state';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = ({ locals, url }) => {
  if (!locals.wyrd) reject('unauthenticated');
  const scope = readScope(url.searchParams, '24h');
  const filters = {
    ...scope,
    status: url.searchParams.get('status') ?? '',
    origin: url.searchParams.get('origin') ?? '',
    evalCard: url.searchParams.get('evalCard') ?? '',
    q: url.searchParams.get('q') ?? ''
  };
  try {
    return { view: locals.wyrd.observeEvals(filters), scope, filters, problem: null };
  } catch (cause) {
    return { view: null, scope, filters, problem: safeProblem(cause) };
  }
};
