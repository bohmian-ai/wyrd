import { reject } from '$lib/server/auth/session';
import { saveDraft, resolveDraft, reviewChange } from '$lib/server/changes/actions';
import type { PageServerLoad, Actions } from './$types';
export const load: PageServerLoad = ({ locals, params, url }) => {
  if (!locals.wyrd) reject('unauthenticated');
  return {
    ...locals.wyrd.change(params.id, url.searchParams.get('revision') ?? undefined),
    editing: url.searchParams.get('edit') === 'draft'
  };
};
export const actions: Actions = {
  save: saveDraft,
  resolve: resolveDraft,
  review: reviewChange
};
