import { reject } from '$lib/server/auth/session';
import { runCheck } from '$lib/server/changes/actions';
import type { PageServerLoad, Actions } from './$types';
export const load: PageServerLoad = ({ locals, params, url }) => {
  if (!locals.wyrd) reject('unauthenticated');
  return locals.wyrd.change(params.id, url.searchParams.get('revision') ?? undefined);
};
export const actions: Actions = { run: runCheck };
