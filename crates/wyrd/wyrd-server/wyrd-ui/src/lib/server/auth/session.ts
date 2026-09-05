import { randomBytes } from 'node:crypto';
import { error } from '@sveltejs/kit';
import type { SessionMetadata, Tenant, WyrdProblem } from '$lib/views';
import { problem } from '../problem';

export const sessionCookie = 'wyrd_session';
export const sessionLifetime = 15 * 60_000;
export type Membership = Tenant & {
  tenantId: string;
  permissions: string[];
  requiresReauthentication?: boolean;
};
export type LocalPrincipal = {
  subject: SessionMetadata['subject'];
  memberships: Membership[];
};
export type Session = LocalPrincipal & {
  expiresAt: number;
  csrf: string;
  recent?: string;
  confirmedTenants: string[];
};
export type TenantContext = {
  tenant: Membership;
  subject: SessionMetadata['subject'];
  permissions: string[];
};

const localPrincipal: LocalPrincipal = {
  subject: { id: '01990000-0000-7000-8000-000000000001', name: 'Jordan Reyes' },
  memberships: [
    {
      key: 'acme',
      name: 'Acme',
      tenantId: '01990000-0000-7000-8000-000000000002',
      permissions: ['cards:read', 'bifrost_query:read', 'evals:read']
    },
    {
      key: 'research',
      name: 'Research',
      tenantId: '01990000-0000-7000-8000-000000000003',
      permissions: ['cards:read', 'bifrost_query:read', 'evals:read']
    }
  ]
};

/** Process-local development identity; production identity replaces this server boundary. */
export class LocalSessions {
  private readonly sessions = new Map<string, Session>();
  readonly principal: LocalPrincipal;

  constructor(principal: LocalPrincipal = localPrincipal) {
    this.principal = structuredClone(principal);
  }

  create(now = Date.now(), tenantKeys?: readonly string[]): string {
    for (const [id, session] of this.sessions) {
      if (session.expiresAt <= now) this.sessions.delete(id);
    }
    // ponytail: bounded single-process store; use production sessions for multiple replicas.
    if (this.sessions.size >= 1000) this.sessions.delete(this.sessions.keys().next().value!);
    const id = randomBytes(32).toString('hex');
    this.sessions.set(id, {
      ...structuredClone(this.principal),
      memberships: structuredClone(
        this.principal.memberships.filter(
          (member) => !tenantKeys || tenantKeys.includes(member.key)
        )
      ),
      expiresAt: now + sessionLifetime,
      csrf: randomBytes(32).toString('hex'),
      confirmedTenants: []
    });
    return id;
  }

  read(
    id: string | undefined,
    now = Date.now()
  ): { session: Session | null; problem: WyrdProblem | null } {
    const session = id ? this.sessions.get(id) : undefined;
    if (!session) return { session: null, problem: problem('unauthenticated') };
    if (session.expiresAt <= now) {
      this.sessions.delete(id!);
      return { session: null, problem: problem('expired') };
    }
    return { session, problem: null };
  }

  remove(id: string): void {
    this.sessions.delete(id);
  }

  destination(session: Session): string | null {
    const tenants = this.metadata(session).tenants;
    const selected =
      tenants.length === 1 ? tenants[0] : tenants.find((t) => t.key === session.recent);
    return selected ? `/t/${encodeURIComponent(selected.key)}` : null;
  }

  bind(session: Session, key: string): TenantContext {
    const context = this.authorize(session, key);
    if (context.tenant.requiresReauthentication && !session.confirmedTenants.includes(key))
      reject('unauthenticated');
    return context;
  }

  private authorize(session: Session, key: string): TenantContext {
    if (session.expiresAt <= Date.now()) reject('expired');
    const issued = session.memberships.find((member) => member.key === key);
    const current = this.principal.memberships.find((member) => member.key === key);
    if (!issued || !current || issued.tenantId !== current.tenantId) reject('denied');
    return {
      tenant: current,
      subject: session.subject,
      permissions: issued.permissions.filter((permission) =>
        current.permissions.includes(permission)
      )
    };
  }

  reauthenticate(
    session: Session,
    key: string,
    request: Request,
    csrf: FormDataEntryValue | null
  ): void {
    this.checkAction(session, request, csrf);
    this.authorize(session, key);
    // Local sign-in deliberately has no external identity provider or credential exchange.
    if (!session.confirmedTenants.includes(key)) session.confirmedTenants.push(key);
  }

  reauthenticationTenant(session: Session, key: string): Tenant | null {
    const { tenant } = this.authorize(session, key);
    return tenant.requiresReauthentication && !session.confirmedTenants.includes(key)
      ? { key: tenant.key, name: tenant.name }
      : null;
  }

  checkAction(session: Session, request: Request, csrf: FormDataEntryValue | null): void {
    if (session.expiresAt <= Date.now()) reject('expired');
    if (
      request.method !== 'POST' ||
      request.headers.get('origin') !== new URL(request.url).origin ||
      typeof csrf !== 'string' ||
      csrf !== session.csrf
    )
      reject('denied');
  }

  switch(
    session: Session,
    key: string,
    request: Request,
    csrf: FormDataEntryValue | null
  ): string {
    this.checkAction(session, request, csrf);
    const context = this.bind(session, key);
    session.recent = context.tenant.key;
    return `/t/${encodeURIComponent(context.tenant.key)}`;
  }

  metadata(session: Session): SessionMetadata {
    return {
      subject: session.subject,
      expiresAt: session.expiresAt,
      csrf: session.csrf,
      tenants: this.principal.memberships
        .filter((current) =>
          session.memberships.some(
            (member) => current.tenantId === member.tenantId && current.key === member.key
          )
        )
        .map(({ key, name }) => ({ key, name }))
    };
  }
}

export const sessions = new LocalSessions();

export function reject(kind: Parameters<typeof problem>[0]): never {
  const value = problem(kind);
  error(value.status, { ...value, message: value.title });
}
