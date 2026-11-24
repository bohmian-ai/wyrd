import { consoleChange } from '$lib/server/auth/platform-sessions';
import { field } from '$lib/server/form';
import type { ProviderView } from './providers/+page.server';
import type { Actions, PageServerLoad } from './$types';

/** Rust's deployment login settings; the challenge secret is write-only and never read. */
type LoginSettings = {
  password_enabled: boolean;
  signup_enabled: boolean;
  signup_workspace_cap: number;
  signup_challenge: { site_key: string } | null;
};

/**
 * The deployment at a glance, each figure from the platform endpoint that owns
 * it: workspaces, the shared sign-in providers and the mail relay, plus the
 * login and signup settings this page edits.
 */
export const load: PageServerLoad = async ({ locals }) => {
  const session = locals.platformSession!;
  const [{ tenants }, { providers }, { relay }, settings] = await Promise.all([
    session.json<{ tenants: { status: string }[] }>('GET', '/tenants'),
    session.json<{ providers: ProviderView[] }>('GET', '/social/providers'),
    session.json<{ relay: unknown }>('GET', '/mail'),
    session.json<LoginSettings>('GET', '/login-settings')
  ]);
  return {
    workspaces: tenants.filter((tenant) => tenant.status === 'active').length,
    providers: { enabled: providers.filter((p) => p.state === 'enabled').length, total: providers.length },
    relay: relay !== null,
    settings
  };
};

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
