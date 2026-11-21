import { fail, isHttpError } from '@sveltejs/kit';
import type { CardTypes, CredentialList } from 'wyrd-ai';
import { reject } from '$lib/server/auth/session';
import type { BrowserSession } from '$lib/server/auth/browser-sessions';
import { field } from '$lib/server/form';
import { problem, safeProblem, upstreamProblem } from '$lib/server/problem';
import type { Actions, PageServerLoad, RequestEvent } from './$types';

type Scope = CardTypes.PermissionScope;
type Invitation = { id: string; recipient: string; delivery: 'not_emailed' | 'accepted' | 'failed'; assignments: CardTypes.ScopedRoleAssignment[]; expires_at: string };
type IssuedInvitation = { invitation: Invitation; invitation_link: string };

/** The page's view of issued links: shown in this result only. */
const links = (issued: IssuedInvitation[]) =>
  issued.map(({ invitation, invitation_link }) => ({
    recipient: invitation.recipient,
    link: invitation_link,
    delivery: invitation.delivery,
    expiresAt: invitation.expires_at
  }));

/** Call `/v1` as the signed-in principal; a non-2xx answer becomes its safe problem. */
async function call(session: BrowserSession, method: string, path: string, body?: unknown): Promise<Response> {
  const response = await session.api(method, path, body);
  if (!response.ok) reject(await upstreamProblem(response));
  return response;
}

const read = async <T>(session: BrowserSession, path: string) => (await (await call(session, 'GET', path)).json()) as T;
const id = (value: string) => encodeURIComponent(value);

/**
 * Tenant access administration as the server answers it to this session: the
 * accounts, roles, groups and bindings, and for `?account=` that account's
 * access summary and, for a Service account, its key metadata. The server
 * authorizes every read; a refusal is the page's whole content.
 */
export const load: PageServerLoad = async ({ locals, url }) => {
  const session = locals.browserSession;
  // Access administration is a server contract; the development identity has no server session.
  if (!session) return { access: null, problem: problem('upstream') };
  const after = url.searchParams.get('after');
  const account = url.searchParams.get('account');
  try {
    const [principals, roles, groups, bindings, selected, invitations] = await Promise.all([
      read<CardTypes.PrincipalPage>(session, `/principals?limit=100${after ? `&after=${id(after)}` : ''}`),
      read<{ roles: CardTypes.RoleView[] }>(session, '/access/roles'),
      read<{ groups: CardTypes.GroupView[] }>(session, '/access/groups'),
      read<{ bindings: CardTypes.RoleBindingView[] }>(session, '/access/bindings'),
      account ? read<CardTypes.AccessSummary>(session, `/principals/${id(account)}/access`) : null,
      read<{ invitations: Invitation[] }>(session, '/principals/users/invitations')
    ]);
    const keys = selected?.kind === 'service' ? await read<CredentialList>(session, `/principals/${id(selected.principal_id)}/credentials?limit=50`) : null;
    return { access: { ...principals, roles: roles.roles, groups: groups.groups, bindings: bindings.bindings, selected, keys, invitations: invitations.invitations }, problem: null };
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    return { access: null, problem: safeProblem(cause) };
  }
};

/** Run one administration action and render its refusal as the server's safe problem; nothing is assumed to have changed. */
async function act<T>(event: RequestEvent, run: (form: FormData, session: BrowserSession) => Promise<T>) {
  try {
    if (!event.locals.browserSession) reject('unauthenticated');
    return await run(await event.request.formData(), event.locals.browserSession);
  } catch (cause) {
    if (!isHttpError(cause)) throw cause;
    const value = safeProblem(cause);
    return fail(value.status, { problem: value });
  }
}

/**
 * The explicit scope a grant form submitted. A missing or unknown scope type is
 * refused here, never read as the whole tenant; the server validates the
 * references and whether the role applies.
 */
function scope(form: FormData): Scope {
  const value = (name: string) => field(form, name, 256).trim();
  switch (form.get('scopeType')) {
    case 'all':
      return 'all';
    case 'card_family':
      return { card: { family: { kind: value('cardKind'), name: value('cardName') } } } as Scope;
    case 'card_version':
      return { card: { version: { kind: value('cardKind'), name: value('cardName'), uid: value('cardUid') } } } as Scope;
    case 'bifrost_schema':
      return { bifrost: { schema: { catalog: value('catalog'), schema: value('schema') } } };
    case 'bifrost_table':
      return { bifrost: { table: { catalog: value('catalog'), schema: value('schema'), table_uid: value('tableUid') } } };
    case 'gateway_deployment':
      return { gateway: { deployment: { deployment: value('deployment') } } } as Scope;
    case 'gateway_model':
      return { gateway: { model: { deployment: value('deployment'), model: value('model') } } } as Scope;
    case 'github_connection':
      return { github_connection: value('connection') } as Scope;
    case 'tool':
      return { tool: value('tool') } as Scope;
    default:
      return reject('validation');
  }
}

/** The account or group a grant form names. */
function subject(form: FormData): CardTypes.BindingSubject {
  const group = form.get('groupId');
  if (typeof group === 'string' && group) return { group: { id: group } } as CardTypes.BindingSubject;
  return { principal: { id: field(form, 'principalId', 64), kind: field(form, 'principalKind', 16) } } as CardTypes.BindingSubject;
}

/** The checked `resource:action` operations of a role form. */
function operations(form: FormData) {
  return form.getAll('operation').map((value) => {
    const [resource, action, extra] = typeof value === 'string' ? value.split(':') : [];
    if (!resource || !action || extra !== undefined) reject('validation');
    return { resource, action };
  });
}

