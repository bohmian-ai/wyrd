# Persistence, tenancy, concurrency, and audit review

## Subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Immutable candidate: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 7
- Original tasks: `TASK-001` through `TASK-008`
- Prior whole-branch verdict and remediation: `review/whole-branch-01/`

I reviewed the complete cumulative range, then traced the current persistence
flows rather than accepting the remediation evidence table. No Cargo or `mise`
command was run, as requested by the orchestrator.

## Boundary and authority coverage

| Boundary | Authority | Source traced | Result |
|---|---|---|---|
| Platform authorization and canonical audit | `AGENTS.md` single canonical staging/publisher rule; `agent-rules.md:12-14`; REQ-037, AC-009 | `wyrd-auth/platform_authz.rs`; Vala migration 26; `vala-sql/audit_staging.rs`; platform handlers | **FAIL** — DATA-R2-1 |
| SQL capability ownership | `agent-rules.md:6-7`; spec's two-connection boundary | `wyrd-sql/operator_pool.rs`; platform query modules; initialization, registration, authorization, provisioning, recovery callers | **FAIL** — DATA-R2-2 |
| Tenant provisioning durability and retry convergence | REQ-025-027; INV-006; AC-007 | platform provisioning service/query; migrations 20/22/24; real-server provisioning journey | **FAIL** — DATA-R2-3 |
| Platform-administrator lifecycle concurrency | REQ-041-044; INV-011; prior `FIND-admin-principals-8` closure contract | platform principals query, identity handler, SQL and real-server tests | **FAIL** — DATA-R2-4 |
| Tenant principal authorization audit and attribution | REQ-037; TASK-005/006 constraints; AC-009 | tenant caller, audit event, principal routes, `auth/revoke.rs`, token issue/verify claims | **FAIL** — DATA-R2-5 |
| Tenant isolation and admission | REQ-026, REQ-031; INV-007 | TenantConn principal routes, RLS migrations, tenant admission function and revocation resolver | PASS for inspected paths |
| Recovery across platform and tenant boundaries | REQ-018, REQ-032; prior cross-plane atomicity resolution | platform recovery, tenant directory lookup, TenantConn credential insert | PASS with the already-recorded cross-boundary limitation; no distributed transaction is required |
| Parallel audit-store removal | canonical single-write-path authority; prior `FIND-admin-principals-1` | cumulative diff and current migrations/query modules | PASS only for deletion of `platform.audit_authz`; DATA-R2-1 shows the broader finding is not closed |
| Cumulative change scope | task-review PASS rule: no unrelated change in diff | base-to-candidate file list and commits `63bc79127`, `5293546f3` | **FAIL** — DATA-R2-6 |

## Material proposed findings

### DATA-R2-1 — VIOLATION — `FIND-admin-principals-1` remains open: canonical platform audit is detached from most same-plane effects and loses actor attribution

**Violated obligation.** `AGENTS.md` and `architecture/agent-rules.md:12-14`
require the one canonical append and require the decision and same-boundary
effect to commit or roll back together. REQ-037 additionally requires the row
to name the authenticating credential. The prior stable finding's correction
explicitly required each same-plane mutation and its allowance in one
operator-owned transaction.

**Exact evidence.** The alternate `platform.audit_authz` table is gone, and
`PlatformAuthorization::authorize` now appends to `vala.audit_staging`
(`crates/wyrd/wyrd-auth/src/platform_authz.rs:121-143`). But the generic helper
immediately commits that returned transaction
(`components/platform/identity.rs:114-129`). The live mutation callers then use
separate pool operations:

- OIDC configure/delete: `identity.rs:156-210,287-304`;
- platform-principal suspension: `identity.rs:488-529`;
- platform credential issue/revoke: `credentials.rs:79-99,175-208`;
- tenant suspension/resume: `provisioning.rs:287-314`.

If any listed write fails after `authorize` commits, retained history says the
operation was allowed even though its effect did not commit. Conversely, these
mutations cannot be rolled back when the audit transaction fails after their
own write, because the transactions are unrelated. This is precisely the
same-plane failure mode retained by stable `FIND-admin-principals-1`, not the
accepted cross-plane limitation of provisioning/recovery.

The canonical rewrite also discarded attribution it already had available.
`PlatformCaller` carries `credential_id` and says it is carried into audit
(`components/auth/platform_extractor.rs:44-53,169-177`), but
`PlatformAuthorization::authorize` has no credential parameter and
`decision_event` does not encode one (`platform_authz.rs:105-110,153-186`). It
also hard-codes every actor as `PrincipalKindTag::GlobalAdmin` at line 176, so a
human `User` platform administrator is durably mislabeled.

**Observable consequence.** A failed privileged mutation leaves a false durable
allowance. Canonical history cannot answer which credential made a platform
request, and human platform administrators appear as machine global admins.

**Minimum testable correction.** Keep the one `vala.audit_staging` append. For
every same-operator-boundary mutation, retain the `TenantConn` returned by
`PlatformAuthorization`, run the effect through that owner-controlled
transaction, and commit once. Preserve the explicitly accepted separate commit
boundaries for provisioning/recovery. Carry the verified platform principal
kind and optional credential id into the canonical event using the canonical
audit contract rather than another table. Inject each post-authorization write
failure and prove that neither effect nor allowed row commits; prove a
credential session and a federated human session retain distinct, correct
attribution.

