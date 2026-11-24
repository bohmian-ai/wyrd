import { fail, isHttpError, redirect } from '@sveltejs/kit';
import { platformConsole } from '$lib/server/auth/platform-sessions';
import { field } from '$lib/server/form';
import { retryAfter, safeProblem } from '$lib/server/problem';
import { worded } from '$lib/platform-copy';
import type { Actions, PageServerLoad } from './$types';

export const load: PageServerLoad = async ({ url, cookies }) => {
  try {
    if (await platformConsole.read(url, cookies)) redirect(303, '/platform/ui');
    return { methods: await platformConsole.methods(url), ended: url.searchParams.has('ended'), problem: null };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { methods: { password: true, oidc: false }, ended: false, problem: safeProblem(cause) };
  }
};

/** The safe problem a refused sign-in answers, telling a throttled browser when it may retry. */
function refusal(cause: unknown, setHeaders: (headers: Record<string, string>) => void) {
  const value = worded(safeProblem(cause), 'signIn', 401);
  const wait = retryAfter(value);
  if (wait) setHeaders({ 'retry-after': String(wait) });
  return value;
}

export const actions: Actions = {
  /** Platform-user password sign-in; Rust verifies the password and sets the console cookie. */
  password: async ({ request, cookies, url, setHeaders }) => {
    let username = '';
    try {
      const form = await request.formData();
      username = field(form, 'username', 256);
      await platformConsole.password(username, field(form, 'password'), url, cookies);
    } catch (cause) {
      if (!isHttpError(cause)) throw cause;
      const value = refusal(cause, setHeaders);
      return fail(value.status, { problem: value, username });
    }
    redirect(303, '/platform/ui');
  },
  /** Platform SSO sign-in; Rust sets the login binding cookie and names the provider URL. */
  sso: async ({ cookies, url, setHeaders }) => {
    let target: string;
    try {
      target = await platformConsole.beginSso(url, cookies);
    } catch (cause) {
      if (!isHttpError(cause)) throw cause;
      const value = worded(refusal(cause, setHeaders), 'noSso', 404);
      return fail(value.status, { problem: value });
    }
    redirect(303, target);
  }
};
