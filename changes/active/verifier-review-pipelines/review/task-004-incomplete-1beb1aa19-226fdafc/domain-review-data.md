# Persistent data and concurrency review

Result: **FAIL**. Source-local proposals: DATA-1 through DATA-4.

Subject: base `dfd19aa129830696d68cf8d4a09c144805847345`, committed candidate `1beb1aa1980cb03495dfb0778a353604670ba935`, immutable candidate tree `226fdafc56bacc707b9167d1887be9b72f5e6d53`. All locations below refer to that tree, including unfinished WIP. No prior acceptance carried forward.

## Boundary and coverage

Inspected five migration families; connection/setup/selection/repository SQL and service callers; binding adoption/deletion writers; delivery claims and live worker wiring; Check recording/listing; repository admission and queue-subject persistence. CodeGraph preceded source navigation. Immutable blobs and cumulative SQL diff were read directly. Authorities: root and SQL AGENTS, task-review skill, spec-driven workflow, maintainer style, security posture Postgres tenant binding, replacement TASK-004, public contracts, execution-state supplement and TESTING. Policy/RBAC belongs to the security specialist.

| Obligation | Implementation / verification evidence | Result |
|---|---|---|
| One installation claimant; disable retains claim | Connection partial unique `(host,app_id,installation_id) WHERE claimed`; activate/deactivate SQL; confirm transaction. Unique constraint resolves competing tenants. | PASS by source; concurrent DB test not run |
| Bounded browser state, spent before provider IO | Database-clock expiry, setup lookup FOR UPDATE, callback state advancement committed before exchange, selected-repository composite setup FK. Activation and allowance share caller transaction. | PASS for inspected persistence boundary |
| Honest branch identity/revisions | Upsert retains ID/revision on equal digest; pg_github_repository asserts equality, changed digest revision, branch separation and retirement/reactivation. Concurrent reconciliation is unprotected. | FAIL DATA-1 |
| Admission respects logical control ownership | Delivery leases fence settlement only; independent deliveries and branch writers lack shared logical authority. | FAIL DATA-1 |
| Same frozen review reaches execution and Check | Worker pins/adopts, enqueue pins again; Check uniqueness discards execution identity components. | FAIL DATA-2 |
| Durable PR-to-Check outcome | Only Check insert/list exists; no production delivery or terminal-state writer. Worker settles several required events without processing. | FAIL DATA-1, DATA-3 |
| Tenant parentage and transaction discipline | TenantConn/OperatorPool and forced RLS used; query functions leave commit to caller. New tables/references omit mandatory FKs. | FAIL DATA-4 |
| Bounded/indexed lookup surfaces | Repository/selection pages use composite primary keys; binding/Check API pages are bounded keysets; delivery claims LIMIT 1 and tenant pass LIMIT 64. No extra index finding without query-plan evidence. | PASS for bounded results; query plans unverified |
| Existing Card queue arm | LEFT JOIN retains predicate `r.card_uid IS NULL OR s.card_uid IS NOT NULL`; Card arm still populates Card columns; repository arm decoded separately. Existing queue remains admission owner. | PASS by inspected diff; tests not run |

## DATA-1 — INCORRECT: settlement fences do not own logical review changes

**Obligation.** TASK-004 requires all PR writers to share ordering/claim authority, stale claims to stop before admission/effects, and old work not to overwrite newer work. Existing claim/cancellation owners must be reused.

**Locations.** `components/github/worker.rs:114-138,177-199,296`; `queries/github_deliveries.rs:39-67`; `queries/github_bindings.rs:27-58,111-150`; `components/github/automation.rs:539-555` (server/SQL paths under their respective crates).

**Trace/scenario.** Runtime constructs this worker (`verification/mod.rs:558-565`). A claims and commits a delivery, then blocks in provider/declaration IO. The lease expires and B reclaims or claims newer work. A resumes adoption/reconcile/enqueue without carrying any lease or logical revision fence; only final settlement notices the lost token. Distinct deliveries for one PR independently claim too. Branch deletion is a sibling writer of the same unfenced reconciliation. Inside reconciliation, retire-before-upsert does not lock a branch: concurrent first reconciliations of disjoint suites can both retire an empty branch and then insert their own selections, leaving the union active rather than either suite.

**Consequence/materiality.** Supported multi-replica operation permits stale admission and stale/combined binding revisions. Finish/release fencing cannot undo committed effects. Required close/draft/supersession/rerequest lifecycle is unfinished under this same logical authority boundary; worker default arms settle several required inputs as done. These are grouped by ownership cause, not one issue per event.

**Smallest correction.** Extend existing queue/claim/cancellation owners and reuse `verification/claims.rs::ClaimLoop` for scheduling/drain rather than grow a separate sequential loop. Establish the approved logical PR generation authority before admission and carry it through effect decisions; serialize branch reconciliation at its existing owner across adoption and deletion. A finish-only token guard is insufficient. Preserve engine budgets, unrelated work and the rule against provider IO transactions/global locks. ClaimLoop supplies scheduling, not the missing PR semantics.

**Focused proof.** Concurrent real tenant transactions reconciling disjoint suites must produce one complete suite. Pause A after claim, let B take over/admit newer work, resume A and assert no stale execution or binding change. Reuse that seam for required closure/rerequest behavior.

