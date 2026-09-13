# TASK-001-R1 task-review verdict

## Immutable subject

- Approved spec: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-001-integrate-redux-data-plane.md`
- Prior verdict: `changes/active/surfaces-oracle-integration/review/task-001-40a73817d-review-01/verdict.md`
- Remediation task: `changes/active/surfaces-oracle-integration/review/integrated-remediation-01/TASK-001-R1-close-task-review-findings.md`
- Original cumulative base: `089f626c7681f4c8bf8abdaddedb61e4a35a26d5`
- Remediation base: `8377fff9f03cc60de4be3e088569e38984382dc4`
- Candidate: `8e61c03493f5c0ebcc12cdb4a9998df284f7fc8f`
- Verdict: **FIX_REQUIRED**

The source candidate stayed immutable. Review artifacts are outside the candidate
commit.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Redux is the sole Bifrost engine; no legacy authority or compatibility surface | Cumulative Redux tree and prior no-legacy inventory | Client-tier and recorded integration/journey lanes | PASS |
| Scribe WAL, replay, fences, bounded ownership, and tenant-qualified identity | Cumulative Redux Scribe owners | Redux/Scribe/SQL lanes | PASS |
| Oracle one-build execution, admission, authorization, protection, cancellation, and terminal behavior | Cumulative Oracle/server owners | Oracle/server journeys | PASS |
| Forge serialization, progress, recovery, cleanup, and readiness | Cumulative Forge owners | Forge/Redux/SQL journeys | PASS |
| Canonical OTLP/Arrow convergence and attribution | Three canonical signal tables and Gate path | OTLP/Redux lanes | PASS |
| Every received permission verdict is recorded exactly once before proceed/refusal | General helpers close many paths, but Bifrost create failures/races and trusted-issuer discovery can lose Allowed | Existing happy/fail-closed tests omit these reachable exits | FAIL |
| Allowed Card writes share their existing authoritative operation transaction | Registration and both deletes use standalone `audit::authorize` | Existing audit-failure tests do not prove operation-failure rollback coupling | FAIL |
| System-tenant security audit reaches retained history | System rows stage, but projection/Scribe/binding reject the nil sentinel | Existing publication journeys use non-nil tenants | FAIL |
| Canonical audit contains decisions, not engine mechanics | Login, reconciliation, storage lifecycle, Scribe, and Forge appends removed | SQL/Bifrost journeys and source audit | PASS |
| Frozen audit range, deterministic replay, settlement, and cross-tenant progress | Publisher owns frozen cycle and bounded source combinator | Replay and two-tenant journeys | PASS for behavior; fixed-ceiling regression proof FAIL |
| Gate audit composition is static and server-owned | Generic Gate with concrete production/test sink types | Focused Gate and Redux lanes | PASS |
| SQL capability ownership and RLS are repository-native | `ValaPostgres`, `OperatorPool`, `TenantConn`; redundant predicates removed | SQL and boundary checks | PASS |
| Documentation and source standards | Public Bifrost pages corrected; touched Rust docs/signatures remain incomplete/stale | Lints/docs pass but do not enforce private/test contract | FAIL |
| Task lifecycle and exact focused evidence | TASK-001/005/006 are `review`; R1 is `implemented`; one R1-edited Card test is unexecuted | TASK-001's Bifrost-focused lanes pass; the edited dead-letter test has no exact green run | FAIL |
| Non-goals/Ponytail minimum | No new dependency, scheduler, lease, claim, durability identity, compatibility path, public seam, or test file | Diff inspection | PASS |
| Repository standards | Independent specialist | `standards-review.md` | FAIL |

## Material findings

### FIND-TASK-001-R1-1 — INCORRECT: system security audit cannot reach retained history

The active nil tenant is deliberately returned as `SYSTEM_OWNER`, and unverified
peer/tail rejection paths append there. `AuditPublisher` then reaches unconditional
nil rejection in audit projection, Scribe ingress, and physical tenant binding.
REQ-026B and REQ-027 require these rows to retain; instead they accumulate in
staging forever. Permit the reserved sentinel only for the internal canonical
audit-table publication path and prove retained uniqueness plus staging drain.

### FIND-TASK-001-R1-2 — MISSING: Bifrost registration loses Allowed verdicts

`register_table` hands an unpersisted event to catalog creation, whose validation,
physical IO, and concurrent-existing return can exit before the append. Authorized
failed or racing requests therefore vanish from audit. Keep successful create
transaction coupling while ensuring every other post-verdict return consumes the
event exactly once or fails closed.

### FIND-TASK-001-R1-3 — MISSING: trusted-issuer discovery starts before Allowed audit is durable

The handler evaluates `service_accounts:write`, then performs tenant-directed OIDC
discovery and fallible sealing before appending. Those failures produce unaudited
verdicts and outbound IO. Commit the decision through the existing standalone
boundary before discovery; do not hold a DB transaction across network IO or append
again during insert.

### FIND-TASK-001-R1-4 — VIOLATION: Card write audit is split from existing operation transactions

Card registration and delete-by-UID/delete-by-ref commit Allowed audit before
entering the authoritative registry transaction. An operation failure can leave a
decision row without its paired effect. Hand the event to the existing Card owner
and append on its transaction; keep reads, completion, storage sagas, denials, and
no-write outcomes on their validated standalone boundaries.

### FIND-TASK-001-R1-5 — MISSING: the fixed publisher concurrency ceiling is not proved

The unit test duplicates `for_each_concurrent` over integers and never invokes the
publisher. It passes if production becomes serial, unbounded, or uses another
limit. Delete that scaffolding and exercise the production sweep through the
existing real-server journey with more tenants than the fixed bound.

### FIND-TASK-001-R1-6 — VIOLATION: required exact verification is incomplete

R1 materially changed the reconciliation dead-letter test but records it as
unexecuted. Repair only the existing test composition needed by that scenario
and record its exact focused run; TASK-002 owns the five broken Card, CLI, and
WyrdState `mise` launchers and their full suites.

### FIND-TASK-001-R1-7 — VIOLATION: touched Rust and authority documentation remains noncompliant

Touched signatures retain fully qualified types; changed tests/functions omit
required rustdoc, `# Errors`, or `# Panics`; and Card source/test plus Wyrd-design
prose still claims removed lifecycle/dead-letter audit. Correct only the cumulative
changed/contradictory surface.

