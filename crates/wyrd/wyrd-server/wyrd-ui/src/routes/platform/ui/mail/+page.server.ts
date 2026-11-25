import { consoleChange } from '$lib/server/auth/platform-sessions';
import { field } from '$lib/server/form';
import { reject } from '$lib/server/auth/session';
import type { Actions, PageServerLoad } from './$types';

/** The relay as `GET /platform/mail` reads it, never its password. */
type Relay = { host: string; port: number; tls: 'starttls' | 'tls' | 'none'; username: string | null; password_set: boolean; sender: string };

export const load: PageServerLoad = async ({ locals }) => {
  const { relay } = await locals.platformSession!.json<{ relay: Relay | null }>('GET', '/mail');
  return { relay };
};

export const actions: Actions = {
  /**
   * Replace the relay; Rust demands fresh proof. A blank password keeps the
   * stored one; removal omits the username, which drops the password sealed
   * to it.
   */
  save: ({ request, locals }) =>
    consoleChange(async () => {
      const form = await request.formData();
      const port = Number(field(form, 'port', 5));
      const tls = field(form, 'tls', 8);
      if (!Number.isInteger(port) || (tls !== 'starttls' && tls !== 'tls' && tls !== 'none')) reject('validation');
      // Username and password are optional: blank is no username, or keep the stored password.
      const optional = (name: string, max: number) => (form.get(name) ? field(form, name, max) : '');
      const remove = form.get('remove') === 'on';
      const username = remove ? '' : optional('username', 256).trim();
      const password = remove ? '' : optional('password', 1024);
      await locals.platformSession!.change('PUT', '/mail', {
        host: field(form, 'host', 255).trim(),
        port,
        tls,
        sender: field(form, 'sender', 320).trim(),
        ...(username ? { username } : {}),
        ...(username && password ? { password } : {})
      });
      return { notice: 'Mail relay saved.' };
    }),
  /** Ask the relay to accept one test message; acceptance is not delivery. */
  test: ({ request, locals }) =>
    consoleChange(async () => {
      const form = await request.formData();
      const result = await locals.platformSession!.json<{ accepted: boolean; error: string | null }>('POST', '/mail/test', { recipient: field(form, 'recipient', 320).trim() });
      return { result };
    })
};
