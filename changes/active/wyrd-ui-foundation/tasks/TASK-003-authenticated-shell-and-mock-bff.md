---
id: TASK-003
title: Authenticated tenant shell and mock BFF
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-001, REQ-002, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-066, REQ-081, REQ-092, REQ-093, REQ-094, REQ-095, REQ-100, REQ-101, REQ-106, REQ-129, INV-001, INV-002, INV-008, INV-009, INV-012, INV-014, INV-021, AC-001, AC-002, AC-003, AC-010]
depends_on: [TASK-002]
parent_task:
remediates: []
---

# Outcome and value

Create the trusted application boundary used by every workspace: local
authentication, tenant routing, five-entry responsive shell, Home, typed mock
data, and one server-only Wyrd client seam.

# Owner and write set

- Own root/tenant layouts, `/` and `/t/[tenantKey]`, `hooks.server.ts`, session
  and tenant server modules, shared app chrome, and mock-client infrastructure.
- Implement an opaque HttpOnly local session containing subject, authorized
  tenants, scopes, expiry, and CSRF context; expose only safe page metadata.
- Use one concrete server-only client. Future transport replacement must not
  change browser component contracts or add a duplicate `/api/ui` tree.
- Keep shell, identity, route ownership, and server access out of the component
  catalog created by TASK-002.

# Locked decisions and non-goals

- The URL requests a tenant; server-side authorization establishes it. `space`
  is a page filter, never another global switcher.
- Tenant switching is a same-origin action that revalidates membership and
  returns to the destination Home without retargeting other tabs.
- No production OIDC, durable domain connection, global Inbox/Connections,
  browser-held Wyrd token, auth dependency, or alternate error vocabulary.

# Ordered test scenarios

1. Unauthenticated and expired sessions yield safe access states.
2. Root resolution handles zero, one, and multiple authorized tenants.
3. Unknown or unauthorized tenant keys fail without returning tenant data.
4. The responsive shell shows exactly Home, Cards, Observe, Changes, and Query,
   always identifies the tenant, and exposes switching only when applicable.
5. The switch action revalidates authorization and rejects missing CSRF context.
6. Home and safe `WyrdProblem` failures flow through real server loads/actions.

# Red-Green-Refactor

Drive one boundary scenario at a time. Consolidate repeated session and tenant
logic on one concrete server owner; do not introduce provider abstractions.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/session.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/routing/tenant.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/app/Shell.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record tenant/session cases, route/load/action ownership, safe error examples,
and desktop/mobile theme captures. Stop before production identity, durable
domain APIs, or any browser credential translation is invented.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
