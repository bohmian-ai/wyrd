import { fail, isHttpError, redirect } from '@sveltejs/kit';
import type { CredentialList, CurrentUserProfile } from 'wyrd-ai';
import { reject } from '$lib/server/auth/session';
import { browserSessions, type BrowserSession } from '$lib/server/auth/browser-sessions';
import { field } from '$lib/server/form';
import { problem, retryAfter, safeProblem, upstreamProblem } from '$lib/server/problem';
import type { Actions, PageServerLoad, RequestEvent } from './$types';

/** The longest User key lifetime the server accepts, in days. */
const MAX_DAYS = 365;

/** Call `/v1` as the signed-in principal; a non-2xx answer becomes its safe problem. */
async function call(session: BrowserSession, method: string, path: string, body?: unknown): Promise<Response> {
  const response = await session.api(method, path, body);
  if (!response.ok) reject(await upstreamProblem(response));
  return response;
}

/** The signed-in Rust session; the development identity has none. */
function signedIn(event: { locals: App.Locals }): BrowserSession {
  if (!event.locals.browserSession) reject('unauthenticated');
  return event.locals.browserSession;
}

const keysPath = (principalId: string, cursor: string | null) =>
  `/principals/${encodeURIComponent(principalId)}/credentials?limit=50${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ''}`;

/**
 * The calling User's profile and one page of their own key metadata, both
 * derived by the server from the session; the target principal is never
 * taken from the page. No key secret is ever part of this page load.
 */
export const load: PageServerLoad = async (event) => {
  if (!event.locals.browserSession) return { profile: null, keys: null, problem: problem('upstream') };
  try {
    const session = signedIn(event);
    const profile = (await (await call(session, 'GET', '/principals/me')).json()) as CurrentUserProfile;
    const cursor = event.url.searchParams.get('after');
    const keys = (await (await call(session, 'GET', keysPath(profile.principal_id, cursor))).json()) as CredentialList;
    return { profile, keys, problem: null };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { profile: null, keys: null, problem: safeProblem(cause) };
  }
};

/** Run one action and render its refusal as a safe problem. */
async function act<T>(event: RequestEvent, run: (form: FormData, session: BrowserSession) => Promise<T>) {
  try {
    return await run(await event.request.formData(), signedIn(event));
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    const value = safeProblem(cause);
    const wait = retryAfter(value);
    if (wait) event.setHeaders({ 'retry-after': String(wait) });
    return fail(value.status, { problem: value });
  }
}

export const actions: Actions = {
  /**
   * Create one of the User's own keys. The secret is returned in this one
   * no-store action result only; reloading or leaving the page discards it.
   */
  create: (event) =>
    act(event, async (form, session) => {
      const days = Number(field(form, 'days', 4));
      if (!Number.isSafeInteger(days) || days < 1 || days > MAX_DAYS) reject('validation');
      const ceiling = form.getAll('permission').filter((value): value is string => typeof value === 'string');
      if (!ceiling.length) reject('validation');
      const response = await call(session, 'POST', `/principals/${encodeURIComponent(session.principalId)}/credentials`, {
        name: field(form, 'name', 256).trim(),
        permission_ceiling: ceiling,
        expires_in: days * 86_400
      });
      const issued = (await response.json()) as { id: string; credential: string };
      return { issued: { id: issued.id, credential: issued.credential } };
    }),
  revoke: (event) =>
    act(event, async (form, session) => {
      const id = field(form, 'id', 64);
      await call(session, 'DELETE', `/principals/${encodeURIComponent(session.principalId)}/credentials/${encodeURIComponent(id)}`);
      return { revoked: id };
    }),
  /** Change the local password; the server ends every local session, so sign in again. */
  password: async (event) => {
    const changed = await act(event, async (form, session) => {
      await browserSessions.changePassword(
        session.tenantKey,
        field(form, 'currentPassword'),
        field(form, 'newPassword'),
        event.url,
        event.cookies
      );
      return { changed: true };
    });
    if ('changed' in changed) redirect(303, `/t/${encodeURIComponent(event.params.tenantKey)}/login`);
    return changed;
  }
};
