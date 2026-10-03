# Focused follow-up review

## Immutable subject

- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `f451d52be01acba66bae003b8509c95086fc8558`
- Range: `base..candidate` only
- Approved task authority: `changes/active/audit-outbox/spec.md`, revision 1,
  and tasks `01-publication-progress.md`, `02-one-outbox.md`, and
  `03-remove-audit-unavailable.md`
- Supplemental user-approved authority: high-throughput acknowledgement means
  receipt; audit is eventually consistent, retries dependency slowness through
  its server-owned outbox, drains on graceful shutdown, and may lose counted
  unflushed work on hard process termination.

The repository has no `.codegraph/` directory. The candidate identity was
rechecked immediately before this report and remained unchanged.

## 1. Retry authority conflict

**Resolution:** the candidate violates the later controlling user authority,
but correction requires an approved spec revision before implementation
remediation.

`AuditOutbox::commit_tenant` makes one connection-acquire, append, and commit
attempt, then counts every event as lost on any error
(`crates/vala/vala-sql/src/audit_outbox.rs:292-308`).
`AuditOutboxWriter::settle` removes that batch and decrements `pending`
regardless of whether it committed (`:271-288`). Thus pool timeout, connection
loss, failover, or transaction interruption while the process remains alive
has the same terminal outcome as an admitted loss: no queued state survives to
retry. This is distinct from and broader than the accepted full-queue,
shutdown-deadline, and hard-kill windows.

Revision 1 expressly requires the opposite behavior: REQ-003 says a failed
commit loses the event, its expensive-to-reverse decisions accept that loss,
and the AC-002 journey deliberately proves permanent loss
(`changes/active/audit-outbox/spec.md:78-83,126-128,137-141`;
`crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs:871-1018`).
The supplemental principle resolves the product decision—transient dependency
slowness must retry—but it does not make revision 1 internally consistent.
Direct remediation under revision 1 would require an implementer to violate
its explicit required behavior and invert its acceptance proof. The spec must
first revise REQ-003, the durability decision, AC-002 failure semantics, and
REQ-007 shutdown wording to distinguish:

- transient dependency failure: retain the tenant batch, preserve order, and
  retry with bounded backoff while the process is alive;
- full-queue admission loss and any explicitly authorized non-retryable event
  failure: counted terminal loss without delaying the request; and
- hard process termination: counted/observable loss of unflushed in-memory
  work remains accepted.

After that revision, the implementation boundary is bounded: retain the
failed tenant batch in the existing `AuditOutboxWriter`, do not let later work
for that tenant overtake it, leave sibling tenants and the request path
non-blocking, and retry through the existing connection ceiling until success
or the shutdown deadline.

## 2. Tenant-admin decision ordering

**Resolution:** `SEC-TEN-002` is reachable on all six routes, and one shared
root correction exists.

All six handlers receive an allowed `AuditEvent` from
`authorize_service_accounts_write`, but stage it only after fallible work:

| Route | Decision | Work that can escape before staging | Stage |
|---|---:|---|---:|
| create trusted issuer | `components/admin/routes.rs:296-306` | OIDC discovery, secret-envelope preparation, tenant connection (`:308-333`) | `:334` |
| list trusted issuers | `:375-382` | tenant connection (`:384`) | `:385` |
| delete trusted issuer | `:443-450` | tenant connection (`:452`) | `:453` |
| create workload binding | `:523-534` | fallible write projection and tenant connection (`:536-545`) | `:546` |
| list workload bindings | `:592-599` | tenant connection (`:604`) | `:605` |
| delete workload binding | `:658-665` | tenant connection (`:667`) | `:668` |

A discovery failure or tenant-pool acquisition failure therefore returns after
permission was evaluated but before its allowed decision entered the outbox.
That contradicts REQ-005 and the shared boundary documented at
`crates/wyrd/wyrd-server/src/audit/mod.rs:105-135`.

The common producer is the correction boundary. Repository search shows
`authorize_service_accounts_write` has exactly these six callers and none
enriches the returned event. It can stage the allowed event itself and return
`Result<(), WyrdError>`, matching the existing `audit::authorize` owner
(`audit/mod.rs:117-126`), while retaining the specialized denial message.
This prevents recurrence without six downstream guards. Focused proof should
force discovery failure and tenant-connection failure after an allowed check
and observe exactly one staged allowance.

**New proposed finding: FU-ADMIN-001 (INCORRECT).** The six tenant-admin
allowance paths can return or perform outbound work before audit staging.

