import { consoleChange } from '$lib/server/auth/platform-sessions';
import { field } from '$lib/server/form';
import type { Actions, PageServerLoad } from './$types';

/** Rust's deployment login settings; the challenge secret is write-only and never read. */
type LoginSettings = {
  password_enabled: boolean;
  signup_enabled: boolean;
  signup_workspace_cap: number;
  signup_challenge: { site_key: string } | null;
};

export const load: PageServerLoad = async ({ locals }) => ({
  settings: await locals.platformSession!.json<LoginSettings>('GET', '/login-settings')
});

export const actions: Actions = {
  /** Replace the login settings; Rust demands fresh proof. A blank challenge secret keeps the stored one. */
  save: ({ request, locals }) =>
    consoleChange(async () => {
      const form = await request.formData();
      const secret = form.get('secret') ? field(form, 'secret', 1024) : undefined;
      await locals.platformSession!.change('PUT', '/login-settings', {
        password_enabled: form.get('password_enabled') === 'on',
        signup_enabled: form.get('signup_enabled') === 'on',
        signup_workspace_cap: Number(field(form, 'signup_workspace_cap', 5)),
        signup_challenge: form.get('challenge') === 'on' ? { site_key: field(form, 'site_key', 512).trim(), ...(secret ? { secret } : {}) } : null
      });
      return { notice: 'Login and signup settings saved.' };
    })
};
