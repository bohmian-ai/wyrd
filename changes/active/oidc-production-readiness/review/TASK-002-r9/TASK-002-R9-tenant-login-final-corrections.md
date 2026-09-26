---
id: TASK-002-R9
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-008, REQ-013, INV-003, AC-005, AC-007]
depends_on: [TASK-002-R8]
parent_task: TASK-002
remediates: [FIND-TASK-002-18, FIND-TASK-002-19, FIND-TASK-002-20, FIND-TASK-002-21, FIND-TASK-002-22]
---

# Serialize provider roles and correct tenant-login contracts

## Authority and immutable review subject

- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Review verdict:
  `changes/active/oidc-production-readiness/review/TASK-002-r9/verdict.md`
- Validated ledger:
  `changes/active/oidc-production-readiness/review/TASK-002-r9/findings-validation.md`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Reviewed candidate: `fa2bda92a7e79471b79b607870c1e86a9f35639c`

This task closes one provider-authority race and four bounded source-contract
or repository-boundary defects without changing the approved login, identity,
machine-authentication, persistence, or public wire behavior.

## Issue diagnoses and required outcomes

### FIND-TASK-002-18 — concurrent callbacks can union provider roles

After a callback verifies the provider assertion and resolves the canonical
User, it replaces that User's roles before `issue_human_session` takes the
existing tenant-qualified refresh-family lock. The replacement statement is
atomic by itself, but two transactions starting from no roles can insert
different role rows without conflicting. The first callback then mints its
role; after it commits, the second callback's later role read can see both the
committed row and its own row, mint their union, and preserve that union as the
durable renewable set. Existing role and journey proofs are sequential.

This violates REQ-008 and INV-003: the second callback can receive authority
not mapped from its verified provider groups. Moving or duplicating checks
after issuance would not repair the durable role set.

**Required outcome:** provider-role replacement, its audit, human-session
issuance, completion sealing, and commit serialize for one canonical User so
each issued token and the final durable roles equal one verified callback's
exact mapped set.

**Recommendation:** reuse the existing tenant-qualified User refresh-family
lock in the callback transaction immediately after canonical User resolution
and before role replacement. Keep it through the existing transaction commit.
Preserve the established family-before-connection order; the later
`issue_human_session` reacquisition is transaction-safe. Do not add a new lock
owner, process mutex, table, isolation mode, retry loop, or role-level locking.

### FIND-TASK-002-19 — generated token contract promises API-key refresh

`TokenResponse::refresh_token` says `wyrd_api_key` exchange can return a
refresh token, and the generated schema copies that statement. Runtime machine
issuance deliberately returns `None`; only human OIDC session issuance and
human refresh rotation fill the field. Wyrd's machine model renews by
re-exchanging the durable credential.

The public contract therefore contradicts REQ-013 and actual server behavior,
encouraging schema consumers to implement a nonexistent machine refresh flow.

**Required outcome:** the source and generated descriptions state that the
optional refresh token belongs only to human OIDC login and human refresh
rotation. Runtime behavior and the wire shape remain unchanged.

**Recommendation:** correct the single source field rustdoc, then regenerate
both checked schema trees through the existing generator. Do not hand-edit the
generated JSON or change `TokenResponse` fields or issuance behavior.

### FIND-TASK-002-20 — token write handler lacks scrubbed tracing

The materially changed `POST /auth/token` handler selects credential exchange,
delegation, refresh, or workload issuance and composes durable refresh and
audit behavior. It lacks the scrubbed `#[tracing::instrument]` required for
write handlers. Default argument capture would be unsafe because its inputs
may contain API keys, assertions, access tokens, and refresh tokens.

**Required outcome:** the token boundary has the repository-standard debug
span without recording any arguments or credential material.

**Recommendation:** apply the existing local
`#[tracing::instrument(level = "debug", skip_all)]` handler pattern. Add no
tracing helper, field capture, middleware, or test-only tracing harness.

### FIND-TASK-002-21 — auth router documentation is incomplete

The changed public `auth_router` now composes login, callback, token, and
issue-key routes under one governor, but its rustdoc remains only “Build auth
routes.” The body also uses `expect` for the static governor configuration
without the required `# Panics` contract.

**Required outcome:** maintainers can see the router's four-surface composition,
shared admission role, and the invariant whose violation panics.

**Recommendation:** replace only the placeholder rustdoc and add the
`# Panics` section. Preserve the static governor, constructor signature, and
body; do not introduce a config type, fallible constructor, helper, or runtime
test for this documentation-only correction.

### FIND-TASK-002-22 — begin-login propagates a raw application pool

Every begin-login request resolves the route slug by calling
`resolve_by_slug_for_app(self.postgres().app_pool(), ...)`. Although the caller
passes the intended application pool, the public raw-`PgPool` signature does
not encode that database-role capability and violates the repository boundary
that keeps pool selection behind `WyrdPostgres`.

This is not a demonstrated cross-tenant disclosure, but it makes the new
pre-authentication path depend on caller discipline rather than the concrete
database owner.

