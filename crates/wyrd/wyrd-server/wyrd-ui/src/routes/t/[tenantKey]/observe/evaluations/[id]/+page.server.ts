import { reject } from '$lib/server/auth/session';
import { safeProblem } from '$lib/server/problem';
import { readScope } from '$lib/features/observe/core/filter-state';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = ({ locals, url, params }) => {
  if (!locals.wyrd) reject('unauthenticated');
  const scope = readScope(url.searchParams, '24h');
  const task = url.searchParams.get('task') ?? '';
  try {
    return { view: locals.wyrd.observeEval(params.id, task), scope, task, problem: null };
  } catch (cause) {
    return { view: null, scope, task, problem: safeProblem(cause) };
  }
};
