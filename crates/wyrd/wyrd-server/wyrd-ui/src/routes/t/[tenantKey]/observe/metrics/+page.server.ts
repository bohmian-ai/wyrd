import { reject } from '$lib/server/auth/session';
import { safeProblem } from '$lib/server/problem';
import { readScope } from '$lib/features/observe/core/filter-state';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = ({ locals, url }) => {
  if (!locals.wyrd) reject('unauthenticated');
  const scope = readScope(url.searchParams, '6h');
  const filters = { ...scope, metric: url.searchParams.get('metric') ?? '' };
  try {
    return { view: locals.wyrd.observeMetrics(filters), scope, filters, problem: null };
  } catch (cause) {
    return { view: null, scope, filters, problem: safeProblem(cause) };
  }
};
