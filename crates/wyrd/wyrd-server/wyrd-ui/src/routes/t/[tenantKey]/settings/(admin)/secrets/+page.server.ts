import { isHttpError } from '@sveltejs/kit';
import type { CardTypes } from 'wyrd-ai';
import { act, call, id, read } from '$lib/server/admin';
import { reject } from '$lib/server/auth/session';
import { problem, safeProblem, type WyrdProblem } from '$lib/server/problem';
import type { Connections } from '../sign-in/+page.server';
import type { Actions, PageServerLoad } from './$types';

/** The server that owns a secret, and so its lifecycle. */
export type Owner = 'provider' | 'operator' | 'oidc' | 'issuer';
/** How a secret's value is replaced: one value, a username and password, or a new sign-in candidate. */
type Replace = 'value' | 'basic' | 'stage' | null;
/** One stored secret, as its owner projects it; the value itself is never read. */
export type Secret = {
  owner: Owner;
  /** The owner's own key: credential name, connection id, or issuer URL. */
  key: string;
  name: string;
  kind: string;
  usedBy: string;
  /** When the value was last replaced, where the owner records it. */
  rotated: string | null;
  state: 'active' | 'revoked' | 'disabled' | 'candidate';
  replace: Replace;
  /** What the replacement value is called. */
  field: string;
  /** Owner fields the replacement sends back: the provider tag, the HTTP scheme and header. */
  detail: Record<string, string>;
  renew: boolean;
  revoke: boolean;
};
/** `GET /v1/admin/trusted-issuers` item; the client secret is never returned. */
type TrustedIssuerView = { issuer: string; client_id: string; client_auth: string };

