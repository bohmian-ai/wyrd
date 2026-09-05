import { error } from '@sveltejs/kit';
import { reject } from '$lib/server/auth/session';
import { safeProblem } from '$lib/server/problem';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ locals }) => {
  if (!locals.wyrd) reject('unauthenticated');
  try {
    return { home: await locals.wyrd.home(), problem: null };
  } catch (cause) {
    const value = safeProblem(cause);
    if (value.status < 500) error(value.status, { ...value, message: value.title });
    return { home: null, problem: value, serverUnavailable: !locals.mockData };
  }
};
