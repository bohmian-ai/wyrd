import type { CardTypes } from 'wyrd-ai';
import { read } from '$lib/server/admin';
import type { LayoutServerLoad } from './$types';

/**
 * The section list's counts, as the server answers them to this session: Users
 * and Service accounts from the directory's status counts, and the Roles the
 * server lists. A section this session may not read shows no count; its page
 * explains the refusal.
 */
export const load: LayoutServerLoad = async ({ locals }) => {
  const session = locals.browserSession;
  if (!session) return { counts: { people: null, services: null, roles: null } };
  const total = (page: CardTypes.PrincipalPage) => page.counts.active + page.counts.suspended;
  const quietly = <T>(value: Promise<T>) => value.catch(() => null);
  const [people, services, roles] = await Promise.all([
    quietly(read<CardTypes.PrincipalPage>(session, '/principals?kind=user&limit=1').then(total)),
    quietly(read<CardTypes.PrincipalPage>(session, '/principals?kind=service&limit=1').then(total)),
    quietly(read<{ roles: CardTypes.RoleView[] }>(session, '/access/roles').then(({ roles }) => roles.length))
  ]);
  return { counts: { people, services, roles } };
};
