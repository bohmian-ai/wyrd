# Sensitive-domain review — tenancy, persistence, and audit

## Subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `072cf8b30c7135e8cf15f92da3e371a9c999703c`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 7
- Scope: tenant isolation and RLS, operator-plane persistence, migrations,
  principal and credential durability, concurrency, and transactional audit.
- Subject stability: `HEAD` remained `072cf8b30c7135e8cf15f92da3e371a9c999703c`
  throughout this review.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Tenant identity and RLS | `AGENTS.md` §§2, 9; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md` “Tenant and data isolation”; `architecture/v1/00-foundations/{sql-foundation,postgres-layout}.md` | migrations 20, 22, 24; `TenantConn` query paths; principal routes; API-key exchange; SQL integration tests | PASS except DATA-5 |
| Principal and credential durability | spec REQ-001–011, INV-001–003, INV-007–009; security posture “Principal and credential lifecycle” | migrations 20 and 23; platform and tenant principal, credential, identity, and grant queries; initialization, provisioning, recovery, and revocation owners | PASS |
| Tenant lifecycle and provisioning | spec REQ-025–028, AC-007–008, INV-005–006 | `components/platform/{provisioning,routes,recovery}.rs`; platform provisioning queries; platform journey tests | FAIL — DATA-3, DATA-4 |
| Authorization audit | `AGENTS.md` §2; `architecture/agent-rules.md`; security posture “Audit integrity and privacy”; spec REQ-037 and AC-009 | migration 21; `platform_authz.rs`; identity, provisioning, and recovery callers; canonical `vala.audit_staging` append and publisher | FAIL — DATA-1, DATA-2 |
| Platform/tenant plane separation | spec REQ-018, REQ-031, REQ-041–046, INV-004a/b; architecture constraints “Identity And Tenant Isolation” | physical split between `platform.*` and RLS-protected `wyrd.*`; `PlatformCaller`; `TenantConn`; `OperatorPool`; platform and tenant journey negatives | PASS |
| Cross-space Card-bound identity handoff | `wyrd-design.md` Card identity and space semantics; spec REQ-004 and explicit non-goal preserving `wyrd apply` provisioning | base and candidate migration 01, migration 20, auth projection, Card registration uniqueness, prior task handoff | OUT OF SCOPE — validated below |
| Test-harness persistence comment handoff | actual helper and lookup behavior | `wyrd-testing/src/server.rs:2652-2689`; `service_accounts.rs:10-38` | OUT OF SCOPE — validated below |

## Material proposed findings

### DATA-1 — VIOLATION — platform authorization created a second audit store outside the canonical audit path

**Violated obligation.** `AGENTS.md` and `architecture/agent-rules.md` require one
audit write path: every authorization event is appended through the canonical
`vala.audit_staging` append and only `AuditPublisher` projects it into retained
`vala.system.audit_log`. `architecture/wyrd-security-posture.md` repeats that
`platform.audit_log` or another store is not a historical authority. Spec
REQ-037 and AC-009 require the administrative decisions to participate in that
transactional audit contract.

**Exact location.** `crates/wyrd/wyrd-sql/migrations/20260601000021_platform_authz_audit.sql:1-40`;
`crates/wyrd/wyrd-sql/src/queries/platform/audit_authz.rs:39-74`;
`crates/wyrd/wyrd-auth/src/platform_authz.rs:112-143`.

**Evidence.** The candidate creates `platform.audit_authz`, grants direct
`INSERT`, and has `PlatformAuthorization` write every platform decision there.
The canonical path remains `vala_sql::queries::audit_staging::append_audit`,
whose hash chain, monotonic per-tenant sequence, publisher, and retained-history
projection never read `platform.audit_authz`. A repository-wide caller trace
finds no publisher or external reader for the new table; tests inspect it
directly.

**Observable consequence.** Platform tenant creation, recovery, and identity
administration appear to be audited to their handlers, but those rows never
enter the canonical hash chain or retained `vala.system.audit_log`. Operators
querying the supported audit surface cannot reconstruct these privileged
decisions, and the new rows have none of the canonical tamper-evident or
publication guarantees.

**Required testable correction.** Delete the parallel audit table and its query
slot. Project platform decisions through the existing `AuditEvent` and canonical
append/publisher path, using the repository's reserved system-owner audit
partition for plane-wide decisions and a typed redacted detail for credential
and target-tenant attribution. Extend the canonical principal-kind admission to
the already-public `GlobalAdmin` tag. Prove an allowed and denied platform
decision is present in `vala.audit_staging`, publishes into
`vala.system.audit_log`, and leaves no second audit table.

### DATA-2 — INCORRECT — several platform effects commit after their authorization audit has already committed

**Violated obligation.** Spec REQ-037/AC-009 and the repository audit rule say
the authorization row commits in the same transaction as the decision and
operation; both share a fate, and an unauditable decision fails closed.

**Exact location.** `crates/wyrd/wyrd-server/src/components/platform/identity.rs:111-133,
154-208,285-302,418-438,471-517` and
`crates/wyrd/wyrd-server/src/components/platform/recovery.rs:69-124`.

**Evidence.** The shared identity helper calls
`PlatformAuthorization::authorize`, commits the returned transaction at
`identity.rs:127`, and only then performs the OIDC upsert/delete, principal list,
or status update through independent `OperatorPool` statements. Recovery likewise
commits the allowance at `recovery.rs:84-88`, then opens a separate tenant
transaction and inserts the replacement credential at `:90-124`. All callers of
the helper were traced: configure/read/remove OIDC, list admins, and set admin
status use this split path; only admin registration keeps its writes in the
returned transaction.

**Observable consequence.** A store failure or cancellation after the audit
commit leaves an `allow` record for an effect that never happened. More
importantly, recovery can durably claim that a credential was authorized while
the tenant transaction rolls back and no credential exists. The audit record and
privileged state no longer share the fate promised by REQ-037.

**Required testable correction.** For single-plane platform operations, execute
the effect on the same operator transaction returned by the authorization owner
and commit once. For the named cross-plane recovery capability, do not claim
single-transaction atomicity that the approved SQL architecture forbids: use the
repository's declared committed-handoff/idempotent-consumer boundary, with the
canonical audit event committed at the durable handoff and tenant credential
issuance resumable from that handoff. Prove injected failures before and after
the effect cannot produce an audit/effect contradiction or duplicate plaintext
credential exposure.

### DATA-3 — MISSING — the required tenant list, inspect, suspend, and resume operations do not exist

**Violated obligation.** Spec REQ-028 requires a global administrator to list
tenants, inspect one, suspend it, and resume it. AC-008 requires a real
suspension/resumption proof, including refusal of credentials and live tokens.
TASK-004 carries the same acceptance criteria.

**Exact location.** `crates/wyrd/wyrd-server/src/components/platform/routes.rs:33-44`;
`crates/shared/wyrd-client/src/platform/handle.rs:102-136`;
`crates/wyrd/wyrd-sql/src/queries/platform/provisioning.rs:150-180`.

**Evidence.** The platform tenant router exposes only create and administrative
credential recovery. The shared client likewise exposes only those two tenant
operations. `set_tenant_suspended` exists in the SQL tier but has zero callers;
there is no list or inspect query/handler/client operation. The root grant holds
`tenant_read` and `tenant_suspend`, but no served route evaluates either
permission. The platform journeys contain no suspend/resume request.

**Observable consequence.** A global administrator cannot perform half of the
tenant-lifecycle contract. In particular, the newly added authentication gate
for non-active tenants cannot be reached through the shipped administration
surface, and AC-008 is unproven.

**Required testable correction.** Reuse the existing platform router,
`PlatformAuthorization`, tenant directory owner, shared `Platform` client, and
generated HTTP contract to expose the four required operations; do not add a
second lifecycle service. The suspension mutation and its authorization audit
must commit together. A real-client journey must suspend an active tenant,
prove both fresh authentication and a previously minted live token are refused,
resume it, and prove the same durable principals, grants, and state work again.

### DATA-4 — INCORRECT — a failure in the final provisioning transition can strand a non-retryable `provisioning` tenant

**Violated obligation.** Spec REQ-026–027, INV-006, and AC-007 require every
failed provisioning attempt to be observable as failed, unusable, and resumable
to one tenant and one administrator. The task requires injected failure at each
stage.

**Exact location.** `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:157-181`;
`crates/wyrd/wyrd-sql/src/queries/platform/provisioning.rs:33-64,99-147`;
`crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:1364-1438`.

**Evidence.** Failures inside `establish_tenant_administration` enter the error
arm, but the result of `mark_tenant_failed` is discarded at `:177-180`. A
failure or cancellation in `mark_tenant_active` takes the success arm's `?` at
`:163-166` and never attempts the failed transition at all. In either reachable
case the directory row can remain `provisioning`. `insert_provisioning_tenant`
adopts only `status = 'failed'`; a `provisioning` row returns the slug-taken
conflict and can never resume. The cited journey does not inject a real stage
failure: it manually updates a completed tenant to `failed`, then tests only
that prepared state.

**Observable consequence.** A transient database failure or request
cancellation after tenant-scoped administration commits can permanently burn
the slug while leaving durable principal, grant, and credential rows behind.
The tenant stays unusable but is neither reported failed nor retryable through
the contract.

**Required testable correction.** Make the existing provisioning state machine
own every exit after its directory claim. A retry must be able to reclaim both
`failed` and stale/incomplete `provisioning` state under the original tenant id,
and transition failure must be surfaced rather than discarded. Preserve the
current idempotent role seed and reuse of the existing tenant-admin principal;
do not introduce a second coordinator. Inject failure/cancellation at each
post-claim stage and prove retry converges on one active tenant, one admin
principal, and one usable returned credential without orphaning or duplicate
plaintext exposure.

### DATA-5 — VIOLATION — new persistence owners expose raw pools and caller-handed SQLx transactions

**Violated obligation.** `architecture/agent-rules.md` and the SQL foundation
allow exactly `&mut TenantConn<'_>` and `&OperatorPool` in library signatures
and fields. Raw `PgPool` fields and a caller handing `sqlx::Transaction` into
query functions are expressly prohibited; cross-tenant work must remain behind
a named operator capability.

