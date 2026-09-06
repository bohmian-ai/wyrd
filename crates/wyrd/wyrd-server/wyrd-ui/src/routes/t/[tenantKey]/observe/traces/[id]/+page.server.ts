import { reject } from '$lib/server/auth/session';
import { safeProblem } from '$lib/server/problem';
import { readScope } from '$lib/features/observe/core/filter-state';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = ({ locals, url, params }) => {
  if (!locals.wyrd) reject('unauthenticated');
  const scope = readScope(url.searchParams);
  // The back link restores the whole search that led here, span selection included.
  const back = {
    status: url.searchParams.get('status') ?? '',
    span: url.searchParams.get('span') ?? ''
  };
  try {
    return {
      view: locals.wyrd.observeTrace(params.id, back.span),
      scope,
      back,
      problem: null
    };
  } catch (cause) {
    return { view: null, scope, back, problem: safeProblem(cause) };
  }
};
