import { fail, isHttpError, redirect } from '@sveltejs/kit';
import { reject } from '$lib/server/auth/session';
import type { BrowserSession } from '$lib/server/auth/browser-sessions';
import { problem, upstreamProblem, safeProblem } from '$lib/server/problem';
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
/** `GET /v1/identity/login-policy`: whether company SSO is the only human sign-in method. */
export type LoginPolicy = {
  require_company_sso: boolean;
  revision: number;
  connection: { id: string; revision: number } | null;
  ready: boolean;
  users_without_company_identity: number;
};

/** Call the tenant connection API as the signed-in principal; non-2xx becomes a safe problem. */
async function call(
  session: BrowserSession,
  method: string,
  path: string,
  body?: unknown
): Promise<Response> {
  const response = await session.api(method, path, body);
  if (!response.ok) reject(await upstreamProblem(response));
  return response;
}

export const load: PageServerLoad = async ({ locals }) => {
  // Connection administration is a server contract; the development identity has no server session.
  if (!locals.browserSession)
    return { connections: null, policy: null, problem: problem('upstream') };
  const session = locals.browserSession;
  try {
    const [connections, policy] = await Promise.all(
      ['/identity/oidc/connections', '/identity/login-policy'].map(async (path) =>
        (await call(session, 'GET', path)).json()
      )
    );
    return { connections: connections as Connections, policy: policy as LoginPolicy, problem: null };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { connections: null, policy: null, problem: safeProblem(cause) };
  }
}

/** Run one mutation as the signed-in principal and render its refusal as a safe problem. */
async function mutate(
  event: RequestEvent,
  run: (form: FormData, session: BrowserSession) => Promise<unknown>
) {
  try {
    const form = await event.request.formData();
    if (!event.locals.browserSession) reject('unauthenticated');
    await run(form, event.locals.browserSession);
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
    mutate(event, async (form, session) => {
      const clientAuth = text(form, 'clientAuth');
      if (!['SecretBasic', 'SecretPost', 'Public'].includes(clientAuth)) reject('validation');
      const secret = text(form, 'clientSecret');
      const email = text(form, 'emailClaim');
      const groups = text(form, 'groupsClaim');
      const expected = text(form, 'revision');
      await call(session, 'PUT', '/identity/oidc/candidate', {
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
  /**
   * Begin the server's real test sign-in for this candidate revision and send the browser to the
   * provider; the server callback marks the revision tested when that sign-in completes.
   */
  test: async (event) => {
    let authorizationUrl: string | undefined;
    const result = await mutate(event, async (form, session) => {
      const response = await call(session, 'POST', '/identity/oidc/candidate/test', {
        expected_revision: revision(form)
      });
      ({ authorization_url: authorizationUrl } = (await response.json()) as {
        authorization_url: string;
      });
    });
    if (!authorizationUrl) return result;
    redirect(303, authorizationUrl);
  },
  activate: (event) =>
    mutate(event, (form, session) =>
      call(session, 'POST', '/identity/oidc/candidate/activate', {
        expected_revision: revision(form),
        recovery_api_key: text(form, 'recoveryApiKey')
      })
    ),
  /**
   * Require or stop requiring company SSO at the policy and connection revisions the page showed.
   * Rust checks the fresh company sign-in; an unconfirmed form changes nothing.
   */
  policy: (event) =>
    mutate(event, (form, session) => {
      if (form.get('confirm') !== 'on') reject('validation');
      const connectionId = text(form, 'connectionId');
      if (!/^[0-9a-f-]{36}$/i.test(connectionId)) reject('validation');
      const connectionRevision = Number(form.get('connectionRevision'));
      if (!Number.isSafeInteger(connectionRevision) || connectionRevision < 0) reject('validation');
      return call(session, 'PUT', '/identity/login-policy', {
        require_company_sso: text(form, 'require') === 'true',
        expected_revision: revision(form),
        expected_connection: { id: connectionId, revision: connectionRevision }
      });
    }),
  deactivate: (event) =>
    mutate(event, (_, session) =>
      call(session, 'POST', '/identity/oidc/active/deactivate')
    ),
  remove: (event) =>
    mutate(event, (form, session) => {
      const id = text(form, 'id');
      if (!/^[0-9a-f-]{36}$/i.test(id)) reject('validation');
      return call(session, 'DELETE', `/identity/oidc/connections/${id}`);
    })
};
