# TASK-002 r1 — Domain review: security, tenancy, and audit isolation

Reviewer: fresh Wave 1 `domain-rev`. Immutable subject: base
`c8bb490ad814c0c7770cac33ed7779897ff776e4`, candidate
`fbfc2591a985b288935180098f892aecdf3b8b49`.

## Reviewed boundary

I traced the TASK-002 security boundary end to end through:

- registered Service/Agent principal projection, the test-only API-key issuance
  helper, API-key exchange, signed Card-scope minting, and SDK credential use;
- local alias selection, immutable `CardRef` correlation, explicit-table queue
  insertion, Gate `bifrost_record:write` authorization/audit, Scribe per-row
  signed-scope validation, authoritative UID resolution, and server stamping of
  publisher and tenant identity;
- fixed-table startup description, lazy built-in materialization, dynamic
  `vala.datasets.*` description, unknown-table behavior, and the public
  `observe.record` reserved/system-table refusal;
- the Rust, Python, and TypeScript real-server write/readback journeys and the
  credential privileges and managed columns they actually assert;
- the audit publication freeze, its new three-second Postgres lock timeout,
  publisher outcome classification, retry behavior, and existing lock-contention
  journey; and
- the test-only audit-publication suppression seam from
  `WyrdTestServerBuilder` through `AppState` to `BoundServer`, including the
  `cfg(feature = "test-support")` production split.

## Authority and source coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Authenticated tenant and publisher identity | Spec REQ-118/121/145, INV-007; `architecture/wyrd-design.md` observation identity; `architecture/wyrd-security-posture.md`; `wyrd-testing/src/server.rs`; token/Card-scope owners; Gate and Scribe ingress | **PASS implementation / FAIL proof.** Tenant and `principal_id` remain derived from verified credentials and server-stamped. Scribe resolves `card_uid` only from the signed UID-bearing scope. The new journeys do not assert the tenant or publisher columns or exercise a denied scope. See `SEC-TEN-001`. |
| Card-scoped observation admission | Spec REQ-118/123/125/145; `wyrd-client/src/observe/mod.rs`; `vala-bifrost-redux/src/gate/mod.rs`; `scribe/execution_lanes.rs` | **PASS implementation / FAIL proof.** `Run::for_card` is immutable and local; Gate records the permission decision; Scribe rejects a present CardRef outside the signed scope before stamping. The task's real-server journeys cover only an admin-authorized positive scope. See `SEC-TEN-001`. |
| Unknown, unauthorized, and reserved table refusal | Spec REQ-127/145 and AC-025; `wyrd-client/src/observe/{mod,tests}.rs`; Bifrost describe route/catalog; the three SDK journeys | **FAIL proof.** Reserved names are refused locally and a mock-only Rust test maps a 404 unknown table, but no real-server SDK journey proves an unknown or object-unauthorized `vala.datasets.*` description refusal. See `SEC-TEN-001`. |
| Audit publication timeout semantics | Repository single-publisher rule; Bifrost audit design; security posture audit/security-event rules; `vala-sql/src/queries/audit_staging.rs`; `wyrd-server/src/audit/publication.rs` | **FAIL.** The bounded wait fixes the unbounded lock, but a real `55P03` publication failure is converted to `None` and then reported as `PublishOutcome::Idle`, bypassing the publisher's failure signal. See `SEC-AUD-001`. |
| Test-support production isolation | Security posture production composition; `wyrd-server/src/{state,app/server}.rs`; `wyrd-testing/src/server.rs`; feature manifests | **PASS.** The field, mutator, and conditional publisher suppression exist only under `test-support`; the non-test-support branch always calls `AuditPublisher::from_state`, and the test-support default is `false`. No environment variable, HTTP route, or production config enables the switch. |
| Secrets and credential exposure | Security posture secret rules; Rust/Python/TypeScript harness projections | **PASS for the reviewed boundary.** The API-key plaintext is returned only from explicit test-harness credential methods, while stored material remains hashed. No candidate production log, response, or Card field exposes it. |

Primary authority coverage included `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/wyrd-design.md`,
`architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`, the
reference router and testing/spec-driven references, approved spec revision 32,
TASK-002, `architecture/logic/{run_api,table_schema}.md`, the complete
base-to-candidate diff, and the surrounding owners named above.

## Security Audit

### Critical

None.

### High

None.

### Medium

