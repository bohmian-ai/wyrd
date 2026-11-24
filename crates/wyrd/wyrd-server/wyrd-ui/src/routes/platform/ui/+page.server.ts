import { consoleChange } from '$lib/server/auth/platform-sessions';
import { field } from '$lib/server/form';
import type { Actions, PageServerLoad } from './$types';

/** One workspace as `GET /platform/tenants` lists it. */
type Tenant = { id: string; slug: string; display_name: string; status: string };

export const load: PageServerLoad = async ({ locals }) => {
  const { tenants } = await locals.platformSession!.json<{ tenants: Tenant[] }>('GET', '/tenants');
  return { tenants };
};

export const actions: Actions = {
  /** Suspend or resume one workspace; Rust demands fresh proof and keeps every row, grant and credential. */
  status: ({ request, locals }) =>
    consoleChange(async () => {
      const form = await request.formData();
      const [id, slug, status] = [field(form, 'id', 64), field(form, 'slug', 128), field(form, 'status', 16)];
      await locals.platformSession!.change('PUT', `/tenants/${encodeURIComponent(id)}/status`, { status });
      return { notice: `${slug} is ${status}.` };
    })
};
