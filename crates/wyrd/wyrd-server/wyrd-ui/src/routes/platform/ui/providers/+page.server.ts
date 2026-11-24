import { consoleChange } from '$lib/server/auth/platform-sessions';
import { field } from '$lib/server/form';
import { reject } from '$lib/server/auth/session';
import type { Actions, PageServerLoad } from './$types';

/** A shared provider as `GET /platform/social/providers` reads it, never with its secret. */
export type ProviderView = {
  provider: 'google' | 'github';
  client_id: string | null;
  revision: number | null;
  state: 'untested' | 'tested' | 'enabled' | null;
  callback_url: string;
  registration_instructions: string;
};

const names = { google: 'Google', github: 'GitHub' } as const;

/** The provider a form names, refusing any other. */
function provider(form: FormData): keyof typeof names {
  const value = field(form, 'provider', 16);
  if (value !== 'google' && value !== 'github') reject('validation');
  return value;
}

export const load: PageServerLoad = async ({ locals }) => {
  const { providers } = await locals.platformSession!.json<{ providers: ProviderView[] }>('GET', '/social/providers');
  return { providers };
};

export const actions: Actions = {
  /** Store a client as the provider's new untested revision; Rust seals the secret and demands fresh proof. */
  save: ({ request, locals }) =>
    consoleChange(async () => {
      const form = await request.formData();
      const [name, clientId, secret] = [provider(form), field(form, 'client_id', 512).trim(), field(form, 'client_secret', 2048)];
      const view = await locals.platformSession!.json<ProviderView>('PUT', `/social/providers/${name}`, { client_id: clientId, client_secret: secret });
      return { notice: `${names[name]} revision ${view.revision} saved. It is untested and not offered.` };
    }),
  /** Begin a real provider sign-in that tests exactly the revision the page showed. */
  test: ({ request, locals }) =>
    consoleChange(async () => {
      const form = await request.formData();
      const [name, revision] = [provider(form), Number(field(form, 'revision', 20))];
      const { authorization_url } = await locals.platformSession!.json<{ authorization_url: string }>('POST', `/social/providers/${name}/test`, { expected_revision: revision });
      return { test: { provider: name, revision, authorization_url } };
    }),
  /** Offer or withdraw the tested revision the page showed; Rust demands fresh proof. */
  enabled: ({ request, locals }) =>
    consoleChange(async () => {
      const form = await request.formData();
      const [name, revision, enabled] = [provider(form), Number(field(form, 'revision', 20)), field(form, 'enabled', 8) === 'true'];
      await locals.platformSession!.change('PUT', `/social/providers/${name}/enabled`, { enabled, expected_revision: revision });
      return { notice: `${names[name]} is ${enabled ? 'offered on sign-in and signup pages' : 'no longer offered'}.` };
    })
};