**Required outcome:** begin-login supplies only the typed tenant slug to a
narrow `WyrdPostgres` capability; application-pool selection stays private to
that owner, while generic unknown/suspended/deleted-tenant refusal remains
unchanged.

**Recommendation:** add one narrow inherent slug-resolution operation to the
existing `WyrdPostgres` owner and have it delegate internally to the existing
resolver with the private app pool. Use that capability from
`HumanConnections::begin_login`. Do not duplicate SQL, add a repository trait
or pool wrapper, or refactor unrelated pre-existing resolver callers.

## Preserved behavior and constraints

- Preserve one-use login state, PKCE, nonce, exact redirect and connection
  binding, provider screening, ID-token verification, and sealed completion.
- Preserve `(issuer, sub)` tenant User identity, email non-linking, zero
  default roles, valid tenant-only role mapping, and canonical transactional
  audit.
- Preserve the family-before-connection lock order and every prior refresh,
  replay, revocation, initial-issuance, and connection-cutoff correction.
- Preserve `TenantConn` transaction ownership and forced RLS; callees do not
  commit or roll back the caller's transaction.
- Preserve machine renewal by API-key re-exchange and the existing public
  `TokenResponse` wire shape.
- Preserve generic login refusal for unknown, suspended, deleted, or
  unconfigured tenants.
- Add no dependency, Cargo feature, migration, table, route, error code,
  compatibility path, public API shape, new lock abstraction, or isolation
  change.
- Keep `FIND-TASK-002-1` through `FIND-TASK-002-17` closed.

## Explicit non-goals

- Do not redesign role storage, role mapping, refresh families, or the callback
  transaction.
- Do not add cross-process application locks or broaden serialization across
  different Users.
- Do not issue refresh tokens to machine principals or remove the optional
  refresh field from the shared response.
- Do not add trace fields derived from credential-bearing request data.
- Do not redesign auth router configuration or make it fallible.
- Do not replace the existing slug query or migrate all pre-existing resolver
  callers in unrelated server code.
- Do not add TASK-003 BFF behavior, TASK-004 CLI persistence, or live-provider
  qualification work.

## Acceptance criteria

| Criterion | Finding |
|---|---|
| Two concurrent callbacks for one existing User and disjoint mapped role sets serialize before role replacement; each issued token contains exactly that callback's mapped role and the final durable set equals the later callback's exact set, with correct role-sync audit cardinality and no union. | `FIND-TASK-002-18` |
| The callback reuses the existing tenant-qualified User refresh-family lock before role replacement and preserves the family-before-connection order, transaction ownership, completion, and audit behavior. | `FIND-TASK-002-18` |
| `TokenResponse::refresh_token` and regenerated schemas describe only human OIDC login and human refresh rotation; API-key runtime behavior and wire shape are unchanged. | `FIND-TASK-002-19` |
| The token write handler has scrubbed debug instrumentation that records none of its credential-bearing arguments. | `FIND-TASK-002-20` |
| `auth_router` rustdoc describes the four mounted auth surfaces and shared governor and includes the static-configuration `# Panics` contract, with no implementation change. | `FIND-TASK-002-21` |
| Begin-login no longer imports the raw-pool resolver or calls `app_pool`; it uses one narrow inherent `WyrdPostgres` slug-resolution capability and retains generic refusal and successful resolution behavior. | `FIND-TASK-002-22` |
| All prior TASK-002 corrections, identity journeys, generated contracts, tenant isolation, pool boundaries, formatting, lints, and diff hygiene remain green. | All |

## Focused proof and broader verification

Add one deterministic Postgres test named
`auth::callback::pg_tests::concurrent_callbacks_replace_roles_without_union`
using the production callback and issuance owners. Control the two callback
orderings through the existing database lock-observation approach, not timing
sleeps. Record the expected RED authority union before the correction and the
GREEN exact-role result afterward. Run the exact selector with repository
Postgres setup:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=auth::callback::pg_tests::concurrent_callbacks_replace_roles_without_union)'"
```

Retain the focused login refusal/resolution proof:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib \
  -E 'test(=login::pg_tests::unknown_tenant_and_no_connection_are_indistinguishable)'"
```

Then run the affected broader proof sequentially:

```bash
mise run test:principals:unit
mise run test:principals:integration
mise run test:sql
mise run test:identity:journey
mise run codegen:check
mise run check:from-pools-allowlist
mise run check:tenant-isolation
mise run fmt
mise run lints
git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174..<new-candidate>
```

Directly inspect the corrected token handler attribute, router rustdoc, source
field documentation, generated schema descriptions, and begin-login call
site. Record the RED/GREEN concurrency result and every final command outcome
in this task's implementation-evidence section.

## Material stop conditions

Stop for renewed specification authority if correctness requires changing the
public authentication contract, token response shape, persistence schema,
role model, lock ordering, transaction isolation, security policy, or another
expensive-to-reverse decision beyond these validated corrections. Ordinary
implementation and test details inside the existing owners remain
implementer-owned.
