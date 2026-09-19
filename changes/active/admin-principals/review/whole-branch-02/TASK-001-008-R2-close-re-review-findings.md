# TASK-001-008-R2 — Close admin-principals re-review findings

## Route

Implement this packet with `$wyrd-implement`. Make the smallest cohesive
correction that closes every finding below. A later `$wyrd-task-review` must
reassess the complete cumulative candidate, not only the remediation diff.

## Authority and reviewed candidate

- Approved specification: `changes/active/admin-principals/spec.md`, revision 7.
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md` through
  `TASK-008-*.md`.
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`.
- Reviewed candidate: `5293546f33b3a5fd9de529098e23ea70d472c412`.
- Validated ledger:
  `changes/active/admin-principals/review/whole-branch-02/findings-validation.md`.

The branch owner's verified-change-contract work is approved cumulative content.
Preserve it and reconcile shared authority text with it; do not remove or label
it drift.

## Outcome

Finish the approved two-plane administration contract with transactionally
truthful canonical audit, narrow database capabilities, safe public errors,
usable administrator and credential lifecycles, immediate tenant admission
changes, faithful OpenAPI, real operator-process proof, and compliant commit
provenance. Reuse the existing owners and delete superseded paths. Do not add a
parallel store, audit path, connection abstraction, auth service, role engine,
or test harness.

## Validated diagnosis and required correction

### `FIND-admin-principals-1` — platform allowance and mutation are split

REQ-037 requires a same-plane allowance and its effect to commit together.
`wyrd-auth/src/platform_authz.rs:105` returns an audited `TenantConn`, but
`wyrd-server/src/components/platform/identity.rs:114-125` commits it inside the
shared authorization helper before OIDC connection/admin-status writes.
Platform credential issue/revoke (`components/platform/credentials.rs:86,181`)
and tenant suspension (`components/platform/provisioning.rs:287-310`) repeat the
sequence. A failed write can therefore leave a false durable allowance.

Delete the early commit. Carry the existing audited connection through the
same-plane owner, execute the platform mutation on its transaction, and commit
once. Keep cross-plane provisioning/recovery as explicit resumable boundaries;
do not invent distributed atomicity.

Proof: inject post-authorization mutation failures for identity, credential,
status, and suspension classes and show that neither effect nor allowed audit
row commits, while denied decisions remain durable.

### `FIND-admin-principals-2` — raw SQL capability escapes its owner

`wyrd-sql/src/operator_pool.rs:38` returns raw
`sqlx::Transaction<Postgres>`. Exported functions in platform provisioning,
principals, credentials, grants, and identity accept it, while live provisioning
and recovery owners retain `WyrdPostgres`. This violates the repository's
`TenantConn`/`OperatorPool` boundary and lets callers perform unrestricted SQL.

Move bounded operator transactions behind private inherent `OperatorPool`
behavior and acquire `TenantConn` at the tenant composition boundary. Remove
raw SQLx transaction types from exported signatures and replace broad
`WyrdPostgres` fields with the existing narrow owners. Do not add another
wrapper.

Proof: extend the existing boundary source check to these platform query/server
modules and run the principal/platform integration journeys.

### `FIND-admin-principals-3` — live public errors expose source strings

`wyrd-server/src/auth/revoke.rs:126`, `wyrd-auth/src/revoke.rs:71`, and
`wyrd-server/src/components/admin/routes.rs:669` place source `Display` text in
`WyrdError::Internal.message`; the HTTP problem renderer publishes it. SQL,
crypto, parser, or provider details can escape.

Delete the parallel leaking constructors. Reuse `http::error::internal_failure`
or its exact static-message pattern and emit the source only through structured
tracing.

Proof: force each reachable source failure and assert stable code/message with
no source text in the problem body.

### `FIND-admin-principals-4` — active authorities contradict the shipped model

`architecture/wyrd-security-posture.md:40-69` still closes principals to
`User`, `Service`, `Agent`, and `System`, and says identities are tenant-owned
and Card-bound. The implementation has tenantless platform principals,
`GlobalAdmin`/`TenantAdmin`, and a card-free tenant service principal.
`architecture/wyrd-design.md` also uses inconsistent principal names.

