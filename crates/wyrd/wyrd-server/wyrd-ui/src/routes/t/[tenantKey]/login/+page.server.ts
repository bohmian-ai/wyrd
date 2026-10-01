import { fail, isHttpError, redirect } from '@sveltejs/kit';
import { localAuthEnabled } from '$lib/server/development';
import { reject } from '$lib/server/auth/session';
import { serverSessions } from '$lib/server/auth/server-sessions';
import { problem, safeProblem } from '$lib/server/problem';
import type { Actions, PageServerLoad } from './$types';

/** Same-origin POST only; the tenant key is routing context, never authority. */
function sameOrigin(request: Request): void {
  if (request.method !== 'POST' || request.headers.get('origin') !== new URL(request.url).origin)
    reject('denied');
}

export const load: PageServerLoad = async ({ params, cookies }) => {
  // The development identity signs in from the root page.
  if (localAuthEnabled()) redirect(303, '/');
  const tenant = { key: params.tenantKey };
  try {
    if (await serverSessions.read(params.tenantKey, cookies))
      redirect(303, `/t/${encodeURIComponent(params.tenantKey)}`);
    return { tenant, ...(await serverSessions.loginOptions(params.tenantKey)), problem: null };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { tenant, sso: false, problem: safeProblem(cause) };
  }
};

export const actions: Actions = {
  sso: async ({ params, request, cookies }) => {
    let target: string;
    try {
      if (localAuthEnabled()) reject('denied');
      sameOrigin(request);
      target = await serverSessions.begin(params.tenantKey, cookies);
    } catch (cause) {
      if (!isHttpError(cause)) throw cause;
      return fail(cause.status, { problem: safeProblem(cause) });
    }
    redirect(303, target);
  },
  apiKey: async ({ params, request, cookies }) => {
    let destination: string;
    try {
      if (localAuthEnabled()) reject('denied');
      sameOrigin(request);
      const apiKey = (await request.formData()).get('apiKey');
      if (typeof apiKey !== 'string' || !apiKey || apiKey.length > 4096) reject('validation');
      // Routine human login uses SSO whenever the tenant has it.
      if ((await serverSessions.loginOptions(params.tenantKey)).sso) reject('denied');
      destination = await serverSessions.exchangeApiKey(params.tenantKey, apiKey, cookies);
    } catch (cause) {
      if (!isHttpError(cause)) throw cause;
      const value = safeProblem(cause);
      // One indistinguishable refusal, status included, for every unusable credential.
      const refusal =
        value.status === 401 || value.status === 403 ? problem('unauthenticated') : value;
      return fail(refusal.status, { problem: refusal });
    }
    redirect(303, destination);
  }
};