export const actions: Actions = {
  /**
   * Issue one invitation per pasted address, each carrying the same explicit
   * role-and-scope assignment. Addresses may be separated by lines, spaces,
   * commas or semicolons; the server validates them. Links are shown in this
   * result only.
   */
  invite: (event) =>
    act(event, async (form, session) => {
      const recipients = field(form, 'recipients', 32768).split(/[\s,;]+/).filter(Boolean);
      const response = await call(session, 'POST', '/principals/users/invitations', {
        recipients,
        assignments: [{ role: field(form, 'role', 64), scope: scope(form) }]
      });
      const issued = (await response.json()) as { invitations: IssuedInvitation[] };
      return { invitations: links(issued.invitations) };
    }),
  /** Replace a pending invitation's link; the old link stops working and the new one is shown in this result only. */
  resendInvitation: (event) =>
    act(event, async (form, session) => {
      const response = await call(session, 'POST', `/principals/users/invitations/${id(field(form, 'invitationId', 64))}/resend`);
      return { invitations: links([(await response.json()) as IssuedInvitation]) };
    }),
  revokeInvitation: (event) =>
    act(event, async (form, session) => {
      await call(session, 'DELETE', `/principals/users/invitations/${id(field(form, 'invitationId', 64))}`);
      return { done: 'Invitation revoked. Its link no longer works.' };
    }),
  /**
   * Create a Service account. A runtime family names a Service Card family,
   * never a release; it grants no operation. The first key is shown in this
   * result only.
   */
  createService: (event) =>
    act(event, async (form, session) => {
      const family = form.get('family');
      const description = form.get('description');
      const response = await call(session, 'POST', '/principals', {
        name: field(form, 'name', 128).trim(),
        ...(typeof description === 'string' && description.trim() ? { description: description.trim() } : {}),
        ...(typeof family === 'string' && family.trim() ? { workload_restriction: { mode: 'family', kind: 'Service', name: family.trim() } } : {})
      });
      const created = (await response.json()) as { principal_id: string; credential: string };
      return { issued: { id: created.principal_id, credential: created.credential } };
    }),
  /** Bind one role to one account or group at one explicit scope. */
  grant: (event) =>
    act(event, async (form, session) => {
      await call(session, 'POST', '/access/bindings', { subject: subject(form), role: field(form, 'role', 64), scope: scope(form) });
      return { done: 'Access granted.' };
    }),
  /** Remove one direct binding; access another binding still grants stays. */
  unbind: (event) =>
    act(event, async (form, session) => {
      await call(session, 'DELETE', `/access/bindings/${id(field(form, 'bindingId', 64))}`);
      return { done: 'Binding removed. Access granted by another binding is unchanged.' };
    }),
  putRole: (event) =>
    act(event, async (form, session) => {
      await call(session, 'PUT', `/access/roles/${id(field(form, 'role', 64).trim())}`, { operations: operations(form) });
      return { done: 'Role saved.' };
    }),
  deleteRole: (event) =>
    act(event, async (form, session) => {
      await call(session, 'DELETE', `/access/roles/${id(field(form, 'role', 64))}`);
      return { done: 'Role deleted.' };
    }),
  createGroup: (event) =>
    act(event, async (form, session) => {
      await call(session, 'POST', '/access/groups', { name: field(form, 'name', 128).trim() });
      return { done: 'Group created.' };
    }),
  deleteGroup: (event) =>
    act(event, async (form, session) => {
      await call(session, 'DELETE', `/access/groups/${id(field(form, 'groupId', 64))}`);
      return { done: 'Group deleted.' };
    }),
  addMember: (event) =>
    act(event, async (form, session) => {
      await call(session, 'PUT', `/access/groups/${id(field(form, 'groupId', 64))}/members/${id(field(form, 'principalId', 64))}`);
      return { done: 'Member added. They now inherit this group’s access.' };
    }),
  removeMember: (event) =>
    act(event, async (form, session) => {
      await call(session, 'DELETE', `/access/groups/${id(field(form, 'groupId', 64))}/members/${id(field(form, 'principalId', 64))}`);
      return { done: 'Member removed. They no longer inherit this group’s access.' };
    }),
  /**
   * Suspend one account through the server's principal revocation: it signs in
   * and mints no new token, tokens it holds stop within seconds and its
   * unfinished Verifier executions are cancelled. The account and its bindings
   * stay; the server records the reason and refuses one that reads as a secret.
   */
  suspend: (event) =>
    act(event, async (form, session) => {
      await call(session, 'POST', `/principals/${id(field(form, 'principalId', 64))}/revoke`, {
        principal_kind: field(form, 'principalKind', 16),
        reason: field(form, 'reason', 1024).trim()
      });
      return { done: 'Account suspended. Its bindings are kept.' };
    }),
  /** Issue one more key for a Service account; the secret is shown in this result only. */
  issueKey: (event) =>
    act(event, async (form, session) => {
      const response = await call(session, 'POST', `/principals/${id(field(form, 'principalId', 64))}/credentials`);
      const issued = (await response.json()) as { id: string; credential: string };
      return { issued: { id: issued.id, credential: issued.credential } };
    }),
  revokeKey: (event) =>
    act(event, async (form, session) => {
      await call(session, 'DELETE', `/principals/${id(field(form, 'principalId', 64))}/credentials/${id(field(form, 'credentialId', 64))}`);
      return { done: 'Key revoked.' };
    })
};