**Exact location.** `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:85-103`;
`crates/wyrd/wyrd-server/src/components/platform/recovery.rs:33-53`;
`crates/wyrd/wyrd-sql/src/queries/platform/{audit_authz.rs:49-52,credentials.rs:106-113,
identity.rs:267-274,principal_grants.rs:25-29,principals.rs:56-61,provisioning.rs:27-32}`.

**Evidence.** `TenantProvisioning` and `TenantRecovery` store a cloned
`sqlx::PgPool` and construct `TenantConn` directly from it. Six new operator
query entry points take `&mut Transaction<'_, Postgres>` handed in by callers.
These are candidate additions, not unchanged neighboring drift.

**Observable consequence.** The new administration path bypasses the sanctioned
`WyrdPostgres::tenant_conn` acquisition owner and makes the privileged
transaction itself a portable capability. That defeats the repository's
construction allowlist, transaction ownership, and future fencing/audit
enforcement at the operator boundary.

**Required testable correction.** Keep tenant acquisition on the existing
`WyrdPostgres` owner and keep operator transaction execution behind a focused
`OperatorPool` capability rather than exporting raw SQLx types across module
signatures. Preserve the one-commit semantics; this is a boundary correction,
not permission to split atomic operations. Close with
`mise run check:tenant-isolation`, `mise run check:from-pools-allowlist`, and the
focused initialization/platform authorization Postgres tests.

