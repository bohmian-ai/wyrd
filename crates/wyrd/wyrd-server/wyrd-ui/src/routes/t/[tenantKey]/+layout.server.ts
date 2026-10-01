import { reject } from '$lib/server/auth/session';
import { sessionMetadata } from '$lib/server/auth/server-sessions';
import type { LayoutServerLoad } from './$types';

export const load: LayoutServerLoad = async ({ locals, cookies }) => {
  if (!locals.tenant) reject('unauthenticated');
  const { key, name } = locals.tenant.tenant;
  return { session: await sessionMetadata(locals, cookies), tenant: { key, name } };
};
