import { isHttpError } from '@sveltejs/kit';
import type { CardTypes } from 'wyrd-ai';
import { problem, safeProblem } from '$lib/server/problem';
import { read } from '$lib/server/admin';
import { account, accessActions, type Invitation } from '../access.server';
import type { PageServerLoad } from './$types';

/**
 * People as the server answers them to this session: one page of Users for
 * the URL's exact filters with their status counts across every page, the
 * Roles an invitation may carry, the pending invitations and, for `?person=`,
 * that account's access and keys. A refusal is the page's whole content.
 */
export const load: PageServerLoad = async ({ locals, url }) => {
  const session = locals.browserSession;
  if (!session) return { people: null, problem: problem('upstream') };
  const q = url.searchParams.get('q')?.trim() ?? '';
  const role = url.searchParams.get('role');
  const after = url.searchParams.get('after');
  const person = url.searchParams.get('person');
  const query = new URLSearchParams({ kind: 'user', limit: '100' });
  if (q) query.set(q.includes('@') ? 'email' : 'name', q);
  if (role) query.set('role', role);
  if (after) query.set('after', after);
  try {
    const [page, roles, invitations, selected] = await Promise.all([
      read<CardTypes.PrincipalPage>(session, `/principals?${query}`),
      read<{ roles: CardTypes.RoleView[] }>(session, '/access/roles'),
      read<{ invitations: Invitation[] }>(session, '/principals/users/invitations'),
      person ? account(session, person) : null
    ]);
    return { people: { ...page, roles: roles.roles, invitations: invitations.invitations, selected }, problem: null };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { people: null, problem: safeProblem(cause) };
  }
};

export const actions = accessActions;
