import { reject } from '$lib/server/auth/session';
import { safeProblem } from '$lib/server/problem';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = ({ locals, url }) => {
  if (!locals.wyrd) reject('unauthenticated');
  const filters = Object.fromEntries(
    ['q', 'kind', 'space', 'status', 'label', 'owner'].map((key) => [
      key,
      url.searchParams.get(key) ?? ''
    ])
  );
  try {
    return { view: locals.wyrd.cards(filters), filters, problem: null };
  } catch (cause) {
    return { view: null, filters, problem: safeProblem(cause) };
  }
};