## 3. Noisy-tenant capacity isolation

**Resolution:** `SYS-002` is proven and REQ-002 requires prevention.

`AuditOutbox::stage` admits solely against one global `pending` counter of
16,384 events (`audit_outbox.rs:40-44,60-66,111-125`). Pending includes queued,
per-tenant waiting, and in-flight work. When tenant A's chain-head transaction
is held, its first batch blocks and all later A events remain charged in
`waiting` (`:175-192,232-268`). A can therefore consume all 16,384 slots; the
next tenant B decision takes the full-queue loss branch before per-tenant
dispatch can help it. The current contention test stages only two events per
tenant (`crates/vala/vala-sql/tests/pg_audit_outbox.rs:131-170`) and does not
exercise this saturation path.

This is not merely the approved fact that a full queue drops a new event.
REQ-002 states that bounded concurrent tenant commits exist so one contended
tenant does not delay another tenant's audit. Here A's contention directly
causes B's audit to disappear despite B having a healthy chain and available
writer connections. The process may keep one global memory bound, but
admission must preserve capacity or fair progress for another tenant. Exact
share/admission mechanics remain a reversible implementation choice. A focused
Postgres test should hold A's chain head, saturate A's permitted backlog, then
prove B commits before A is released while total pending work stays bounded.

**New proposed finding: FU-TENANT-001 (INCORRECT).** One contended tenant can
consume the global pending allowance and cause an unrelated tenant's audit to
be dropped, violating REQ-002's cross-tenant progress outcome.

## 4. Disputed verification closure

### AC-002 Oracle failure injection

**Resolution:** not closed.

The cited `oracle_peer_postgres_audit_routes_security_identity_and_detail`
test (`crates/wyrd/wyrd-server/src/oracle/peer_audit.rs:109-164`) uses a healthy
outbox, shuts it down successfully, and verifies two committed rows. It does
not inject a staging/commit failure, execute a served Oracle request through
that failure, or assert `audit_outbox_commit_failures_total{surface="bifrost"}`.
The Gate/run-start journey's `bifrost` assertion is a Gate write assertion
(`audit_publication.rs:970-1011`), not Oracle proof. No Oracle query or peer
failure-injection test was found. AC-002 therefore still needs the named
family-specific proof. Under the revised retry authority, that proof should
show the Oracle result is unaffected, the failed attempt is counted, the
dependency recovers, and the retained event subsequently commits once.

### AC-005 capacity lane

**Resolution:** command absence is integration naming drift; passing evidence
is still absent and blocks acceptance.

The candidate's `mise.toml:511-528` still exposes the predecessor
`bench:verification:capacity`; it does not define `bench:capacity`. The active
verified-change-contract authority already says the latter replaces the old
verification and ingest lanes
(`changes/active/verified-change-contract/spec.md:1706-1752`). A later TASK-008
integration commit (`1d05642bf`) supplies `[tasks."bench:capacity"]`, so the
missing name is not an audit-outbox production-code defect and should not be
remediated by adding an alias on this task branch.

However, neither the audit-outbox task evidence nor that integration record
contains a full passing canonical run against this cumulative implementation.
The later record contains only a reduced-duration smoke run, which exited 1
as expected and cannot establish the judged sustained and scale-out SLOs.
Earlier `bench:verification:capacity` run 3 failed two-replica scale-out and is
the evidence that motivated this change. AC-005 therefore remains a
verification-evidence blocker: after integration with the canonical task,
`mise run bench:capacity` must complete with the one-replica sustained and both
two-replica verdict steps passing. No code finding is established unless that
run fails.

**Proposed verification finding: FU-VERIFY-001 (MISSING EVIDENCE).** Oracle's
required injected-failure proof and the full canonical capacity result remain
absent. The capacity command-name mismatch itself is not a candidate defect.

## 5. Live authority and documentation conflicts

**Resolution:** the conflicts are real and fall into three correction groups.

