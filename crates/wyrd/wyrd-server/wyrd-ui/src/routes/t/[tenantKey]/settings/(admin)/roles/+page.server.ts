import { isHttpError } from '@sveltejs/kit';
import type { CardTypes } from 'wyrd-ai';
import { problem, safeProblem } from '$lib/server/problem';
import { read } from '$lib/server/admin';
import { accessActions } from '../access.server';
import type { PageServerLoad } from './$types';

/** The tenant's Roles, each with its server description and member count. */
export const load: PageServerLoad = async ({ locals }) => {
  const session = locals.browserSession;
  if (!session) return { roles: null, problem: problem('upstream') };
  try {
    return { roles: (await read<{ roles: CardTypes.RoleView[] }>(session, '/access/roles')).roles, problem: null };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { roles: null, problem: safeProblem(cause) };
  }
};

export const actions = accessActions;
