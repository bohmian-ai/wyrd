import { fail, isHttpError, redirect } from '@sveltejs/kit';
import { localAuthEnabled } from '$lib/server/development';
import { reject } from '$lib/server/auth/session';
import { browserSessions, type LoginMethods } from '$lib/server/auth/browser-sessions';
import { field } from '$lib/server/form';
import { problem, retryAfter, safeProblem } from '$lib/server/problem';
import type { Actions, PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ params, cookies, url }) => {
  // The development identity signs in from the root page.
  if (localAuthEnabled()) redirect(303, '/');
  const tenant = { key: params.tenantKey };
  const reset = url.searchParams.get('reset') === 'done';
  const login = url.searchParams.get('login');
  // Rust's own refusal reasons for a verified provider identity.
  const notice = login === 'no_account' || login === 'account_exists' ? login : null;
  // Only the methods Rust advertises render; a failed read renders none.
  let methods: Pick<LoginMethods, 'password' | 'company_sso' | 'providers'> = {
    password: false,
    company_sso: false,
    providers: []
  };
  try {
    if (await browserSessions.read(params.tenantKey, url, cookies))
      redirect(303, `/t/${encodeURIComponent(params.tenantKey)}`);
    methods = await browserSessions.methods(params.tenantKey, url);
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { tenant, reset, notice, ...pick(methods), problem: safeProblem(cause) };
  }
  return {
    tenant,
    reset,
    notice,
    ...pick(methods),
    problem: login === 'failed' ? problem('unauthenticated') : null
  };
};

/** The sign-in methods the page renders. */
function pick({ password, company_sso, providers }: Pick<LoginMethods, 'password' | 'company_sso' | 'providers'>) {
  return { password, companySso: company_sso, providers };
}

/**
 * The safe problem a refused sign-in action answers, telling the browser
 * when a throttled one may retry.
 */
function refusal(cause: unknown, setHeaders: (headers: Record<string, string>) => void) {
  const value = safeProblem(cause);
  const wait = retryAfter(value);
  if (wait) setHeaders({ 'retry-after': String(wait) });
  return value;
}

export const actions: Actions = {
  /** Local password sign-in; the Rust server verifies the password and sets the session cookie. */
  password: async ({ params, request, cookies, url, setHeaders }) => {
    let target: string;
    let username = '';
    try {
      if (localAuthEnabled()) reject('denied');
      const form = await request.formData();
      username = field(form, 'username', 256);
      target = await browserSessions.password(
        'login',
        params.tenantKey,
        { username, password: field(form, 'password') },
        url,
        cookies
      );
    } catch (cause) {
      if (!isHttpError(cause)) throw cause;
      const value = refusal(cause, setHeaders);
      return fail(value.status, { problem: value, username });
    }
    redirect(303, target);
  },
  sso: async ({ params, cookies, url, setHeaders }) => {
    let target: string;
    try {
      if (localAuthEnabled()) reject('denied');
      target = await browserSessions.begin(params.tenantKey, url, cookies);
    } catch (cause) {
      if (!isHttpError(cause)) throw cause;
      const value = refusal(cause, setHeaders);
      return fail(value.status, { problem: value });
    }
    redirect(303, target);
  },
  /** Shared social sign-in; with an invitation code it redeems that link. */
  social: async ({ params, request, cookies, url, setHeaders }) => {
    let target: string;
    try {
      if (localAuthEnabled()) reject('denied');
      const form = await request.formData();
      const provider = field(form, 'provider', 16);
      if (provider !== 'google' && provider !== 'github') reject('validation');
      const code = form.has('invitationCode') ? field(form, 'invitationCode', 256).trim() : '';
      target = await browserSessions.begin(params.tenantKey, url, cookies, provider, code || undefined);
    } catch (cause) {
      if (!isHttpError(cause)) throw cause;
      const value = refusal(cause, setHeaders);
      return fail(value.status, { problem: value });
    }
    redirect(303, target);
  }
};
