# TASK-001-R1 implementation review

## Immutable subject

- Approved spec: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-001-integrate-redux-data-plane.md`
- Prior verdict: `changes/active/surfaces-oracle-integration/review/task-001-40a73817d-review-01/verdict.md`
- Remediation task: `changes/active/surfaces-oracle-integration/review/integrated-remediation-01/TASK-001-R1-close-task-review-findings.md`
- Original cumulative base: `089f626c7681f4c8bf8abdaddedb61e4a35a26d5`
- Remediation base: `8377fff9f03cc60de4be3e088569e38984382dc4`
- Candidate: `8e61c03493f5c0ebcc12cdb4a9998df284f7fc8f`
- Result: **FAIL**

The complete cumulative implementation, prior findings, and R1 correction were
reviewed without changing the candidate.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Redux is the sole engine and preserves Bifrost ownership boundaries | Cumulative Gate, Scribe, Oracle, Forge, catalog, and client tree | Redux/server integration and six journeys | PASS |
| Tenant-qualified Scribe WAL, replay, fences, cancellation, and recovery | Cumulative Scribe/Oracle implementation | SQL, Redux, Scribe, Oracle, and Forge lanes | PASS |
| Canonical signal schemas and OTLP attribution converge | Cumulative Gate/schema implementation | OTLP and Redux lanes | PASS |
| Every evaluated permission verdict is recorded exactly once before proceed/refusal | Shared handback helpers cover most routes, but Bifrost create and trusted-issuer failure paths return before append | Existing focused tests do not force these exits | FAIL |
| Allowed decisions share an authoritative operation transaction where one exists | Bifrost success does; Card registration and both delete paths use standalone authorization | Audit-failure tests do not prove effect rollback coupling | FAIL |
| Required system-owner security decisions reach retained audit | System events stage, but nil validation rejects their projection/ingress/binding | Publication journeys use ordinary tenants | FAIL |
| Canonical audit contains authorization decisions rather than engine mechanics | R1 removes login/reconciliation/storage/Scribe/Forge lifecycle events | Source audit and recorded lanes | PASS |
| Audit publication uses deterministic replay, frozen ranges, bounded progress, and settlement | Production sweep is bounded and replay journey is integrated | Runtime progress passes; fixed-bound test is disconnected | FAIL for direct ceiling proof |
| Gate audit composition is static and server-owned | Generic Gate with concrete production/test sinks | Focused Gate and Redux tests | PASS |
| SQL access uses repository capabilities and RLS | `ValaPostgres`, `OperatorPool`, and `TenantConn` replace raw access | SQL and boundary lanes | PASS |
| Public behavior, docs, and touched Rust source follow repository contracts | Public Bifrost docs improve; cumulative Card prose and touched signatures/docs remain stale/incomplete | Lints/docs do not enforce all source rules | FAIL |
| Every materially changed named test has exact executable proof | Many exact tests and broad lanes pass | Changed Card dead-letter test is blocked; other exact selectors are absent | FAIL |
| Task lifecycle follows allowed states | Parent tasks are `review` | R1 says unsupported `implemented` | FAIL |
| Non-goals remain excluded | No new engine, compatibility path, durability identity, scheduler, lease, claim, dependency, or public seam | Diff review | PASS |

## Proposed findings

### TASKREV-R1-01 — INCORRECT: system-owner audit rows cannot publish

- Violated obligation: REQ-026B and REQ-027 require system-attributed security
  decisions to enter retained canonical audit.
- Location: `crates/vala/vala-sql/src/tables/audit/projection.rs`,
  `crates/vala/vala-bifrost/src/scribe/ingress.rs`, and
  `crates/vala/vala-bifrost/src/catalog/tenant_table.rs` reject the nil sentinel
  returned by the active-tenant directory and used by peer/tail audit owners.
- Consequence: reachable system decisions remain in transient staging forever.
- Correction: preserve `SYSTEM_OWNER`, allow it only through trusted internal
  publication to `vala.system.audit_log`, and prove retention, uniqueness,
  staging drain, and continued rejection for non-audit nil ingress.

### TASKREV-R1-02 — MISSING: Bifrost registration can lose Allowed verdicts

- Violated obligation: REQ-026/REQ-027C require exactly one durable verdict per
  received request and transaction coupling when an operation transaction exists.
- Location: `crates/wyrd/wyrd-server/src/bifrost/service.rs` hands an event to
  `crates/vala/vala-bifrost/src/catalog/bifrost_catalog.rs`, where validation,
  physical IO, or the concurrent-existing return can precede the late append.
- Consequence: authorized failed or racing registrations leave no audit record.
- Correction: carry one event through the existing owner, append in the usable
  operation transaction, and consume it once at the standalone boundary only
  when no such transaction exists.

### TASKREV-R1-03 — MISSING: trusted-issuer external IO precedes durable verdict

- Violated obligation: REQ-026 requires a received verdict to be recorded before
  the request proceeds.
- Location: `crates/wyrd/wyrd-server/src/admin/routes.rs` performs OIDC discovery
  and fallible secret work after evaluation but before append.
- Consequence: discovery/sealing failure performs tenant-directed work yet loses
  the Allowed decision.
- Correction: record Allowed through the existing standalone boundary before
  discovery, hold no database transaction across network IO, and do not append a
  second time during issuer insertion.

### TASKREV-R1-04 — VIOLATION: three Card writes split audit from mutation

- Violated obligation: INV-008C requires an Allowed event to share the existing
  authoritative operation transaction.
- Location: Card registration, delete-by-UID, and delete-by-ref authorize in
  `crates/wyrd/wyrd-server/src/cards/routes.rs` before service-owned transactions.
- Consequence: a later SQL failure can leave a committed decision without its
  paired effect.
- Correction: hand the event into those three existing transactions; retain
  standalone audit for no-write outcomes and workflows without one encompassing
  transaction.

### TASKREV-R1-05 — MISSING: fixed publication concurrency is not directly proved

- Violated obligation: R1 requires a fixed non-configurable bound and focused
  regression proof.
- Location: the new publisher unit test applies `for_each_concurrent` to integers
  rather than calling `AuditPublisher`.
- Consequence: it stays green if production becomes serial, unbounded, or changes
  the limit.
- Correction: delete the disconnected scaffolding and extend the existing
  real-server publication journey through the production sweep with more than
  eight blocked tenant cycles.

### TASKREV-R1-06 — VIOLATION: exact required verification is incomplete

- Violated obligation: INV-025, TASK-001, R1, and AGENTS.md require every changed
  named test to execute exactly.
- Location: R1 records `card_reconciler_dead_letters_after_three_failures` as
  blocked and substitutes broad/prefix proof for other changed tests.
- Consequence: cumulative acceptance lacks executable regression evidence.
- Correction: repair the existing Local/Forge test composition needed by that
  scenario and run exact selectors for every materially changed test; exclude
  unrelated CLI defects.

### TASKREV-R1-07 — VIOLATION: source and lifecycle contracts remain incomplete

- Violated obligation: repository rustdoc, bare-signature-type, accurate-authority,
  and lifecycle rules.
- Location: touched audit/Eval/journey/storage signatures and tests; Card service,
  test, and Wyrd-design lifecycle prose; R1 frontmatter.
- Consequence: source contracts contradict authorization-only audit, and the task
  uses unsupported `status: implemented`.
- Correction: perform only the cumulative diff-scoped docs/import corrections and
  change R1 to `review`.

### TASKREV-R1-08 — INCORRECT: service-account authorization bypasses its owner

- Violated obligation: the configured `PermissionCheck` is the runtime RBAC
  chokepoint.
- Location: `crates/wyrd/wyrd-server/src/audit/mod.rs` calls the shortcut
  `require_service_accounts_write` from the shared audit helper.
- Consequence: effect and audit can follow a verdict different from an injected
  configured checker.
- Correction: use the existing configured checker, preserve action-specific
  denial mapping, and prove response/effect/one-row behavior under disagreement.

## Verification limits

The recorded format, lint, codegen, docs, SQL, Redux/server integration, journey,
and boundary lanes support their exercised paths. They cannot establish the
reachable missing-verdict paths, system-owner retention, transaction rollback
coupling, the production concurrency ceiling, or the changed test that did not
execute. The broken extra mise entries and unrelated CLI base failures are not
candidate findings.
