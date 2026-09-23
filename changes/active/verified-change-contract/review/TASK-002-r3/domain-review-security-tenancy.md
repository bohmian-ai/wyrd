# TASK-002 R3 Security, Authorization, Audit, and Tenancy Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `04f73570397d5123eb767abafa60d016c37de1db`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior reviews and remediation: `review/TASK-002-r1/` and `review/TASK-002-r2/`

`HEAD` resolved to the candidate before and after inspection. This review
independently traced the complete base-to-candidate security-sensitive paths;
prior reports were used only to identify stable finding IDs whose closure had
to be reassessed.

## Reviewed boundary

The review covered:

- fixed and dynamic table description authorization, denial, audit ordering,
  failure behavior, and client caching;
- Gate/Scribe publisher-versus-subject identity, signed Card scope, managed
  tenant and principal stamping, and reserved-table refusal;
- lazy built-in materialization and all tenant inputs reaching the catalog;
- the test fixture that credentials an already registered Service principal;
- the bounded audit-publication chain-head lock and retry behavior;
- test-only audit-publication suppression and the new describe count/failure
  probes, including their Rust, Python, and TypeScript projections; and
- stale-schema rejection and the affected journey evidence.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Describe authorization and audit | `AGENTS.md` §§2, 9; `architecture/agent-rules.md` audit rules; `wyrd-security-posture.md` Authorization and policy / Audit integrity; REQ-127, REQ-128, REQ-145 | `wyrd-server/src/bifrost/service.rs::describe_table`, canonical `audit::authorize`, `pg_bifrost_e2e::denied_describe_is_audited_before_admission_impl` | PASS |
| Tenant authority | `wyrd-security-posture.md` tenant-derived-from-credentials rule; `wyrd-design.md` observation identity; `bifrost-design.md` tenant qualification; INV-007 | `Caller::data_tenant_id` is the only tenant passed from describe to `BifrostCatalog`; `TableRef`, row data, and CardRef never select it; catalog access resolves through tenant-qualified owners | PASS |
| Lazy fixed-table materialization | REQ-127; `bifrost-design.md` lazy built-ins and tenant-qualified catalog | `BifrostCatalog::describe_table` materializes a built-in only after the public service has authorized and audited the caller, and calls `ensure_builtin(tenant, ...)` with the verified caller tenant | PASS |
| Observation subject scope | REQ-075, REQ-076, REQ-118, REQ-121, REQ-145; `wyrd-design.md` Card-to-Run-to-Observation rules | Run views carry exact UID-bearing subject refs; the shared queue carries correlation; Gate/Scribe remain the server owners that validate signed scope and stamp `principal_id`, `card_uid`, and `data_tenant_id` | PASS |
| Existing Service credential fixture | Credential and tenant rules in `wyrd-security-posture.md`; repository test-only boundary | `WyrdTestServer::credential_registered_service` uses the fixture tenant's `TenantConn`, resolves the already projected Service account, creates the API key in that tenant, and grants only requested fixture roles before commit; Python and TypeScript expose only this test harness method | PASS |
| Audit publication contention | single audit publisher and frozen-bound protocol in `AGENTS.md`, `agent-rules.md`, `wyrd-security-posture.md`, and `bifrost-design.md` | `freeze_publication_range` applies a transaction-local 3-second lock timeout and propagates timeout as an error from the aborted transaction; `AuditPublisher::freeze` rolls back on error and retries the unchanged durable range on a later sweep | PASS |
| Audit-publication test switch | `wyrd-security-posture.md` production composition; one-publisher rule | `AppState::audit_publication_disabled`, its builder, and the alternate startup branch are all `cfg(feature = "test-support")`; ordinary and production composition always constructs the canonical publisher | PASS |
| Describe fault/count probes | trust-boundary validation and tenant isolation rules; R2 AC-025 remediation | `WyrdTestServer::{table_describe_count,fail_table_describe,restore_table_describe}` live only in `wyrd-testing`; count queries bind the fixture tenant and table; the interpolated trigger argument rejects every byte except ASCII alphanumeric, underscore, and dot, and its trigger predicate includes the exact fixture tenant | PASS |
| Stale writer fence | REQ-128, AC-025; Bifrost schema authority | `assert_stale_writer_is_fenced` crosses the real server boundary, receives `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` for flush and re-registration, and the enclosing journey verifies no stale row was stored | PASS |

