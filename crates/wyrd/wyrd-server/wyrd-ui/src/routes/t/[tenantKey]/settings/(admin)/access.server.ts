/**
 * Tenant access administration shared by People, Service accounts and Roles:
 * the server reads behind an account drawer and every access action. The
 * server authorizes each call; a refusal is returned to the form as its safe
 * problem, with nothing assumed changed.
 */
import type { CardTypes, CredentialList } from 'wyrd-ai';
import type { Actions } from '@sveltejs/kit';
import { reject } from '$lib/server/auth/session';
import type { BrowserSession } from '$lib/server/auth/browser-sessions';
import { act, call, id, read } from '$lib/server/admin';
import { field } from '$lib/server/form';

type Scope = CardTypes.PermissionScope;
export type Invitation = {
  id: string;
  recipient: string;
  delivery: 'not_emailed' | 'accepted' | 'failed';
  assignments: CardTypes.ScopedRoleAssignment[];
  expires_at: string;
};
type IssuedInvitation = { invitation: Invitation; invitation_link: string };

/** The page's view of issued links: shown in this result only. */
const links = (issued: IssuedInvitation[]) =>
  issued.map(({ invitation, invitation_link }) => ({
    recipient: invitation.recipient,
    link: invitation_link,
    delivery: invitation.delivery,
    expiresAt: invitation.expires_at
  }));

/**
 * The account drawer's server state for `principalId`: its access summary and,
 * where the server lets this session read them, its key metadata. A key
 * listing the server refuses shows no keys rather than failing the drawer.
 */
export async function account(session: BrowserSession, principalId: string) {
  const [access, keys] = await Promise.all([
    read<CardTypes.AccessSummary>(session, `/principals/${id(principalId)}/access`),
    read<CredentialList>(session, `/principals/${id(principalId)}/credentials?limit=50`).catch(() => null)
  ]);
  return { access, keys };
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

/** The account a grant form names. */
function subject(form: FormData): CardTypes.BindingSubject {
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

/** Every access action; each section page exposes them all so a form posts to the page it is on. */
export const accessActions = {
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
  /** Bind one role to one account at one explicit scope. */
  grant: (event) =>
    act(event, async (form, session) => {
      await call(session, 'POST', '/access/bindings', { subject: subject(form), role: field(form, 'role', 64), scope: scope(form) });
      return { done: 'Access granted.' };
    }),
  /** Remove one direct binding; access another binding still grants stays. */
  unbind: (event) =>
    act(event, async (form, session) => {
      await call(session, 'DELETE', `/access/bindings/${id(field(form, 'bindingId', 64))}`);
      return { done: 'Role removed. Access granted another way is unchanged.' };
    }),
  /** Create or replace a custom role's operations and optional description; an empty description clears it. */
  putRole: (event) =>
    act(event, async (form, session) => {
      const description = form.get('description');
      if (typeof description === 'string' && description.length > 200) reject('validation');
      await call(session, 'PUT', `/access/roles/${id(field(form, 'role', 64).trim())}`, {
        operations: operations(form),
        ...(typeof description === 'string' && description.trim() ? { description: description.trim() } : {})
      });
      return { done: 'Role saved.' };
    }),
  deleteRole: (event) =>
    act(event, async (form, session) => {
      await call(session, 'DELETE', `/access/roles/${id(field(form, 'role', 64))}`);
      return { done: 'Role deleted.' };
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
      return { done: 'Account suspended. Its roles are kept.' };
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
} satisfies Actions;
