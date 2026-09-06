import { reject } from '$lib/server/auth/session';
import { safeProblem } from '$lib/server/problem';
import { readScope } from '$lib/features/observe/core/filter-state';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = ({ locals, url }) => {
  if (!locals.wyrd) reject('unauthenticated');
  const scope = readScope(url.searchParams, '30d');
  const filters = {
    ...scope,
    driftCard: url.searchParams.get('driftCard') ?? '',
    feature: url.searchParams.get('feature') ?? ''
  };
  try {
    return { view: locals.wyrd.observeDrift(filters), scope, filters, problem: null };
  } catch (cause) {
    return { view: null, scope, filters, problem: safeProblem(cause) };
  }
};
