import { reject } from '$lib/server/auth/session';
import { safeProblem } from '$lib/server/problem';
import type { PageServerLoad } from './$types';
export const load: PageServerLoad = ({ locals, url }) => {
  if (!locals.wyrd) reject('unauthenticated');
  const filters = Object.fromEntries(
    ['view', 'q', 'owner', 'team', 'repository', 'lifecycle'].map((key) => [
      key,
      url.searchParams.get(key) ?? ''
    ])
  );
  if (!['open', 'needs-attention', 'verified', 'closed'].includes(filters.view))
    filters.view = 'open';
  try {
    return { list: locals.wyrd.changes(filters), filters, problem: null };
  } catch (cause) {
    return { list: null, filters, problem: safeProblem(cause) };
  }
};
