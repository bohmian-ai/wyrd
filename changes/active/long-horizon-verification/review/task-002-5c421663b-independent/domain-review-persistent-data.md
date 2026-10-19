# Persistent data and tenancy review

Result: **PASS** for the reviewed persistent-data boundary. No material proposed findings.

## Immutable subject and authority

- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate: `5c421663b43d7deeca135134d6b07cf49a11b83f`; HEAD remained this candidate at the closing identity check.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-002-scheduled-binding-simplification.md`.
- Acceptance authority: approved revision 13, REQ-012, AC-007, INV-001–005, under the explicit subject instructions. The checked-in specification is revision 14 draft. Its managed-Bifrost-table reset/upgrade addition is not needed to justify this task's PostgreSQL selector-column removal and was not treated as approved behavior.
- Applied AGENTS.md SQL/ownership/testing rules; architecture/agent-rules.md; spec-driven-development and maintainer-style references; Wyrd design's doctrine, Verifier/binding, runtime identity and durable-result boundaries; Wyrd doctrine; Bifrost design's tenant identity and verification-record boundaries; architecture-constraints and analytical-operations-reliability references.
- Used CodeGraph before source navigation. Read the base-to-candidate diff and expanded the SQL/runtime/registration seams independently of the task's completion claims.

## Boundary and source coverage

| Boundary | Source and caller coverage | Assessment |
|---|---|---|
| Persisted binding projection | `queries/verification.rs::NewBinding`, `PROJECT_BINDING_SQL`, `project_bindings`; server `BindingProjector::{project,freeze}`; registration transaction in `components/cards/service.rs::write` | Only the redundant selector argument/column/bind is removed. Subject occurrence, exact subject and Verifier UID, Trigger identity, Operators, schedule and natural key remain. Projection uses the registration caller's TenantConn and never commits. |
| Schema removal | New `20261017000000_drop_verification_binding_inputs.sql`; original binding-table definition and `20261016000000_verifier_executions.sql` | Drops precisely the inputs column introduced by the preceding migration. Does not change identity columns, natural keys, foreign keys, RLS, lease state, existing executions, baselines or result references. Removing the selector's column-owned object check follows the removal of that representation. No CASCADE or broad destructive operation is introduced. |
| Scheduled enqueue and replay | `RESOLVE_BINDING_SQL`, `INSERT_SCHEDULED_SQL`, `EXISTING_OCCURRENCE_SQL`, `VerifierExecutionQueue::{enqueue_scheduled,claim_due,schedule_due}`; scheduler and runtime callers | Exact Verifier UID/version, subject UID, window, owner, Trigger, Operators and accepted authority are still frozen into the execution. Duplicate occurrence handling remains the existing unique occurrence plus AlreadyEnqueued result. Due cursor and enqueue stay in one caller-owned transaction. |
| Claim, retry and recovery | `CLAIM_SQL`, `ClaimedRow::into_claimed`, `ExecutionInput`; `claim`, `retry`, lease renewal/expiry/settlement SQL; server `ClaimLoop::claim`, `VerifierRunner::{claim,resolve,dispatch}` | The removed binding lookup no longer supplies an independent authority. Scheduled input now derives from stored window columns. Claim/reclaim updates attempts and lease fields, not subject/window/Verifier identity. Exact UID lookup and cache keyed by tenant/UID supply the same immutable Verifier declaration on each attempt. Failed resolution rolls back the claim; the owning loop commits before processing. |
| Tenant boundary | Forced RLS policies from binding and execution migrations, TenantConn query signatures, OperatorPool runnable-tenant discovery; `Reader::{authority,read}` | No raw pool or widened tenant path is added. Statement subqueries inherit tenant RLS. Cross-tenant discovery remains operator-scoped; processing opens the selected tenant connection. Bifrost reads resolve a tenant-qualified observations table under SYSTEM authority serving the accepted execution authority. |
| Subject/window data selection | `DriftEngine::{checks,score}`, `ObservationWindow::{new,rows,rows_in}` | Every pinned drift check is enumerated; each query retains exact stored subject UID and half-open stored window. Baselines remain selected by exact Verifier UID and requirement ID. Slot-map removal does not broaden a query to other subjects or tenants. |
| Settlement and durable results | Queue `complete`, `COMPLETE_SQL`, `INSERT_DISPATCH_SQL`; runner/result flow and changed caller fixtures | Existing lease-token/result fencing and atomic failed-result dispatch creation remain. Submitted input maps/route overrides are unchanged. No outbox, acknowledgment, result-storage, delivery or audit durability mechanism changes. |

The existing owners already implement all required identity, transaction and tenancy behavior. The candidate extends them by deleting the duplicate selectors; it does not introduce a second durable workflow.

## Obligation matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| No binding-owned selector survives as scheduled authority | Binding contract/NewBinding/SQL column removal; claim removes schedule_inputs; Scheduled contains window only | Updated `scheduled_and_submitted_executions_freeze_their_inputs`; generated contract and caller fixture changes | PASS |
| Claim/retry/recovery preserve exact subject, Verifier and window | Execution row freezes identities; claim/retry update only lifecycle columns; runner resolves exact UID; checks enumerate that spec | `concurrent_schedulers_create_one_run_per_occurrence`, `reclaimed_lease_fences_the_stale_token`, `retries_back_off_then_exhaust_to_errored`, runtime scheduler restart/cancellation tests | PASS |
| Invalid authoring leaves no persisted Card/binding | Contract decoding precedes persistence; composite registration parses and writes under one rollback-capable transaction | New Rust SDK/raw-HTTP rejection test asserts 400 and subsequent Card 404; existing no-registration-write integration cases | PASS |
| Projection remains atomic and tenant isolated | TenantConn + forced RLS; caller-owned commit; no new tenant filters or cross-tenant reads | `projection_is_transactional_and_tenant_isolated`, keyed-submission tenant isolation coverage, changed SQL integration suite | PASS |
| Every declared requirement uses original subject/window, including missing data | DriftEngine loops all drift_checks; score retains ObservationWindow and existing unscored semantics | New two-requirement scenarios in Rust/Python/TS; existing empty-window and baseline regression branches | PASS |
| Existing submitted execution and Operator durability retained | Submitted input branch unchanged apart from removal of unrelated tuple member; lease/settlement/outbox owners unchanged | Scheduled/submitted frozen-input tests, stale-token/idempotent completion tests, runtime/Operator regression sources | PASS |
| No group lifecycle/outbox/admission work enters this task | Diff restricted to scheduled contract/removal, decoding error context, associated consumers and proof | Complete changed-path audit | PASS |

## Verification assessment and limits

The task evidence claims 32 SQL integration tests, 93 server integration tests, 10 shared runtime tests and the focused three-SDK scheduled scenarios passed, along with tenant-isolation and dependency checks. These are supplied claims, not independently rerun results in this report.

This reviewer performed source/diff/authority inspection only and did not run Postgres, migrations, tests, journeys, or a broad gate. Runtime verification is coordinated by the primary reviewer. The new two-requirement journey cases do not themselves force a retry/reclaim; existing supporting tests prove scheduled window preservation and lease fencing, while source inspection establishes preservation of the exact Verifier UID and complete declaration-derived requirement set. No new requirement to freeze live observation contents was inferred: the task requires frozen identities and the existing scoring behavior.

This is a bounded data/tenancy acceptance judgment, not independent proof of deployment upgrades, live mixed-version rollout, backup restoration, or unrelated Bifrost behavior. The column removal intentionally cannot preserve the removed selector representation, as REQ-012 requires.

## Findings

None. No bounded persistent-data or tenancy correction is proposed.

Skipped runtime reruns and deployment-upgrade qualification; verification claims require the primary review's coordinated evidence.
