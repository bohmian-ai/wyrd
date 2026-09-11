import { reject } from '$lib/server/auth/session';
import { safeProblem } from '$lib/server/problem';
import { readScope } from '$lib/features/observe/core/filter-state';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = ({ locals, url }) => {
  if (!locals.wyrd) reject('unauthenticated');
  const scope = readScope(url.searchParams);
  const filters = {
    ...scope,
    level: url.searchParams.get('level') ?? '',
    trace: url.searchParams.get('trace') ?? '',
    q: url.searchParams.get('q') ?? '',
    record: url.searchParams.get('record') ?? '',
    page: url.searchParams.get('page') ?? '',
    per: url.searchParams.get('per') ?? ''
  };
  try {
    return { view: locals.wyrd.observeLogs(filters), scope, filters, problem: null };
  } catch (cause) {
    return { view: null, scope, filters, problem: safeProblem(cause) };
  }
};