Edit the existing authorities in place to describe the two planes, exact stable
principal names, Card-free exceptions, platform OIDC and revocation behavior,
and the canonical audit rule. Reconcile with the approved verified-change
contract; do not create a parallel explanation.

Proof: docs validation and a focused source-text audit find one consistent
principal set and no universal tenant/Card-bound claim.

### `FIND-admin-principals-8` — last-admin guard counts unusable identities

`wyrd-sql/src/queries/platform/principals.rs:267` counts any identity row as a
usable authentication anchor, including an unpinned subject or one whose OIDC
connection was removed. That can permit suspension of the only administrator
that can actually sign in.

Tighten the existing locked count. A usable administrator must have the fixed
admin grant and either an active credential or a pinned identity backed by the
current OIDC connection. Retain the existing advisory lock; add no state table.

Proof: cover unpinned, pinned/current, removed-connection, live-credential,
ungranted, and concurrent-suspension cases.

### `FIND-admin-principals-13` — generated OpenAPI is incomplete

The live `/auth/token`, `/v1/admin/trusted-issuers`, and
`/v1/admin/workload-bindings` routes are absent from `WyrdApiDoc` and
`openapi.yaml`. Declared problem bodies use ordinary `application/json` while
runtime serves `application/problem+json`, and route-specific stable errors are
not projected. Generated clients cannot discover or faithfully handle these
approved surfaces.

Annotate and register the existing routes under the current OpenAPI owner.
Reuse the existing problem response/error catalog construction per operation;
do not add another catalog.

Proof: add one source-of-truth comparison between served auth/admin routes and
OpenAPI paths, assert problem media and stable codes, then regenerate through
`mise run codegen:check`.

### `FIND-004-3` — provisioning retry can orphan a live credential

`components/platform/provisioning.rs:355-426` reuses the administrator but
always inserts a new API key. Cancellation after that tenant transaction, or a
failed active transition, leaves an undisclosed usable credential; retry creates
another. Current tests fail earlier and count principals, not usable credentials.

Make the existing administration stage idempotent. Before retry returns one new
credential, ensure any prior undisclosed provisioning credential is unusable.
Reuse the current transaction, principal, and credential owners; do not add a
provisioning ledger or secret store.

Proof: fail/cancel after credential commit but before activation, retry, and
assert one administrator and exactly one usable credential matching the
returned plaintext.

### `FIND-005-1` — tenant admin audit and effects use different transactions

`components/admin/routes.rs:244,334,397,467` records an allowance and then uses
a new `TenantConn` for issuer/binding mutation. `auth/revoke.rs:86` does the same
before bumping the principal epoch. These live routes can record effects that
failed or apply effects without the matching decision.

Reuse one `TenantConn` and canonical `append_on` for each allowed same-plane
mutation, then commit once. Keep denied decisions independently durable. Delete
the route-local claim that an allowed audit must be standalone.

Proof: inject mutation and audit-append failures for issuer, binding, and
principal revocation and assert no allowance/effect mismatch.

### `FIND-003-2` — initialization proof bypasses the shipped process

The binary has `wyrd-server init`, but all initialization journeys call
`initialize_platform_root` directly. They do not prove clap parsing, exit
status, stdout/stderr separation, repeat refusal, or one-time secret delivery.

Extend the existing process journey to spawn the real binary. Do not introduce
another harness.

Proof: capture the first and repeated invocations; exactly one credential is
printed only to initial stdout, none reaches stderr/logs, repeat refuses safely,
and the existing concurrency/failure-retry expectations remain covered.

### `FIND-004-5` — no operator recovery after total credential loss

The server binary exposes only `Init`, which refuses an initialized deployment.
Docs require another live platform credential, so losing all platform bearer
credentials permanently removes administration despite REQ-033.