- **SEC-AUD-001 — classification: VIOLATION.**
  **Violated obligation:** audit publication failures must remain visible and
  retryable security events; idle means no retained history is owed.
  **Location:** `crates/vala/vala-sql/src/queries/audit_staging.rs:239-272` and
  `crates/wyrd/wyrd-server/src/audit/publication.rs:256-262`.
  **Evidence:** `freeze_publication_range` catches Postgres `55P03` after the
  three-second `lock_timeout` and returns `Ok(None)`. `publish_tenant` maps every
  `None` to `PublishOutcome::Idle`, so `publish_logged` emits none of its
  structured failure warning. The transaction is also already aborted by the
  timed-out statement, making this observably different from an empty staging
  prefix even though the API erases that distinction. No test references the
  timeout constant, SQLSTATE, or classifier.
  **Observable consequence:** a stuck chain-head writer can repeatedly delay
  retained audit history while the publisher reports the tenant as idle. An
  operator loses the required signal that audit retention is failing and can
  mistake missing retained history for an empty audit stream.
  **Required correction:** preserve the bounded `lock_timeout`, but propagate or
  explicitly classify `55P03` as a transient publication failure rather than
  `None`, reusing the publisher's existing structured failure path (and its
  security-event metric if supplied by the owning telemetry surface). Add a
  Postgres-backed focused test that holds the tenant head beyond the timeout,
  proves the cycle returns within a bound with the non-idle failure signal, then
  releases the lock and proves the same staged rows publish on a later cycle.

- **SEC-TEN-001 — classification: MISSING verification.**
  **Violated obligation:** REQ-118/121/145 and AC-025 require real-boundary proof
  of the authenticated publisher/tenant split, exact signed Card scope, and
  unknown/unauthorized table refusal; repository rules make the user journey the
  primary contract.
  **Location:** `sdks/wyrd-sdk-rust/tests/observe_run.rs:295-398`,
  `sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:280-327`,
  `sdks/wyrd-sdk-ts/wyrd/tests/integration/observe-run.test.ts:131-265`, and
  `crates/shared/wyrd-client/src/observe/tests.rs:752-766`.
  **Evidence:** all three real-server journeys grant the registered writer the
  `admin` role. Their readback selects `card_uid` and `run_id`, but not
  `principal_id` or `data_tenant_id`. Their only table refusal is the SDK-local
  `vala.drift.observations` prefix guard; the only unknown dataset test uses a
  stub HTTP 404, and no candidate test uses an under-privileged token, an
  object-unauthorized registered dataset, a foreign-tenant table, or a
  CardRef outside the writer's signed scope.
  **Observable consequence:** a regression that stamps the wrong publisher or
  tenant, widens a registered Service credential's Card scope, or lets a writer
  describe another authorized object's table can still leave every recorded
  TASK-002 journey green. The current green evidence therefore does not prove
  the task's security acceptance boundary.
  **Required correction:** extend the real server journey evidence with the
  smallest cases that assert managed `principal_id` and `data_tenant_id` against
  the credential/fixture tenant, reject a scoped observation under a credential
  whose signed graph does not contain that subject, and reject unknown plus
  object-unauthorized `vala.datasets.*` descriptions before queue admission.
  Preserve the existing positive exact-UID readback and assert the stable server
  errors and canonical allow/deny audit rows at the boundaries that evaluate
  `bifrost_table:read` and `bifrost_record:write`.

### Low / Defense In Depth

None.

### Positive Controls

- `credential_registered_service` reuses the principal projected by Card
  registration under the fixture tenant rather than minting a second identity;
  its UID-less lookup still binds kind, name, version, and space under RLS.
- `Run::for_card` returns immutable sibling views, and every emitted row carries
  that view's exact CardRef plus one invocation ID. No mutable active Card can
  retarget sibling observations.
- Gate derives the tenant from verified auth metadata, audits the
  `bifrost_record:write` permission fail-closed, and never accepts a caller tenant
  field. Scribe validates every present row CardRef against the signed scope and
  stamps both `card_uid` and non-null publisher/tenant identity itself.
- `observe.record` accepts only one-level `vala.datasets.<name>` FQNs before any
  describe, so fixed observation, audit, trace, and result table names cannot be
  reached through that high-level generic API.
- Lazy built-in description remains tenant-qualified and materializes the same
  canonical definition Scribe enforces; it does not accept a client schema.
- Audit publication still freezes one tenant range transactionally and preserves
  the shared frozen-bound/deduplicated replay model; the finding concerns loss of
  failure classification, not range identity or cross-tenant isolation.
- `audit_publication_disabled` is absent from a non-`test-support` `AppState`,
  defaults off even in test-support builds, and has no remote/configuration
  control surface.

## Verification evidence and limits

The immutable TASK-002 record reports the queue regression, shared/SDK/Bifrost
family and all three journey lanes, Python/TypeScript checks, codegen, boundary
checks, format, and lint green at the candidate. I inspected those tests and
their selectors but did not repeat the broad suites. The evidence does not
contain a focused lock-timeout test or the negative/managed-identity assertions
described in the findings, so the green aggregate cannot close either gap.

This review did not require future Verifier execution, SYSTEM result-writer,
Operator delivery, or result-table admission behavior assigned to later tasks.
It reviewed only TASK-002's active observation, table-description, tenancy,
Card-scope, audit-publication regression, and test-support isolation paths.

## Overall result

**FAIL.** Production test-support isolation and the implemented tenant/Card
stamping path are structurally sound, but the candidate silently classifies an
audit-publication lock timeout as idle and lacks the required real-boundary
security proof for publisher/tenant identity, signed Card-scope refusal, and
unknown/object-unauthorized table description.
