import { isHttpError } from '@sveltejs/kit';
import type { CardTypes } from 'wyrd-ai';
import { problem, safeProblem } from '$lib/server/problem';
import { read } from '$lib/server/admin';
import { account, accessActions } from '../access.server';
import type { PageServerLoad } from './$types';

/**
 * Service accounts as the server answers them to this session: one page with
 * status counts across every page, the Roles a grant may carry and, for
 * `?person=`, that account's access and keys.
 */
export const load: PageServerLoad = async ({ locals, url }) => {
  const session = locals.browserSession;
  if (!session) return { services: null, problem: problem('upstream') };
  const after = url.searchParams.get('after');
  const person = url.searchParams.get('person');
  try {
    const [page, roles, selected] = await Promise.all([
      read<CardTypes.PrincipalPage>(session, `/principals?kind=service&limit=100${after ? `&after=${encodeURIComponent(after)}` : ''}`),
      read<{ roles: CardTypes.RoleView[] }>(session, '/access/roles'),
      person ? account(session, person) : null
    ]);
    return { services: { ...page, roles: roles.roles, selected }, problem: null };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { services: null, problem: safeProblem(cause) };
  }
};

export const actions = accessActions;
