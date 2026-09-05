import { reject } from '$lib/server/auth/session';
import { saveDraft, resolveDraft } from '$lib/server/changes/actions';
import type { PageServerLoad, Actions } from './$types';
export const load: PageServerLoad = ({ locals }) => {
  if (!locals.wyrd) reject('unauthenticated');
  return locals.wyrd.newChange();
};
export const actions: Actions = { save: saveDraft, resolve: resolveDraft };
