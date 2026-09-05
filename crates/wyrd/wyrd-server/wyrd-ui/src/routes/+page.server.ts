import { dev } from '$app/environment';
import {
  localAuthEnabled,
  mockCookie,
  mockDataEnabled,
  loginScenario,
  loginScenarioCookie,
  mockTenantKey
} from '$lib/server/development';
import { fail, isHttpError, redirect } from '@sveltejs/kit';
import { reject, sessionCookie, sessionLifetime, sessions } from '$lib/server/auth/session';
import { problem, safeProblem } from '$lib/server/problem';
import type { Actions, PageServerLoad } from './$types';

export const load: PageServerLoad = ({ locals, request }) => {
  // Failed actions render their problem on the chooser instead of redirecting it away.
  const destination = locals.session && sessions.destination(locals.session);
  const reauthentication =
    destination && locals.session
      ? sessions.reauthenticationTenant(
          locals.session,
          decodeURIComponent(destination.slice(3))
        )
      : null;
  if (destination && !reauthentication && request.method === 'GET') redirect(302, destination);
  return {
    session: locals.session ? sessions.metadata(locals.session) : null,
    problem:
      locals.sessionProblem?.code === 'WYRD_AUTH_401_UNAUTHENTICATED'
        ? null
        : locals.sessionProblem,
    reauthentication,
    localAuth: localAuthEnabled() && locals.mockData
  };
};

export const actions: Actions = {
  mockData: async ({ locals, request, cookies }) => {
    if (!dev || request.headers.get('origin') !== new URL(request.url).origin)
      return fail(403, { problem: problem('denied'), reauthentication: null });
    const data = await request.formData();
    try {
      if (locals.session) sessions.checkAction(locals.session, request, data.get('csrf'));
      const enabled = data.get('enabled');
      if (enabled !== 'true' && enabled !== 'false') reject('denied');
      const destination = data.get('returnTo');
      if (
        typeof destination !== 'string' ||
        !destination.startsWith('/') ||
        destination.startsWith('//') ||
        destination.includes('\\')
      )
        reject('denied');
      cookies.set(mockCookie, enabled, {
        path: '/',
        httpOnly: true,
        sameSite: 'strict',
        secure: new URL(request.url).protocol === 'https:'
      });
      redirect(303, destination);
    } catch (cause) {
      if (isHttpError(cause))
        return fail(cause.status, { problem: safeProblem(cause), reauthentication: null });
      throw cause;
    }
  },
  loginScenario: async ({ locals, request, cookies }) => {
    if (
      !dev ||
      !mockDataEnabled(cookies) ||
      request.headers.get('origin') !== new URL(request.url).origin
    )
      return fail(403, { problem: problem('denied'), reauthentication: null });
    try {
      const data = await request.formData();
      if (locals.session) sessions.checkAction(locals.session, request, data.get('csrf'));
      const scenario = data.get('scenario');
      if (scenario !== 'single' && scenario !== 'multiple' && scenario !== 'none')
        reject('denied');
      cookies.set(loginScenarioCookie, scenario, {
        path: '/',
        httpOnly: true,
        sameSite: 'strict',
        secure: new URL(request.url).protocol === 'https:'
      });
      const previous = cookies.get(sessionCookie);
      if (previous) sessions.remove(previous);
      cookies.delete(sessionCookie, { path: '/' });
      redirect(303, '/');
    } catch (cause) {
      if (isHttpError(cause))
        return fail(cause.status, { problem: safeProblem(cause), reauthentication: null });
      throw cause;
    }
  },
  login: ({ request, cookies }) => {
    if (!localAuthEnabled() || request.headers.get('origin') !== new URL(request.url).origin) {
      return fail(403, { problem: problem('denied'), reauthentication: null });
    }
    if (!mockDataEnabled(cookies))
      return fail(502, { problem: problem('upstream'), reauthentication: null });
    const scenario = loginScenario(cookies);
    const tenantKey = mockTenantKey();
    if (
      scenario === 'single' &&
      !sessions.principal.memberships.some((member) => member.key === tenantKey)
    )
      return fail(403, { problem: problem('denied'), reauthentication: null });
    const previous = cookies.get(sessionCookie);
    if (previous) sessions.remove(previous);
    const id = sessions.create(
      Date.now(),
      scenario === 'single' ? [tenantKey] : scenario === 'none' ? [] : undefined
    );
    cookies.set(sessionCookie, id, {
      path: '/',
      httpOnly: true,
      secure: new URL(request.url).protocol === 'https:',
      sameSite: 'lax',
      maxAge: sessionLifetime / 1000
    });
    const session = sessions.read(id).session!;
    redirect(303, sessions.destination(session) ?? '/');
  },
  switch: async ({ locals, request }) => {
    try {
      if (!locals.session) reject('unauthenticated');
      const data = await request.formData();
      const key = data.get('tenantKey');
      if (typeof key !== 'string') reject('denied');
      sessions.checkAction(locals.session, request, data.get('csrf'));
      const reauthentication = sessions.reauthenticationTenant(locals.session, key);
      if (reauthentication)
        return fail(401, { problem: problem('unauthenticated'), reauthentication });
      const destination = sessions.switch(locals.session, key, request, data.get('csrf'));
      redirect(303, destination);
    } catch (cause) {
      if (isHttpError(cause))
        return fail(cause.status, { problem: safeProblem(cause), reauthentication: null });
      throw cause;
    }
  },
  reauthenticate: async ({ locals, request, cookies }) => {
    try {
      if (!localAuthEnabled() || !mockDataEnabled(cookies) || !locals.session)
        reject('unauthenticated');
      const data = await request.formData();
      const key = data.get('tenantKey');
      if (typeof key !== 'string') reject('denied');
      sessions.reauthenticate(locals.session, key, request, data.get('csrf'));
      redirect(303, sessions.switch(locals.session, key, request, data.get('csrf')));
    } catch (cause) {
      if (isHttpError(cause))
        return fail(cause.status, { problem: safeProblem(cause), reauthentication: null });
      throw cause;
    }
  },
  logout: async ({ locals, request, cookies }) => {
    try {
      if (!locals.session) reject('unauthenticated');
      const data = await request.formData();
      sessions.checkAction(locals.session, request, data.get('csrf'));
      sessions.remove(cookies.get(sessionCookie)!);
      cookies.delete(sessionCookie, { path: '/' });
      redirect(303, '/');
    } catch (cause) {
      if (isHttpError(cause))
        return fail(cause.status, { problem: safeProblem(cause), reauthentication: null });
      throw cause;
    }
  }
};
