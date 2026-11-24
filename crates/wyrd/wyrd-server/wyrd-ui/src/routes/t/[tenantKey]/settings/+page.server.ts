import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';

/** Settings opens on its first section. */
export const load: PageServerLoad = ({ params }) => redirect(307, `/t/${encodeURIComponent(params.tenantKey)}/settings/people`);