## Validated owner handoffs (not findings in this task)

### Cross-space Card principal name collision

The issue is real and reachable. `wyrd.auth_service_accounts` still has
`UNIQUE (data_tenant_id, name)` at
`migrations/20260601000001_auth.sql:85`, while Card identity and registration
are qualified by kind, space, name, and version. The Card auth projection writes
the Card name into the service-account `name`, so applying same-named Service or
Agent Cards in two spaces under one tenant makes the second principal projection
violate that unique key even though both Cards are valid registry identities.

It is not introduced by this candidate: the constraint and projection are
present at the base, and migration 20 explicitly preserves the constraint.
REQ-004 and the approved non-goal require existing `wyrd apply` Card-bound
principal provisioning to remain unchanged. Relaxing the key also invalidates
the current `service_account_by_card_ref` single-row argument and therefore
requires a Card-identity/spec-owner decision plus a qualified lookup change.
It must remain an explicit spec-owner handoff, not a remediation finding for
`admin-principals`.

### Stale fixture comment

`crates/wyrd/wyrd-testing/src/server.rs:2670-2671` says the principal keeps an
uid-less `card_ref` “for the exact JSONB lookup.” The lookup is now containment
(`card_ref @> $3`) at `service_accounts.rs:29-38`, not exact equality, and the
production auth projection stores the registered uid-bearing ref. The comment
is stale and candidate-introduced, but it affects only a fixture explanation;
the helper's stored value and tested behavior are unchanged. Correct it with the
owning fixture/spec work; it is not a material acceptance defect here.

## Verification limits

- Static review covered the complete base-to-candidate diff and every caller of
  the platform authorization, provisioning, recovery, and lifecycle query seams.
- `git diff --check c5c20754..072cf8b3` passed.
- No Cargo/mise command was started from this review agent because repository
  rules require Cargo-backed commands to run sequentially across agents sharing
  the checkout. Existing artifacts report focused green lanes, but they do not
  exercise DATA-1 through DATA-4.
- The supplied `auth_e2e::cache_ttl_path_also_flips_verdict` failure maps to
  `DelegateError::Database(_)` at `exchange_api_key.rs:791-795` and is reported
  as reproducing at the immutable base. It is therefore not attributed to this
  candidate in this domain ledger; whole-repository green remains unproven until
  the owning baseline defect is resolved and the aggregate reruns.
- The supplied `cargo doc -p wyrd-sql` private intra-doc-link warning is real but
  is outside this task's behavior and outside the current rustdoc CI lane.
  Widening that lane is a separate repository-policy decision, not remediation
  for this candidate.

## Overall result

**FAIL**

The candidate preserves the core RLS tenant split and principal/credential
durability, but it does not satisfy the complete tenant-lifecycle contract and
its platform audit/persistence implementation violates the repository's single
audit path and transaction/capability boundaries.