1. **Routed architecture/operations authority still specifies deleted owners
   or semantics.** Update:
   - `architecture/wyrd-security-posture.md:358-387` (failed commits are final
     loss, Oracle acceptance WAL/relay, transactional gateway administration);
   - `architecture/operations/README.md:25,50` (Oracle WAL/relay readiness);
   - `architecture/operations/reliability-and-recovery.md:31-36,146-150`
     (audit availability/commit failure and Oracle WAL/relay);
   - `architecture/operations/runbooks.md:116-125` (no retry, and a tenant log
     field the implementation does not emit);
   - `architecture/references/architecture/patterns.md:243-265` and
     `architecture/references/doctrine/architecture-constraints.md:106-122`
     (transactional/fail-closed default plus named exceptions);
   - the same routed remnants identified at
     `architecture/references/languages/rust-core.md:604-612` and
     `architecture/references/domain/vala-architecture.md:77-80`.

2. **Public and generated contract prose still promises same-transaction,
   fail-closed audit.** Update the source first, then regenerate:
   - `crates/wyrd-spec/src/vala/api.rs:2422,2450-2456`;
   - generated `crates/wyrd-spec/schemas/bifrost_audit_event.json:4` and
     `crates/wyrd-spec/tests/schemas/bifrost_audit_event.json:4`;
   - public docs `docs/src/content/docs/bifrost/architecture.svx:313-328`.

3. **Served OpenAPI and touched Rust/client documentation still advertises an
   audit-caused request failure or transaction atomicity.** The concrete live
   locations include:
   - platform route response descriptions:
     `components/platform/routes.rs:79,146,197,269,303,341`,
     `components/platform/credentials.rs:61-64,78-79,122-124,137-138,186-189,206-207`,
     and `components/platform/identity.rs:195,300,359,425,525,592`;
   - gateway route rustdoc/OpenAPI:
     `components/gateway/routes.rs:152-165,256-261,294-299,329-334,366-371,
     405-410,443-448,481-486,516-521,551-556,588-593,620-625,648-653,
     683-688,715-720,748-753,783-788,815-820,1267,1392,1436,1469,1504,
     1545,1575,1613,1647`;
   - auth/server/client rustdoc:
     `crates/wyrd/wyrd-auth/src/callback.rs:65-73,165-173`,
     `crates/wyrd/wyrd-server/src/auth/callback.rs:41-47`,
     `crates/wyrd/wyrd-auth/src/issue_api_key.rs:42`,
     `crates/wyrd/wyrd-server/src/auth/revoke.rs:25-33`,
     `crates/shared/wyrd-client/src/principals/handle.rs:132-135`,
     `crates/wyrd/wyrd-server/src/components/platform/routes.rs:1-6`, and
     `crates/wyrd/wyrd-server/src/components/platform/recovery.rs:38-48`;
   - stale test-support terminology:
     `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:963-969`.

The following matches are intentionally retained and are not live refusal
contracts: `AuditErrorCode::AuditUnavailable` at
`crates/wyrd-spec/src/vala/audit_detail.rs:440` and its generated schema enum
value preserve decoding of historical retained audit rows; proto number/name 6
is reserved at `crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto:30-31`; historical
migration comments and completed/change records remain immutable context.
Likewise, calling `vala.audit_staging` transactional write-ahead state is valid
when it describes the append's own atomic hash-chain transaction, not atomicity
with the protected operation.

**New proposed finding: FU-DOC-001 (INCORRECT).** Live authorities, served
OpenAPI, public docs, generated schema prose, and materially touched rustdoc
still expose the removed transactional/WAL/fail-closed audit contract. The
historical decode/reserved names above must remain.

## Source paths inspected

- Approved spec and all three task packets.
- Every existing report under `changes/active/audit-outbox/review/r1/`.
- Complete `base..candidate` name/status and diff statistics.
- `crates/vala/vala-sql/src/audit_outbox.rs`, its Postgres tests, and canonical
  staging append/publication queries.
- `crates/wyrd/wyrd-server/src/components/admin/routes.rs` and the shared audit
  authorization owner.
- Oracle audit adapters and cited tests in `wyrd-server`, `vala-bifrost-redux`,
  and `wyrd-testing`.
- `mise.toml`, verified-change-contract REQ-171, and the later integration
  commit that defines the canonical capacity task.
- The architecture, operations, public docs, schema source/generated output,
  OpenAPI annotations, and rustdoc locations listed above.

## Overall follow-up result

**RESOLVED.** Each disputed path is source-resolved. Retry-on-dependency-
slowness is controlling behavior but requires a new approved audit-outbox spec
revision before code remediation; admin ordering and noisy-tenant admission are
bounded implementation findings; Oracle proof is missing; capacity command
naming is resolved by the integration task but the full passing run remains an
evidence blocker; and the live documentation conflict is confirmed with
historical decode names excluded.
