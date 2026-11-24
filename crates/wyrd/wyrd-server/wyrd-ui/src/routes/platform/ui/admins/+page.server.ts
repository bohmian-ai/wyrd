import type { PageServerLoad } from './$types';

/** One platform principal as `GET /platform/admins` lists it. */
type Admin = { principal_id: string; principal_kind: 'global_admin' | 'user'; name: string; status: string; match_claim: string | null; subject: string | null };

export const load: PageServerLoad = async ({ locals }) => {
  const { principals } = await locals.platformSession!.json<{ principals: Admin[] }>('GET', '/admins');
  return { admins: principals.filter((admin) => admin.status !== 'deleted') };
};
