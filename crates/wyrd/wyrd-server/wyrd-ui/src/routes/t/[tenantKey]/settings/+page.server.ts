import { fail, isHttpError, type Cookies } from '@sveltejs/kit';
import { reject } from '$lib/server/auth/session';
import { checkAction, problemKind, serverSessions } from '$lib/server/auth/server-sessions';
import { problem, safeProblem } from '$lib/server/problem';
import type { Actions, PageServerLoad, RequestEvent } from './$types';

/** Redacted connection projection of `GET /v1/identity/oidc/connections`. */
export type ConnectionView = {
  id: string;
  revision: number;
  state: 'Candidate' | 'Active' | 'Inactive';
  issuer: string;
  client_id: string;
  client_auth: 'SecretBasic' | 'SecretPost' | 'Public';
  claim_mapping: { subject: string; email?: string; groups?: string };
  group_role_map: Record<string, string[]>;
  tested_revision: number | null;
  tested_until: string | null;
  updated_at: string;
};
export type Connections = {
  active: ConnectionView | null;
  candidate: ConnectionView | null;
  callback_url: string | null;
};

/** Call the tenant connection API as the signed-in principal; non-2xx becomes a safe problem. */
async function call(
  tenantKey: string,
  cookies: Cookies,
  method: string,
  path: string,
  body?: unknown
): Promise<Response> {
  const response = await serverSessions.api(tenantKey, cookies, method, path, body);
  if (!response.ok) reject(await problemKind(response));
  return response;
}

export const load: PageServerLoad = async ({ locals, params, cookies }) => {
  // Connection administration is a server contract; the development identity has no server session.
  if (!locals.serverSession) return { connections: null, problem: problem('upstream') };
  try {
    const response = await call(params.tenantKey, cookies, 'GET', '/identity/oidc/connections');
    return { connections: (await response.json()) as Connections, problem: null };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { connections: null, problem: safeProblem(cause) };
  }
}

/** Run one CSRF-checked mutation and render its refusal as a safe problem. */
async function mutate(
  event: RequestEvent,
  run: (form: FormData, tenantKey: string) => Promise<unknown>
) {
  try {
    const form = await event.request.formData();
    checkAction(event.locals, event.request, form.get('csrf'));
    if (!event.locals.serverSession) reject('unauthenticated');
    await run(form, event.params.tenantKey);
    return { done: true };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    const value = safeProblem(cause);
    return fail(value.status, { problem: value });
  }
}

function revision(form: FormData): number {
  const value = Number(form.get('revision'));
  if (!Number.isSafeInteger(value) || value < 0) reject('validation');
  return value;
}

function text(form: FormData, name: string): string {
  const value = form.get(name);
  if (typeof value !== 'string' || value.length > 4096) reject('validation');
  return value.trim();
}

/** Parse `group = role, role` lines into the server's group-to-role map. */
function roleMap(source: string): Record<string, string[]> {
  const map: Record<string, string[]> = {};
  for (const line of source.split('\n').filter((value) => value.trim())) {
    const [group, roles] = line.split('=');
    if (!group?.trim() || roles === undefined) reject('validation');
    map[group.trim()] = roles
      .split(',')
      .map((role) => role.trim())
      .filter(Boolean);
  }
  return map;
}

export const actions: Actions = {
  stage: (event) =>
    mutate(event, async (form, key) => {
      const clientAuth = text(form, 'clientAuth');
      if (!['SecretBasic', 'SecretPost', 'Public'].includes(clientAuth)) reject('validation');
      const secret = text(form, 'clientSecret');
      const email = text(form, 'emailClaim');
      const groups = text(form, 'groupsClaim');
      const expected = text(form, 'revision');
      await call(key, event.cookies, 'PUT', '/identity/oidc/candidate', {
        issuer: text(form, 'issuer'),
        client_id: text(form, 'clientId'),
        client_auth: clientAuth,
        ...(secret ? { client_secret: secret } : {}),
        claim_mapping: {
          subject: 'sub',
          ...(email ? { email } : {}),
          ...(groups ? { groups } : {})
        },
        group_role_map: roleMap(text(form, 'groupRoles')),
        ...(expected ? { expected_revision: revision(form) } : {})
      });
    }),
  test: (event) =>
    mutate(event, (form, key) =>
      call(key, event.cookies, 'POST', '/identity/oidc/candidate/test', {
        expected_revision: revision(form)
      })
    ),
  activate: (event) =>
    mutate(event, (form, key) =>
      call(key, event.cookies, 'POST', '/identity/oidc/candidate/activate', {
        expected_revision: revision(form),
        recovery_api_key: text(form, 'recoveryApiKey')
      })
    ),
  deactivate: (event) =>
    mutate(event, (_, key) =>
      call(key, event.cookies, 'POST', '/identity/oidc/active/deactivate')
    ),
  remove: (event) =>
    mutate(event, (form, key) => {
      const id = text(form, 'id');
      if (!/^[0-9a-f-]{36}$/i.test(id)) reject('validation');
      return call(key, event.cookies, 'DELETE', `/identity/oidc/connections/${id}`);
    })
};