Add the smallest operator-only recovery action beside initialization. It must
use deployment database/secret access, the existing `OperatorPool`, platform
credential issuer, and existing root principal. It must not be an HTTP path or
create a principal, role engine, grant model, or secret store. Correct the
existing operator docs and journey.

Proof: through the real binary, lose/revoke every platform credential, recover
one, authenticate, and complete tenant plus OIDC configuration without manual
SQL.

### `FIND-TASK-001-10` — commit provenance violates repository policy

In the reviewed range, 49 of 100 commits have a noncompliant author or
committer and 84 contain `Co-Authored-By: Claude` or `Claude-Session:`. The
branch owner's approval of verified-change scope did not waive Git identity
rules.

After the implementation tree is final, the branch owner must rewrite only the
unmerged offending commits using the already configured contributor identity
and remove prohibited trailers while preserving every tree. Do not run
`git config`, set identity environment variables, or add replacement trailers.

Proof: compare pre/post tree IDs and audit every base-to-candidate author,
committer, and message for exact compliance.

### `FIND-admin-principals-R2-2` — platform principal kind is discarded

`wyrd-runtime/src/principal.rs:362` and
`wyrd-auth/src/platform_sessions.rs:47` omit kind. Credential and federated
session creation discard the stored kind; `platform_authz.rs:153` consequently
hard-codes every audit event as `GlobalAdmin`, including human `User` records.

Propagate the already stored, verified platform-eligible kind through the
existing session, runtime context, and canonical audit event. Do not infer kind
from permissions or add another context type.

Proof: authenticate root and a federated human, perform the same operation,
and assert their correct distinct kinds in staged and published audit.

### `FIND-admin-principals-R2-3` — audit lacks authenticating credential ID

`PlatformCaller` has a credential ID but authorization loses it; tenant claims
and `Caller` have none. `AuditEvent`, staging/hash, and retained audit schemas
have no credential field. Decisions by two credentials for one principal are
therefore indistinguishable, contrary to REQ-037 and AC-009.

Add one optional, non-secret credential ID through tenant token
issuance/verification and platform authorization into the existing canonical
`AuditEvent`, staging hash, publisher, and retained schema. Federated sessions
use `None`. Do not use free-form detail or another table.

Proof: authenticate two credentials for one principal and assert their distinct
IDs in retained audit; assert federated records use `None` and no plaintext is
stored or rendered.

### `FIND-admin-principals-R2-4` — TenantAdmin refresh token is unusable

API-key exchange returns a refresh token for card-free `TenantAdmin`, but
`wyrd-auth/src/refresh.rs:100+` only rotates Service/Agent and requires their
Card reference. The advertised token always fails.

Extend the existing rotation match for card-free `TenantAdmin`, reusing current
access-token issuance and `insert_refresh_token_rotated`. Do not create an admin
refresh service.

Proof: real TenantAdmin exchange, refresh, and protected request succeed; replay
of the consumed refresh token fails.

### `FIND-admin-principals-R2-5` — tenant admission is cached across lifecycle changes

`wyrd-auth/src/revocation_resolver.rs` performs tenant admission inside the
five-second principal-epoch cache. Suspension emits no tenant-wide invalidation,
so a cached active decision can admit the next request and cached denial can
survive resume. The current journey uses zero TTL and cannot prove production
behavior.

Move tenant admission outside the principal epoch cache and evaluate it for
every request. This is smaller than adding cross-instance invalidation and uses
the existing resolver.

Proof: with a nonzero production-like TTL, prime active, suspend and reject the
next live-token request, then resume and allow the next request.

### `FIND-admin-principals-R2-6` — tenant API-key failures leak timing

`components/auth/routes.rs:65-70` rejects malformed keys before Argon2, while
`wyrd-auth/src/exchange_api_key.rs:219+` rejects cross-tenant, non-admitting,
unknown-prefix, and disabled cases before secret verification. A live known
prefix with a wrong tail performs Argon2, enabling remote enumeration.

