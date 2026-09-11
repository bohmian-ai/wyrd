import { reject } from '$lib/server/auth/session';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = ({ locals, params, url }) => {
  if (!locals.wyrd) reject('unauthenticated');
  return { detail: locals.wyrd.card(params.uid, url.searchParams.get('version') ?? undefined) };
};