### DATA-R2-2 — VIOLATION — `FIND-admin-principals-2` remains open: changed SQL APIs still export raw SQLx transactions

**Violated obligation.** `architecture/agent-rules.md:6` permits only
`&mut TenantConn<'_>` and `&OperatorPool` in library function signatures and
explicitly bans a caller-handed `sqlx::Transaction<'_, Postgres>`. The prior
remediation acceptance criterion required the changed signatures to expose only
those two capabilities.

**Exact evidence.** Current public query functions still accept raw
transactions:

- `queries/platform/principals.rs:56-61`;
- `queries/platform/credentials.rs:106-113`;
- `queries/platform/identity.rs:244-249`;
- `queries/platform/principal_grants.rs:25-29`;
- `queries/platform/provisioning.rs:27-32`.

`OperatorPool::begin` itself returns a naked SQLx transaction to its caller
(`wyrd-sql/src/operator_pool.rs:27-40`). Registration and initialization pass
those transactions among query modules, while provisioning passes
`TenantConn::transaction()` into the raw-transaction API. The implementation
evidence claiming this finding closed is contradicted by the current exported
signatures.

**Observable consequence.** A privileged transaction is a portable capability
outside the two approved owners, so callers can bypass TenantConn binding and
the focused operator/audit lifecycle that the repository requires.

**Minimum testable correction.** Reuse `TenantConn` and `OperatorPool`; remove
raw transaction types from changed public signatures and keep multi-write
operator workflows behind their cohesive owner. Extend the existing boundary
check to select these concrete signatures so the rule cannot regress.

### DATA-R2-3 — INCORRECT — `FIND-004-3` remains open: a resumed tenant can retain an orphaned usable credential

**Violated obligation.** REQ-027 and AC-007 require interruption and retry to
converge on one tenant, one administrative principal, and no orphaned
credentials. The stable finding's correction and proof explicitly required one
usable credential.

**Exact evidence.** Tenant administration commits before directory promotion
(`components/platform/provisioning.rs:174-186,355-424`). On retry,
`establish_tenant_administration` reuses the existing admin principal at
lines 370-394 but unconditionally generates and inserts another credential at
lines 404-420. Therefore cancellation, process death, or a failure in
`mark_tenant_active` after the tenant transaction committed leaves credential A
durable but its plaintext unreturned. The retry inserts credential B and returns
only B; A remains valid and unowned by any operator.

The purported closure journey does not exercise this boundary. Its injected
failure fires before inserting the admin principal
(`platform_admin_e2e.rs:2588-2612`), so the tenant transaction rolls back. Its
"abandoned" case first provisions successfully, manually rewrites the row to
`provisioning`, and retries (`:2677-2708`), which does create an additional
credential, but the test counts only principals (`:2751-2763`) and never counts
or invalidates credentials despite its line 2568 claim of "one usable
credential."

**Observable consequence.** A retry can leave a live administrative credential
whose plaintext was never delivered and cannot be managed by the operator. That
credential silently expands the tenant root-of-trust set and violates the
convergence contract.

**Minimum testable correction.** Make the tenant-administration stage
idempotent as a unit: a resumed attempt must not leave a prior undisclosed
credential live. Reuse the existing tenant transaction and principal/credential
owners; do not add a second provisioning ledger. Add a real fail/cancel seam
after tenant transaction commit and before activation, retry, then assert one
admin principal and exactly one usable credential whose returned plaintext
authenticates.

### DATA-R2-4 — INCORRECT — `FIND-admin-principals-8` remains open: unpinned identities are counted as independent ways into the deployment

**Violated obligation.** The stable finding's accepted correction says an
unpinned row cannot justify root suspension and that competing suspensions must
leave an independently authenticating authorized administrator.

**Exact evidence.** `USABLE_ADMINISTRATORS_SQL` treats any
`platform.principal_identities` row as authentication capability
(`wyrd-sql/src/queries/platform/principals.rs:267-286`), without requiring
`subject IS NOT NULL`/`pinned_at IS NOT NULL`. Registration creates exactly such
an unpinned row and grants it authority (`components/platform/identity.rs:359-406`).
The SQL test describes its fixture as pinned at lines 329-330 but `register`
only inserts the unpinned identity (`pg_platform_identity.rs:52-69`); it never
calls `pin_platform_identity`, so the test passes because it reproduces the
defect.

**Observable consequence.** Pre-registering a human who has never successfully
logged in can make the root credential holder suspendable. If that expected
identity cannot complete login, the deployment has no served administrator.

**Minimum testable correction.** Count a federated identity only after its
subject is durably pinned, while continuing to count a live credential. Keep the
existing advisory-lock transaction. Correct the SQL fixture so it explicitly
tests unpinned, pinned, credentialed, ungranted, and concurrent cases; a newly
registered but never logged-in human must not make the last independently
authenticating root suspendable.