Reuse the platform credential fixed dummy-verifier pattern so every invalid
tenant API-key condition performs exactly one Argon2 verification and returns
the same public response. Do not add padding or a crypto abstraction.

Proof: instrument verifier-call count for malformed, cross-tenant, suspended,
unknown-prefix, disabled, revoked/expired, and known-wrong inputs; every path
must perform exactly one verification and return the same problem.

## Constraints and preserved behavior

- Keep the single canonical `vala.audit_staging` -> `AuditPublisher` ->
  `vala.system.audit_log` path. No audit table, WAL, relay, or publisher.
- Preserve tenant RLS and the existing `TenantConn`/`OperatorPool` distinction.
- Preserve one-time plaintext credential delivery and secret redaction.
- Preserve principal revocation semantics: it invalidates issued tokens by
  epoch; it does not retire every credential.
- Preserve all closed whole-branch-01 findings and approved
  verified-change-contract work.
- Do not widen the rustdoc CI lane, change Card cross-space identity, run the
  broad gate, or fix the base-reproduced auth e2e failure in this packet.
- No compatibility routes, duplicate services, speculative abstractions, or
  new dependencies unless an existing mechanism cannot implement an explicit
  requirement.

## Acceptance mapping

| Finding | Acceptance criterion |
|---|---|
| `FIND-admin-principals-1` | Every allowed platform same-plane mutation and canonical audit row commit or roll back together. |
| `FIND-admin-principals-2` | No exported/live platform API exposes raw SQLx pool/transaction capability or retains a broader owner than required. |
| `FIND-admin-principals-3` | Forced internal failures return stable safe problems and keep source text server-side. |
| `FIND-admin-principals-4` | Active design/security authorities consistently document the exact two-plane principal model. |
| `FIND-admin-principals-8` | Only independently usable, granted administrators satisfy last-admin protection under concurrency. |
| `FIND-admin-principals-13` | Served auth/admin routes, problem media, and stable errors are present in generated OpenAPI. |
| `FIND-004-3` | Interrupted provisioning retry leaves exactly one disclosed usable credential and one administrator. |
| `FIND-005-1` | Allowed issuer, binding, and principal-revoke effects are atomic with canonical audit. |
| `FIND-003-2` | The actual init process proves one-time stdout delivery, safe refusal, and exit behavior. |
| `FIND-004-5` | Deployment operator access recovers total platform-credential loss through the actual binary. |
| `FIND-TASK-001-10` | Every base-to-candidate commit has the configured identity and no prohibited AI trailer, with unchanged trees. |
| `FIND-admin-principals-R2-2` | Platform context and audit retain the verified root/human principal kind. |
| `FIND-admin-principals-R2-3` | Canonical retained audit distinguishes authenticating credential IDs and represents federated absence. |
| `FIND-admin-principals-R2-4` | A returned TenantAdmin refresh token rotates once and authenticates; replay fails. |
| `FIND-admin-principals-R2-5` | Suspend and resume affect the immediately following live-token request with nonzero cache TTL. |
| `FIND-admin-principals-R2-6` | Every invalid tenant API-key path performs one verifier call and returns the same public problem. |

## Required verification

Run Cargo-backed commands sequentially. Use repository-managed Postgres wrappers
for database tests and record the exact focused commands for every newly named
test. At minimum:

```bash
mise run fmt:check
mise run lints
mise run check:client-tier
mise run check:unwrap-audit
RUSTDOCFLAGS='-D warnings' mise exec -- cargo doc --locked -p wyrd-sql --no-deps
mise run codegen:check
mise run docs:check
mise run test:principals:unit
mise run test:principals:integration
mise run test:platform:journey
mise run test:bifrost:journey:mcp
mise run test:cli:journey
```

Run the exact focused new tests through `mise exec -- cargo nextest run --locked`
with package, target, and exact `test(=...)` expressions inside the narrowest
repository-managed setup wrapper. The platform journey must be green repeatedly
with production-like nonzero cache TTL; an isolated pass does not close its
observed nondeterminism.

Do not run `mise run gate`; approved VER-003 excludes it for this change.