const OWNERS = { provider: 'Gateway provider keys', operator: 'Operator connections', oidc: 'Company SSO', issuer: 'Trusted issuers' } as const;
const OPERATOR = { slack: ['Slack', 'Bot token'], pager_duty: ['PagerDuty', 'Integration key'], http: ['HTTP', 'Bearer token'] } as const;
const host = (url: string) => url.replace(/^https?:\/\//, '').replace(/\/.*$/, '');

function providerRow(view: CardTypes.ProviderCredentialView): Secret {
  const managed = view.source === 'managed_secret';
  const live = view.state === 'active';
  const source = typeof view.source === 'string' ? '' : 'environment' in view.source ? ' · environment' : ' · external secret';
  return {
    owner: 'provider',
    key: view.name,
    name: view.name,
    kind: `Gateway provider key${source}`,
    usedBy: `Gateway · ${view.provider}`,
    rotated: view.rotated_at,
    state: view.state,
    replace: managed && live ? 'value' : null,
    field: 'API key',
    detail: { provider: view.provider },
    renew: managed && live,
    revoke: live
  };
}

function operatorRow(view: CardTypes.OperatorConnectionView): Secret {
  const [label, field] = OPERATOR[view.provider];
  const auth = view.provider === 'http' ? view.auth : null;
  return {
    owner: 'operator',
    key: view.connection_id,
    name: view.name,
    kind: `Operator connection · ${label}`,
    usedBy: 'Operators',
    rotated: null,
    state: view.status === 'active' ? 'active' : 'disabled',
    replace: auth?.scheme === 'basic' ? 'basic' : 'value',
    field: auth?.scheme === 'header' ? `Value of ${auth.name}` : field,
    detail: { provider: view.provider, ...(auth ? { scheme: auth.scheme } : {}), ...(auth?.scheme === 'header' ? { header: auth.name } : {}) },
    renew: true,
    revoke: false
  };
}

function oidcRows(connections: Connections): Secret[] {
  return [connections.active, connections.candidate]
    .filter((view) => view !== null && view.client_auth !== 'Public')
    .map((view) => ({
      owner: 'oidc',
      key: view!.id,
      name: host(view!.issuer),
      kind: 'Company SSO client secret',
      usedBy: view!.state === 'Active' ? 'Sign-in' : 'Sign-in · not on yet',
      rotated: null,
      state: view!.state === 'Active' ? 'active' : 'candidate',
      replace: 'stage',
      field: 'Client secret',
      detail: {},
      renew: true,
      revoke: false
    }));
}

function issuerRow(view: TrustedIssuerView): Secret {
  return {
    owner: 'issuer',
    key: view.issuer,
    name: host(view.issuer),
    kind: 'Trusted issuer client secret',
    usedBy: `Workload sign-in · ${view.client_id}`,
    rotated: null,
    state: 'active',
    replace: 'value',
    field: 'Client secret',
    detail: {},
    renew: true,
    revoke: false
  };
}

/**
 * One inventory composed from the four owners, each read on its own: an owner
 * that refuses or fails shows its safe problem and lists nothing, never an
 * empty success.
 */
export const load: PageServerLoad = async ({ locals }) => {
  const session = locals.browserSession;
  if (!session) return { secrets: [], problems: [{ owner: 'provider' as Owner, label: OWNERS.provider, problem: problem('upstream') }] };
  const owned = async (owner: Owner, rows: () => Promise<Secret[]>) => {
    try {
      return { owner, rows: await rows(), problem: null };
    } catch (cause) {
      if (!isHttpError(cause)) throw cause;
      return { owner, rows: [], problem: safeProblem(cause) };
    }
  };
  const results = await Promise.all([
    owned('provider', async () => (await read<CardTypes.ProviderCredentialView[]>(session, '/admin/gateway/provider-credentials')).map(providerRow)),
    owned('operator', async () => (await read<CardTypes.OperatorConnectionView[]>(session, '/operator-connections')).map(operatorRow)),
    owned('oidc', async () => oidcRows(await read<Connections>(session, '/identity/oidc/connections'))),
    owned('issuer', async () =>
      (await read<TrustedIssuerView[]>(session, '/admin/trusted-issuers')).filter((view) => /^Secret(Basic|Post)$/.test(view.client_auth)).map(issuerRow)
    )
  ]);
  return {
    secrets: results.flatMap((result) => result.rows),
    problems: results.flatMap(({ owner, problem }) => (problem ? [{ owner, label: OWNERS[owner], problem: problem as WyrdProblem }] : []))
  };
};

/** One required, bounded form field. */
function field(form: FormData, name: string): string {
  const value = form.get(name);
  if (typeof value !== 'string' || !value.trim() || value.length > 8192) reject('validation');
  return value.trim();
}

function owner(form: FormData): Owner {
  const value = field(form, 'owner');
  if (!(value in OWNERS)) reject('validation');
  return value as Owner;
}

/** The operator-connection patch that replaces only its secret. */
function operatorSecret(form: FormData) {
  const provider = field(form, 'provider');
  if (provider === 'slack') return { provider, bot_token: field(form, 'value') };
  if (provider === 'pager_duty') return { provider, integration_key: field(form, 'value') };
  if (provider !== 'http') reject('validation');
  const scheme = field(form, 'scheme');
  if (scheme === 'bearer') return { provider, auth: { scheme, token: field(form, 'value') } };
  if (scheme === 'basic') return { provider, auth: { scheme, username: field(form, 'username'), password: field(form, 'value') } };
  if (scheme === 'header') return { provider, auth: { scheme, name: field(form, 'header'), value: field(form, 'value') } };
  reject('validation');
}

export const actions: Actions = {
  /** Create a managed gateway provider key; the value goes to the server once and is never read back. */
  add: (event) =>
    act(event, async (form, session) => {
      const name = field(form, 'name');
      await call(session, 'PUT', `/admin/gateway/provider-credentials/${id(name)}`, {
        name,
        provider: field(form, 'provider'),
        source: { managed_secret: { secret: field(form, 'value') } }
      });
      return { done: `${name} saved.` };
    }),
  /** Replace one secret's value in place through its owner. */
  replace: (event) =>
    act(event, async (form, session) => {
      const key = field(form, 'key');
      const which = owner(form);
      if (which === 'provider')
        await call(session, 'PUT', `/admin/gateway/provider-credentials/${id(key)}`, {
          name: key,
          provider: field(form, 'provider'),
          source: { managed_secret: { secret: field(form, 'value') } }
        });
      else if (which === 'operator') await call(session, 'PATCH', `/operator-connections/${id(key)}`, operatorSecret(form));
      else if (which === 'issuer')
        await call(session, 'PUT', `/admin/trusted-issuers/client-secret?issuer=${id(key)}`, { client_secret: field(form, 'value') });
      else reject('validation');
      return { done: 'Secret replaced. The new value is in use.' };
    }),
  /** Reseal one secret under a fresh data key; the value is unchanged. */
  renew: (event) =>
    act(event, async (form, session) => {
      const key = field(form, 'key');
      const path = {
        provider: `/admin/gateway/provider-credentials/${id(key)}/renew-encryption`,
        operator: `/operator-connections/${id(key)}/renew-encryption`,
        oidc: `/identity/oidc/connections/${id(key)}/renew-encryption`,
        issuer: `/admin/trusted-issuers/renew-encryption?issuer=${id(key)}`
      }[owner(form)];
      await call(session, 'POST', path);
      return { done: 'Encryption renewed. The value is unchanged.' };
    }),
  /** Revoke a gateway provider key; terminal. */
  revoke: (event) =>
    act(event, async (form, session) => {
      const name = field(form, 'key');
      await call(session, 'POST', `/admin/gateway/provider-credentials/${id(name)}/revoke`);
      return { done: `${name} revoked.` };
    })
};
