import { reject } from '$lib/server/auth/session';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ locals, params, url }) => {
  if (!locals.wyrd) reject('unauthenticated');
  return { detail: await locals.wyrd.card(params.uid, url.searchParams.get('version') ?? undefined) };
};