### FIND-TASK-001-R1-8 — VIOLATION: R1 lifecycle metadata is invalid

R1 uses unsupported `status: implemented`. It must enter `review` before verdict.

### FIND-TASK-001-R1-9 — INCORRECT: service-account decisions bypass the configured checker

`authorize_service_accounts_write` calls a shortcut helper rather than
`state.authz.permission_check`. Admin, issuance, and revocation can therefore audit
and act on a verdict other than the configured runtime owner's. Use the existing
checker, preserve the action-specific error, and prove the response/effect/audit
follow an injected checker verdict exactly once.

## Independent validation

Every finding above was confirmed or narrowed by a fresh Ponytail validator. A
second independent scope validation revised finding 6 to retain only R1's direct
dead-letter proof and transfer the shared launcher repair to TASK-002. The
validator rejected the unreachable workload-binding serialization branch, rejected
transaction coupling for Card reads/completion and storage sagas, and resolved the
system-tenant question from approved authority without a specification revision.
Decision-complete recommendations are preserved in `findings-validation.md` and
incorporated into the R2 task.

## Wave 1 results

| Independent report | Result | Material scope |
|---|---|---|
| `task-review.md` | FAIL | System retention, lost permission verdicts, Card transaction coupling, production-bound proof, exact verification, source contracts, lifecycle, and authorization ownership |
| `standards-review.md` | FAIL | Transaction, configured-checker, rustdoc, bare-type, lifecycle, and focused-proof rules |
| `domain-review-security-tenancy-durability.md` | FAIL | System tenant publication, Bifrost/Card durability boundaries, trusted-issuer ordering, and service-account RBAC ownership |

Wave 2 independently inspected and deduplicated the complete union; the final
ledger is the nine findings above.

## Prior-finding closure

| Prior finding | Result |
|---|---|
| TASK-001/005 missing permission decisions | PARTIAL: ordinary route outcomes added; R1 findings 2–4 and 9 remain |
| TASK-001/005 non-permission audit | CLOSED |
| TASK-001/005 Oracle duplicate claim | REJECTED by prior independent validation; no durability mechanism added |
| TASK-001/006 focused/cross-surface evidence | PARTIAL: required broad lanes pass; R1 finding 6 remains |
| TASK-001/005/006 SQL capability owners and RLS predicates | CLOSED |
| TASK-005 dynamic Gate audit trait | CLOSED |
| TASK-006 partial publication methods | CLOSED |
| TASK-006 serial tenant publication | CLOSED for runtime behavior; fixed-ceiling proof remains under R1 finding 5 |
| TASK-006 combined replay journey | CLOSED |
| Prior rustdoc/signature/docs findings | PARTIAL: authority pages improved; R1 finding 7 remains |
| Prior lifecycle finding | PARTIAL: TASK-001/005/006 corrected; R1 itself uses an invalid state |
| Obsolete tenant-isolation exemption | CLOSED |

## High-risk boundary review

The independent security/tenancy/durability specialist failed system-owner
publication, Bifrost registration verdict coverage, trusted-issuer ordering, and
Card operation coupling. It passed ordinary-tenant RLS, frozen ranges, monotonic
settlement, deterministic Scribe fencing, static Gate audit, removal of engine-only
audit, and Oracle WAL-first behavior.

## Verification limits

Recorded format, lint, codegen, docs, SQL, Redux/server integration, all six journey
lanes, and boundary checks are credible for the scenarios that completed. They do
not prove the reachable missing audit paths, the production concurrency ceiling,
or the R1-edited test that never became runnable. TASK-002 owns correction and
execution of the five stale Card/CLI/WyrdState launchers.
