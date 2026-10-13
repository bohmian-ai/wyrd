import { reject } from '$lib/server/auth/session';
import { safeProblem } from '$lib/server/problem';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ locals, url }) => {
  if (!locals.wyrd) reject('unauthenticated');
  const filters = Object.fromEntries(
    ['q', 'kind', 'space', 'status', 'label', 'owner'].map((key) => [
      key,
      url.searchParams.get(key) ?? ''
    ])
  );
  const cursor = url.searchParams.get('cursor') ?? '';
  try {
    return { view: await locals.wyrd.cards(filters, cursor), filters, cursor, problem: null };
  } catch (cause) {
    return { view: null, filters, cursor, problem: safeProblem(cause) };
  }
};
