import { reject } from '$lib/server/auth/session';
import { safeProblem } from '$lib/server/problem';
import { readScope } from '$lib/features/observe/core/filter-state';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = ({ locals, url, params }) => {
  if (!locals.wyrd) reject('unauthenticated');
  const scope = readScope(url.searchParams, '24h');
  const variables = Object.fromEntries(
    ['service', 'region'].map((key) => [key, url.searchParams.get(key) ?? ''])
  );
  try {
    return {
      view: locals.wyrd.observeDashboard(params.id, variables),
      scope,
      variables,
      problem: null
    };
  } catch (cause) {
    return { view: null, scope, variables, problem: safeProblem(cause) };
  }
};
