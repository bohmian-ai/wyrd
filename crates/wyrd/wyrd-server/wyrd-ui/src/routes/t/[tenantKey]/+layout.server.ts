import { sessions, reject } from '$lib/server/auth/session';
import type { LayoutServerLoad } from './$types';

export const load: LayoutServerLoad = ({ locals }) => {
  if (!locals.session || !locals.tenant) reject('unauthenticated');
  const { key, name } = locals.tenant.tenant;
  return { session: sessions.metadata(locals.session), tenant: { key, name } };
};
