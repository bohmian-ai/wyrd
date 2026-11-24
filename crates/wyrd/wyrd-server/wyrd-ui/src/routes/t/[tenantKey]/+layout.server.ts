import type { CurrentUserProfile } from 'wyrd-ai';
import { reject, sessions } from '$lib/server/auth/session';
import { browserSessions } from '$lib/server/auth/browser-sessions';
import type { LayoutServerLoad } from './$types';

/**
 * The name block's person: display name, else username, else email, with the
 * email as the second line. Read from the caller's own `/me`; a principal
 * without a User profile keeps the session's name, never its id.
 */
async function identity(api: (method: string, path: string) => Promise<Response>) {
  const response = await api('GET', '/principals/me');
  if (!response.ok) return null;
  const me = (await response.json()) as CurrentUserProfile;
  const name = me.display_name ?? me.username ?? me.email;
  return name ? { name, email: me.email } : null;
}

export const load: LayoutServerLoad = async ({ locals, cookies, url, untrack }) => {
  if (!locals.tenant) reject('unauthenticated');
  const { key, name } = locals.tenant.tenant;
  const browser = locals.browserSession;
  const [session, me] = locals.session
    ? [sessions.metadata(locals.session), null]
    : browser
      ? await Promise.all([
          // Only the origin is read, so navigating within the tenant reuses this enumeration.
          browserSessions.metadata(browser, untrack(() => new URL(url.origin)), cookies),
          identity((method, path) => browser.api(method, path))
        ])
      : reject('unauthenticated');
  return { session, me, tenant: { key, name } };
};