## Security conclusions

- A describe permission is evaluated and its allowed or denied decision is
  appended before namespace validation or catalog access, preventing
  under-privileged table-existence disclosure and failing closed when audit
  staging is unavailable.
- Describe and lazy materialization remain scoped exclusively by the verified
  caller tenant; neither the requested FQN nor an asserted CardRef can widen
  tenant authority.
- Successful describes alone enter the client cache, while denied, unknown,
  or audit-failed describes create neither a cached destination nor a producer.
- Observation rows preserve the required identity split: the authenticated
  principal is the publisher, the signed Card scope authorizes the exact
  subject, and tenant/principal/subject managed columns remain server-owned.
- The registered-Service credential helper is a test fixture over existing
  tenant-scoped identity rows, not a production credential or authorization
  path, and its runtime-language projections remain in testing packages.
- The audit timeout does not translate contention into an idle success and
  does not create a second sink, relay, publisher, or audit authority.
- The describe fault trigger and audit-publication switch are explicit test
  controls; neither is reachable from a production server API or ordinary
  production composition.
- No new secret exposure, unsafe input interpolation, cross-tenant pool use,
  caller-selected tenant, authorization bypass, or alternate audit path was
  found in the cumulative range.

## Prior-finding closure

| Finding | R3 security-domain status | Evidence |
|---|---|---|
| `FIND-TASK-002-9` | CLOSED | Chain-head contention returns an explicit error from an aborted tenant transaction; the next sweep retries the unchanged range rather than treating it as no work. |
| `FIND-TASK-002-12` | CLOSED | The real denied-describe journey proves the stable RBAC refusal, one tenant-bound canonical denied audit row, no cached table, and zero producers. |
| `FIND-TASK-002-4` | CLOSED for security scope | Rejecting own TypeScript symbol keys strengthens trust-boundary validation and introduces no new deserializer or native bypass. |
| `FIND-TASK-002-13` | CLOSED for security scope | The remediated journeys keep trace/media validation before admission and preserve server-owned identity stamping; no authorization or tenant boundary moved client-side. |
| `FIND-TASK-002-14` | CLOSED for security scope | The new probes demonstrate fail-closed startup and cache behavior through the real authorized boundary while remaining test-only and tenant-qualified. |

`FIND-TASK-002-10` is a repository-documentation obligation rather than a
security-domain finding and is left to the repository-standards review.

## Verification evidence and limits

The task records the focused denied-describe and audit-lock tests, all three
SDK journeys, the stale-writer journey, and the complete capability closure as
passing. `verify:bifrost` passed 9/9 at `4fc251ce`; after that point the only
code changes before this candidate were two Oracle test corrections, whose two
focused tests and Clippy are recorded as passing, so the full capability lane
was not rerun at the exact candidate. I inspected those test-only diffs and
found no production authorization, audit, credential, or tenant-boundary
change. I did not rerun the environment-owning Postgres journeys in this review.

`git diff --check c8bb490ad814c0c7770cac33ed7779897ff776e4..04f73570397d5123eb767abafa60d016c37de1db`
was clean during inspection.

## Proposed findings

None.

## Overall result

**PASS.** The cumulative candidate preserves verified-credential tenancy,
signed Card-scope enforcement, fail-closed authorization audit, server-owned
managed identity, and the single canonical audit publication path. The R2
test controls do not enter production composition, and no material security,
authorization, audit-integrity, or tenant-isolation defect remains in the
reviewed TASK-002 boundary.
