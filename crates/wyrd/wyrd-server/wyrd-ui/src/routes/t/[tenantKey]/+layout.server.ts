import { reject } from '$lib/server/auth/session';
import { sessionMetadata } from '$lib/server/auth/server-sessions';
import type { LayoutServerLoad } from './$types';

export const load: LayoutServerLoad = ({ locals, cookies }) => {
  if (!locals.tenant) reject('unauthenticated');
  const { key, name } = locals.tenant.tenant;
  return { session: sessionMetadata(locals, cookies), tenant: { key, name } };
};