## DATA-2 — INCORRECT: review identity is reconstructed independently downstream

**Obligation.** Exact pinned configuration/subject/execution/Check association; exact-head mapping and replay in TASK-004 scenario 3.

**Locations.** `components/github/worker.rs:177-180,281-305,322-340`; `components/verification/service.rs:723-735`; `queries/github_checks.rs:24-43`; migration `20261109000400_github_checks.sql:19,32`.

**Producer/consumer evidence.** Worker pins subject A and adopts its base configuration. Dispatch hashes A into the execution key but passes an unpinned public RepositoryTarget to ordinary enqueue. Enqueue calls pin_pull_request again and stores subject B. Worker records A alongside the returned B execution ID. An ordinary PR update between provider reads causes the mismatch. Separately, execution_key includes binding revision/digest, base/head, target and PR number; Check uniqueness uses tenant/repository/head/generation/name, with generation always defaulted to 1. record-on-conflict retains the old row unchanged. Advancing the base/configuration without changing the head queues distinct work but retains the previous execution's Check association. Different PRs reusing a head can collapse too.

**Consequence/materiality.** A Check names commits/configuration different from its execution, or new work has no matching Check. This directly breaks the required team journey.

**Smallest correction.** Keep public execute/enqueue behavior, but hand the worker's already-authorized frozen subject through the existing admission owner rather than pinning again. Persist/use the same approved logical review generation across admission and Check association, including replay/rerequest. Resolve Check upsert identity at that owner; repeated downstream comparisons or another independent key derivation cannot repair the cause. No parallel execution API/store is needed.

**Focused proof.** Change GitHub's PR response between adoption and admission and assert configuration base, stored execution subject and Check subject agree. Re-adopt changed base/configuration with unchanged head; each admitted execution must retain its proper Check. Replay must retain the original association.

## DATA-3 — MISSING: durable Check records have no delivery consumer

**Obligation.** TASK-004 scenarios 3–4 require actual execution Checks, independent retry/reconciliation, and visible delivery failure without rerunning verification.

**Locations/evidence.** `queries/github_checks.rs` entire owner; `components/github/worker.rs:239-268,296-305`; runtime capability wiring in `verification/mod.rs`. Candidate reference search finds Check record calls only in the worker. SQL exports record/list, with no progress/terminal/delivery mutation or claim API. check_run_id remains null and delivery remains pending. Existing execution/Operator delivery owners are present but not connected to these rows.

**Consequence/materiality.** Even successful ordinary execution cannot publish a GitHub Check. Configuration errors also remain local. Terminal mapping, lost-response recovery and delivery inspection are facets of this one unfinished outcome, not independent missing-item findings.

**Smallest correction.** Connect required delivery to existing execution/Operator delivery and retained-result owners, preserving ACK/retry/uncertainty behavior, using the generic library Check methods. Add neither another scheduler/store nor verification reruns. A smaller task may move this complete journey to a named successor with its proof; onboarding cannot claim Checks before that passes.

**Focused proof.** Complete one no-input Verifier and observe its actual exact-head Check payload. Lose a create response, retry/restart, and assert one reconciled Check with unchanged execution ID. Retained-state uncertainty must delay delivery without recomputation.

## DATA-4 — VIOLATION: tenant-qualified parentage is incomplete

**Obligation.** `architecture/wyrd-security-posture.md:758-765`: tenant rows reference platform.tenants; persistent child references use tenant-qualified foreign keys.

**Locations/evidence.** `20261109000300_github_deliveries.sql` has data_tenant_id without tenant parent. `20261109000200_github_repository_bindings.sql` has a repository FK but no composite Card FK for verifier_card_uid. `20261109000400_github_checks.sql` has neither tenant/repository nor execution parent FKs; only binding_id is constrained. New repository columns in `20261109000100_verifier_execution_repository_subject.sql` lack a tenant/repository FK. RLS confines visibility; it does not enforce these provenance links.

**Consequence/materiality.** Database integrity does not enforce declared tenant parentage, violating an explicit repository gate. Service lookups do not replace required constraints. No existing cross-tenant exploit is claimed. This is one schema-integrity correction rather than one finding per column.

**Smallest correction.** Reuse existing tenant, Card, repository and execution composite parent targets for actual durable references. Preserve nullable configuration-Check associations, retained provenance and existing deletion semantics; no new cascading deletion. Batch with lifecycle schema completion.

**Focused proof.** Real PostgreSQL inserts accept same-tenant parent links and reject nonexistent/foreign-tenant IDs, including repository and execution associations. Preserve RLS and connection uniqueness proof.

## Smaller task recommendation and verification limits

Make onboarding one complete outcome: operator configuration, both browser entry points, verified confirmation, selected repository management and explicit disable/disconnect behavior. Keep repository configuration/execution/PR-to-Check as a separately named outcome under the existing approved specification, carrying its lifecycle, revocation and recovery guarantees/proof together. Do not silently assign it to extraction TASK005 or drop guarantees. Reuse queue, ClaimLoop, admission and delivery owners. This recommendation changes no task authority.

No source/task/manifest edits, Cargo, builds, lint, PostgreSQL tests or codegen were run. SQL plans and live concurrency were not measured. Proposed findings are source-proven paths or explicit mandatory constraints, with focused proof stated for validation; no speculative performance or universal exactly-once requirement is added.