### DATA-R2-5 — VIOLATION — `FIND-005-1` is reopened: tenant audit neither shares every mutation transaction nor names the authenticating credential

**Violated obligation.** REQ-037, TASK-005/006, and AC-009 require every covered
decision to append in the deciding/mutation transaction and name principal,
authenticating credential, permission, resource, tenant, and outcome. The prior
stable finding used that exact obligation.

**Exact evidence.** The newer credential-management routes in
`components/principals/routes.rs` correctly reuse `audit::append_on` and their
`TenantConn`. The separately served principal-revocation route does not:
`auth/revoke.rs:70-93` commits `audit::record_audit` on one connection, then
opens another TenantConn for the epoch mutation. Its module documentation
explicitly claims the split is deliberate at lines 21-24, contradicting the
approved task. A failure after line 88 therefore preserves an allowed audit row
for a revocation that never committed.

Attribution is missing across the tenant audit path as well. Tenant access-token
issuers pass `None` for the `cid` claim (`wyrd-auth-issue/src/lib.rs:193-317`),
`Caller` has no credential id (`components/auth/caller_extractor.rs:13-31`), and
`AuditEvent` has no credential field (`wyrd-spec/src/vala/api.rs:2708-2732`).
`audit_event` consequently records only principal/kind/permission/resource at
`wyrd-server/src/audit/mod.rs:42-60`. The implementation can never satisfy the
task's requirement to attribute a tenant credential decision to the credential
that authenticated it.

**Observable consequence.** Principal revocation can be falsely recorded as
allowed after its mutation failed. Investigators cannot distinguish which of a
principal's overlapping credentials performed any tenant administrative
operation, defeating the audit requirement that motivated concurrent
credential support.

**Minimum testable correction.** Route principal revocation through the same
existing `TenantConn`/`append_on` transaction used by the principal credential
routes. Extend the existing signed-token, verified caller, canonical
`AuditEvent`, staging, and publication path with an optional non-secret
credential id for machine-credential sessions; federated sessions keep it
absent. Do not create a second audit sink. Prove an injected revocation write
failure leaves neither epoch change nor allowed row, and prove two credentials
for one principal produce distinguishable canonical audit records.

### DATA-R2-6 — DRIFT — unrelated verified-change specification work entered the cumulative admin-principals candidate

**Violated obligation.** A task-review `PASS` requires that no unrelated change
enter the base-to-candidate diff. Admin-principals scope does not include
redesigning Verifier/Eval/Drift Card doctrine or planning the verified-change
runtime.

**Exact evidence.** Commits `63bc79127` and `5293546f3` modify
`changes/active/verified-change-contract/spec.md`, six architecture artifacts,
and add eight task packets. The same cumulative range changes the repository's
registrable-kind authority in `AGENTS.md`, `architecture/wyrd-design.md`, and
`architecture/wyrd-doctrine.mdx` from the pre-existing Drift/Eval model to the
separate Verifier decision. Those changes are substantive and not generated
admin-principals artifacts.

**Observable consequence.** This immutable candidate cannot be attributed or
accepted as only the approved admin-principals change; its cumulative verdict
would also silently accept a separate approved-spec revision and task plan.

**Minimum testable correction.** Review admin-principals on an immutable
candidate whose cumulative range excludes the verified-change commits, or
change the declared review subject to an intentionally combined change and
review both approved specifications. Do not delete or rewrite the substantive
verified-change work merely to make this report green.

## Revalidated closures and non-findings

- The prohibited parallel audit table/query/module was deleted. The candidate
  has one durable audit staging path and one publisher. That closes only the
  storage-duplication portion of `FIND-admin-principals-1`; DATA-R2-1 retains
  its transactional and attribution portions.
- Tenant-scoped principal and credential routes derive tenancy from the verified
  caller and use `TenantConn`; the inspected mutations add no manual tenant
  filter or widened tenant query.
- Recovery checks the directory for `active` before opening the tenant
  transaction and reuses the existing tenant admin principal. Its platform
  decision and tenant effect are an expressly accepted cross-plane workflow,
  not a reason to invent a distributed transaction.
- The pre-existing `UNIQUE (data_tenant_id, name)` Card-name collision and the
  stale `wyrd-testing` comment were prior owner handoffs and are not re-raised as
  admin-principals findings.
- The base-red `auth_e2e::cache_ttl_path_also_flips_verdict` failure and the
  rustdoc lane-widening decision remain non-findings for this candidate.

## Verification limits

This was a static domain review. I did not run Cargo or `mise`. The committed
evidence reports green focused lanes, but those lanes cannot close DATA-R2-1,
DATA-R2-2, DATA-R2-3, DATA-R2-4, or DATA-R2-5 because current source directly
contradicts the asserted property; the provisioning and last-admin tests in
particular encode the gaps described above.

The candidate remained `5293546f33b3a5fd9de529098e23ea70d472c412`
throughout inspection.

## Overall result

**FAIL**

Proposed retained IDs: `FIND-admin-principals-1`,
`FIND-admin-principals-2`, `FIND-004-3`, `FIND-admin-principals-8`, and
`FIND-005-1`. Proposed new drift ID: `FIND-admin-principals-R2-1`.
