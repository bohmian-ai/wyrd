import type { PageServerLoad } from './$types';

/** One workspace as `GET /platform/tenants` lists it. */
type Tenant = { id: string; slug: string; display_name: string; status: string };

export const load: PageServerLoad = async ({ locals }) => {
  const { tenants } = await locals.platformSession!.json<{ tenants: Tenant[] }>('GET', '/tenants');
  return { tenants };
};
