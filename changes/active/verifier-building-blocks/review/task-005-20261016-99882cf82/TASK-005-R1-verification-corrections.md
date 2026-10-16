---
id: TASK-005-R1
kind: remediation
status: ready
spec: SPEC-verifier-building-blocks
spec_revision: 9
requirements: [REQ-001, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-017, INV-002, INV-003, INV-004, INV-006, INV-007, INV-008, AC-005, AC-006, AC-007, AC-009]
depends_on: [TASK-005]
parent_task: TASK-005
remediates: [FIND-TASK-005-1, FIND-TASK-005-2, FIND-TASK-005-3, FIND-TASK-005-4, FIND-TASK-005-5, FIND-TASK-005-6, FIND-TASK-005-7, FIND-TASK-005-8, FIND-TASK-005-9]
---

# TASK-005-R1 — Correct verification admission, evidence and recovery

Route directly to `$wyrd-implement`. This is one bounded remediation of the
existing TASK-005 candidate, not a replacement implementation plan. It contains
all nine retained IDs; FIND-4 and FIND-6 share one correction boundary.

## Authority and immutable subject

- Approved spec: `changes/active/verifier-building-blocks/spec.md`, revision 9,
  `status: approved`; especially REQ-010, REQ-012 and revision history entry 9.
- Original task: `changes/active/verifier-building-blocks/tasks/TASK-005-server-integration.md`.
- Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Reviewed branch: `change/verifier-building-blocks-task-005`.
- Base exclusive: `f2a726c90edbf8956c7b91c843379175ee20eac0`.
- Reviewed candidate: `99882cf824e70a601033aba4cfa5c5b27a8dfc7c`.
- Artifact completion HEAD: `d88387d79fad063be856b14947b4385b78f422e7`.
  Its sole change after the candidate is the original task's external-example
  evidence wording; production and test source remain identical. The approved
  r9 spec is supplied in the working tree. Neither changes the reviewed source
  identity. Preserve the original [task snapshot](task-snapshot.md).
- Review: [verdict.md](verdict.md), original independent ledger
  [findings-validation.md](findings-validation.md),
  [followup-review.md](followup-review.md), and [cumulative.diff](cumulative.diff).
  The verdict's r9 disposition supersedes only the ledger's r8 approval stop for
  FIND-6. The other eight validated diagnoses remain authoritative.
- Apply `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
  `architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`,
  and the spec-driven-development, maintainer-style and testing references.

The owner explicitly states nothing has shipped: no data/catalog upgrades,
legacy pending-dispatch migration, compatibility machinery or migration proof.
DATA-001 and BEH-04 stay rejected. Revision 9 approves the public local evidence
PUT; do not reopen, remove or replace that decision.

## Outcome and ownership

Preserve the implemented direct/queued/scheduled verification model while
restoring signed subject admission, bounded resource deferral, explicit
scheduled inputs, operation-level evidence staging and per-attempt result
identity. Remove only the validated unused trace-wait API, and batch mandatory
Rust and normative documentation corrections.

Reuse these existing owners; extend their missing behavior rather than adding
parallel orchestration:

| Boundary | Existing owner and affected consumers |
|---|---|
| Manual admission | `wyrd-server::components::verification::service::VerificationControl`; HTTP routes, MCP verification and shared-client submit converge here; signed `Principal::card_attribution` / `CardRefScope::uids`, existing audit stage |
| Hosted resource admission | `verification::hosted::{sandbox::Sandbox, workspace::Workspaces}`, code action/check adapters and `tools::Invocation`; Vala block engine, existing runner `EngineOutcome::Deferred`, claim loop and SQL queue release |
| Scheduled drift | `verification::drift::DriftEngine`, runner's claimed `ExecutionInput::Scheduled.slots`, existing Vala `DriftChecks` |
| File staging | `VerificationControl::{evidence, upload_local_evidence}`, runtime `PermissionSet::covers_operation`, existing audit stage, `wyrd-storage` evidence plans/local writes; all SDKs use shared `RunEvidence::pin` |
| Result selection | `verification::results::ResultPayloadBuilder`, queued runner/direct staging, existing Scribe payload test support; SQL completion's selected result pointer, frozen Operator context, status/result and feature readers |
| Queue deletion | `wyrd-sql::queries::verifier_executions::VerifierExecutionQueue` and its dedicated integration test |
| Documentation | Owning Rust items; existing Wyrd/Bifrost/security/telemetry authorities listed below |

## Diagnoses and selected corrections

### FIND-TASK-005-1 — Restore signed subject admission before staging

**Obligation:** INV-003/004 and the original manual accepted-authority boundary.
**Evidence:** `crates/wyrd/wyrd-server/src/components/verification/service.rs:553-615`
checks the selected Verifier at line 569, then captures accepted authority and
stages the request's subject without checking its signed scope. SQL
`crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:1105-1172::accept`
resolves target/readiness/replay; `verification/submissions.rs::write` commits
acceptance. `verification/runner.rs:383-415` restores authority and attributes
results to `run.card_uid`. A JSON/assert-only execution needs no later protected
gateway or tool read, so a bound caller with exact V invocation permission can
produce retained judgments for unrelated same-tenant S. Tenant isolation and
green happy-path tests do not enforce this narrower signed attribution scope.

**Correction:** enforce the subject decision once in shared manual
`VerificationControl::enqueue`, before input staging. Use the effective
principal's existing `CardAttribution::Scoped` and signed `scope.uids()`
membership, following `components/gateway/invocation.rs:758-791::attribute`.
`wyrd-auth/src/card_scope.rs:53-109` resolves those UIDs at issuance;
`wyrd-spec/src/reference.rs:337-374,488-542` preserves them through signed text
serialization, and `wyrd-runtime/src/request_context.rs:85-105` preserves them
in accepted authority. Reuse the existing RBAC denial and nonblocking Scribe
audit stage for an accurate denied scope decision. This closes the producer
before HTTP/MCP/SDK work can reach the outbox; do not add downstream guards.

**Preserve/non-goals:** keep AnyRegistered callers, exact V invocation checks,
effective delegated subject narrowing, accepted authority and actor history.
Actor scope must not widen subject scope. Keep the pre-existing direct scope
exception, scheduled binding-owner authority, and asynchronous availability,
readiness, replay/conflict decisions. No registry lookup, fabricated CardRef,
new scope resolver, request Postgres write or asynchronous permission check.

**Acceptance:** a scoped caller can enqueue its mapped subject; an unrelated
active same-tenant subject receives synchronous RBAC denial, a denied audit,
and no execution input or queue work. An unbound caller still succeeds.
Delegation with a broader actor graph cannot widen the effective subject's
scope. Exercise HTTP and MCP through their existing shared control path.
Focused proof: P1 and the route/MCP slice in P8.

### FIND-TASK-005-2 — Keep resource pressure in the queue

**Obligation:** deployment limits, INV-007, TASK-005 Scenario 6 and the design's
resource-refusal contract. **Evidence:** `verification/hosted/sandbox.rs:153-224::exec`
returns ordinary `BlockError::Retryable` on pool/waiter saturation or checkout
deadline. `crates/vala/vala-eval/src/blocks/engine.rs:318-369::attempt` spends
authored retries and makes the action fail; `:374-414::judge` turns code/Agent
errors into inconclusive. `verification/runner.rs:410-415` receives a successful
engine outcome and completes it. `hosted/tools.rs:280-303::Invocation::run`
stringifies the same refusal into `ToolError` for Agent `code.run`.
`hosted/workspace.rs:60-100::open` waits on a semaphore without bounded waiter
admission, reached through `HostedTools::open`. The claim loop can accumulate
these waits until execution deadlines. Transient process contention therefore
consumes work as failed/inconclusive/timed-out instead of allowing it to resume.
Scripted Deferred tests and standalone pool-cap tests do not exercise this chain.

**Correction:** keep resource-admission refusal distinguishable at the existing
Sandbox and Workspaces owners and through every hosted consumer: code action,
code check and enabled Agent `code.run`. Carry it through the Vala engine to
existing queued `EngineOutcome::Deferred` / `Transition::Defer`, token-fenced
`VerifierExecutionQueue::release` with refunded attempt, and ClaimLoop pressure
handling. Bound workspace waiting at its existing owner using the approved
process admission limits. No new scheduler, queue, configuration policy or
whole-execution Python worker reservation. This is one admission invariant;
fix category loss at its producer/adapters instead of classifying terminal
results afterward.

**Preserve/non-goals:** genuine provider/retryable/check infrastructure failures
retain authored retry and check-local semantics; do not promote all Retryable
errors to queue deferral. Resource refusal must not spend authored retries or
Python execution budgets. Keep direct execution bounded by its existing
deadline and its genuine check-failure behavior. Preserve cancellation, lease
renewal/fencing, shutdown/drain, accepted authority and non-Python/sibling
availability. No synthetic host load, platform-specific isolation or new permit
ceiling over whole executions.

**Acceptance:** with the real configured tiny pool held by controlled work, a
second hosted queued code check defers with no terminal result or consumed
attempt, then completes the same execution with its actual code verdict after
capacity release. Cover refusal propagation for code action and Agent
`code.run`, bounded workspace admission and cancelled waits, direct bounded
behavior and continued non-Python progress. A scripted Deferred response alone
cannot close this. Focused proof: P2, P7 and relevant engine slice P8.

### FIND-TASK-005-3 — Honor omitted optional scheduled slots

**Obligation:** REQ-001/014, INV-006, AC-007. **Evidence:**
`crates/wyrd-spec/src/card/verifier/validate.rs:878-911::validate_schedule`
permits an optional samples slot to be omitted. SQL
`ClaimedRow::into_claimed` at `verifier_executions.rs:1981-2000` freezes the
binding mappings in `ExecutionInput::Scheduled.slots`, but
`verification/runner.rs:363-369` discards them.
`verification/drift.rs:686-702::checks` scores every drift check's configured
metric/features regardless of its `on` slot. Vala
`blocks/engine.rs:544::check_bindings` bypasses ordinary slot binding for drift;
`blocks/drift.rs:76-90::judge` prefers a pre-scored report over missing-samples
handling. Retained rows for an omitted optional slot can thus create a real
pass/fail verdict; existing all-mapped proof misses that reachable case.

**Correction:** use the frozen explicit slot mapping at existing DriftEngine
report production. Produce a scored report only for a drift check whose
specified `on` slot was supplied. Omission leaves no pre-scored report, allowing
existing DriftChecks missing-evidence semantics to produce inconclusive. Carry
the claimed mapping from its existing owner; do not reconstruct it from data
rows or add consumer guards after scoring.

**Preserve/non-goals:** required-slot registration, legitimately optional slots,
exact baseline identity/readiness, windows, authorization, statistical
algorithms and mapped-slot results remain intact. No inferred source mapping,
new loader, synthetic success or requirement that optional slots become required.

**Acceptance:** with a required mapped slot, an optional omitted slot, and
retained rows for both, the omitted requirement is inconclusive, has no
fabricated scored detail and cannot alone dispatch a failed Operator. Explicitly
mapping that slot restores its real existing judgment. Use the existing runtime
and drift fixtures, not a replacement scoring harness. Focused proof: P3.

### FIND-TASK-005-4 and FIND-TASK-005-6 — Operation-level evidence admission under r9

**Obligation:** REQ-010/011/012, INV-002/003 and approved r9 local upload rules.
**Evidence:** shared `crates/shared/wyrd-client/src/verify/mod.rs::RunEvidence::pin`
requests plans/uploads before execute or enqueue.
`components/verification/service.rs:323-329::evidence` and
`:399-406::upload_local_evidence` call `Permission::verifier_run_any()`;
`crates/shared/wyrd-runtime/src/permission.rs:511-516` constructs scope All.
An exact V grant cannot cover All, even when the file already exists, because
this check precedes lookup. All three SDKs encounter the same failure. Existing
MCP coarse admission already accepts exact grants via
`mcp/verification.rs:133-142::may_run_verifiers` and
`PermissionSet::covers_operation` (`permission.rs:950-964`).

Revision 9 closes FIND-6's original public-API approval objection, but does not
make this authorization mismatch correct. Independent supplemental validation
confirmed FIND-6 as REVISED, consolidated with FIND-4, with no extra OpenAPI
mismatch: `components/verification/routes.rs:38-49` conditionally mounts and
publishes the local route through the same `routes!` registration;
`:162-184` describes the binary PUT and refusals. `http/openapi.rs:18-52`
inherits the global token scheme, with no OAuth scope list to invent.
`service.rs:415-420` passes caller tenant; storage
`crates/wyrd/wyrd-storage/src/service.rs:546-570` checks digest then writes the
tenant evidence key; `local.rs:57-85` writes atomically. Valid same-digest writes
replace identical bytes without metadata. Those correct behaviors stay in place.

**Correction:** replace All-object admission on both generic evidence operations
with the existing operation-level `covers_operation(Resource::Verifier,
Action::Run)` mechanism, preserving allowed and denied decisions through the
existing nonblocking audit stage. This preparatory boundary selects no Verifier
and cannot execute one. Keep exact selected-UID authorization on execute/enqueue.
One correction in VerificationControl closes both IDs for HTTP, MCP plans and
shared SDK file staging; no new evidence request field, client permission logic,
All grant escalation or separate upload authorization policy.

**Preserve/non-goals:** retain the approved local-only published PUT, caller-derived
tenant, bounded body, digest validation before visibility, idempotence, no
metadata row, remote direct single-PUT plans, normal Artifact authorization and
Card/result read checks. Align the existing `architecture/wyrd-design.md`
Verifier route inventory and file-transfer wording with r9's approved local
binary transfer; this is documentation of the approved route, not a new route
or general relaxation of invocation authority. Do not add OpenAPI scopes or
change the global authentication scheme when no mismatch exists.

**Acceptance:** exact-V-only run permission plus ordinary required read grants
can plan and upload a missing local File, then use it for verify and submit.
Invoking W is still refused. A caller with no Verifier run grant is refused and
audited for both staging operations. Existing tenant separation, size/digest
refusals, same-digest retry and no metadata row hold. Remote plan admission
accepts an exact grant through existing credential-free backend fixtures; remote
routers do not mount/publish local PUT. Served local OpenAPI retains the
approved path, binary body, 204, authentication and refusal descriptions.
Focused proof: P4; use existing backend/route fixtures and no live cloud.

### FIND-TASK-005-5 — Select the winning attempt's result

**Obligation:** REQ-012/013/015 and TASK-005 recovery Scenario 3.
**Evidence:** `verification/results.rs:209-216::ResultPayloadBuilder::result_id`
aliases result ID to execution UUID. `runner.rs:736-790::result` builds one
payload; `:438-509::settle` stages it after fenced SQL complete but before commit.
Its documented failure path permits result A to survive failed commit and lease
loss. `verifier_executions.rs:221-259::CLAIM_SQL` reclaims with a new token;
executed attempt B may return a different allowed stochastic verdict, yet shares
A's result ID. Content-derived Scribe batch IDs differ, so deduplication cannot
remove A. `components/verification/service.rs:251-296::read_result` uses
`WHERE result_id ... LIMIT 1`; the completed B handle can read A. Feature joins
can mix both attempts. Happy-path result projection and publication-retry proof
do not cover this current-install recovery failure.

**Correction:** restore the base's per-produced-payload `VerificationResultId`,
minted once and retained throughout summary/features/payload construction and
settlement retries. A newly executed attempt gets a new result ID. Reuse the
existing builder, queue completion's selected result pointer, frozen Operator
context and status lookup. Preserve separate `execution_id` correlation. Cover
builder siblings: queued result production, direct `stage_direct`, Scribe test
support and payload tests; enqueue uses the input-only builder path. Fix identity
at production, not readers guessing which row won.

**Preserve/non-goals:** transport/settlement retry of one payload keeps its ID
and identical sealed bytes; keep existing publication order, precommit staging,
fencing, loss guarantees and independent Operator delivery. Do not generate a
fresh ID on each accessor, sort result reads to guess the winner, add a store or
durable outbox, move staging across commit or redesign queue/schema/recovery.

**Acceptance:** force A's settlement rollback after its result stages, reclaim
under a new lease and complete B with a distinguishable verdict. Retain both
published payloads and prove distinct A/B result IDs, one execution correlation,
B's queue-selected ID, B-only real status/read outcome and matching frozen
Operator context. Feature reads selected by B must not join A's features.
Retries of A and B individually retain their original identity/bytes; direct
results remain valid and internally consistent. Focused proof: P5 plus P6.

### FIND-TASK-005-7 — Complete substantive changed-item rustdoc

**Obligation:** AGENTS.md §16 and agent-rules' mandatory rustdoc gate.
**Exact omissions:**

- `crates/shared/wyrd-runtime/src/outbox.rs:107::Debug::fmt` lacks method docs
  and its formatter-error contract; an impl comment does not cover it.
- `crates/wyrd/wyrd-server/src/verification/submissions.rs:59-61`: associated
  `Item`, `Error`, `NAME` lack item docs.
- `crates/vala/vala-eval/tests/integration/orchestrator_parity.rs:52::drive`;
  `crates/vala/vala-eval/src/scenario.rs:452,494` changed scenario tests;
  `src/tasks/assertion.rs:297`, `src/tasks/agent.rs:148`, `src/tasks/trace.rs:279`
  changed executor factories; `src/executor.rs:939` changed full-spec test
  lack the mandatory substantive item documentation.
- New `vala-eval/src/scenario.rs:382::response_task`,
  `wyrd-server/src/verification/engines.rs:102::tests::requirement`, and
  `wyrd-sql/src/queries/verifier_executions.rs:2197::tests::request` use
  fixture `expect` without the required panic contract. The last also
  incorrectly describes a fixed subject although it generates a fresh UID.

**Consequence/proof gap:** maintainers lack required operational, fallible and
panic contracts; green builds do not prove private-item docs. This is the
explicit blocking documentation exception, not a discretionary style round.

**Correction:** document these items in their owning modules, explaining their
role and actual error/panic conditions; correct the fresh-subject description.
Include substantive docs for any materially changed items in this remediation.
Preserve bodies, assertions and runtime behavior. No placeholder prose,
unrelated helper rewrite or new documentation checker.

**Acceptance/proof:** inspect every cited complete item and its docs, including
`# Errors`, actual panic invariants and cancellation/partial progress where
applicable. Batch with behavioral repairs. Static source review plus P8 format,
lints and owning rustdoc builds; no behavioral test solely for documentation.

### FIND-TASK-005-8 — Align normative descriptions with current behavior

**Obligation:** REQ-015/017, AC-009 and public-contract consistency.
**Evidence:** `architecture/bifrost-design.md:231-255` promises a top-level
`drift_report` Struct absent from the complete fields in
`crates/vala/vala-bifrost-redux/src/tables/verification/results.rs:51-69`;
detail actually lives in the outcome Variant and requirement feature projection.
Bifrost design `:1006-1010` promises `wyrd_ingested_at` partitioning for execution
inputs; `tables/verification/execution_inputs.rs:62-67` uses approved
`recorded_at`. `architecture/references/domain/telemetry-observations.md:65-77`
retains the removed `kind` metric label; actual
`verification/telemetry.rs:154-172,229-269` and `app/metrics.rs:71-96` do not emit
it. `architecture/wyrd-security-posture.md:125-130` retains observation-triggered
activation; `:208-209` retains old paired Drift/Eval input-read authority.
These normative claims mislead SQL/metrics/security consumers despite green
runtime tests; they are not merely historical descriptions.

**Correction:** update those existing prose owners to the current approved
schema and detail projection, `recorded_at` layout, emitted bounded metric
dimensions, schedule-or-submit activation with ACK-only queue handoff, and
caller-authorized trace reads versus restricted scheduled drift reads. Use
actual table field/layout, telemetry emitter and boot/Gate/ACK owners as the
source map. Include r9 route prose alignment from FIND-6 in the same edit.

**Preserve/non-goals:** retain nullable-Struct rules for tables that actually
still contain them, accepted-principal/SYSTEM attribution, scheduled drift
read restrictions and loss guarantees. Do not restore retired columns, kinds,
activation or metric labels, add compatibility, change timestamps or algorithms,
or invent migration proof.

**Acceptance/proof:** compare each corrected assertion against the complete
owning source and remove contradictory retired wording for these named
contracts. Run P8's owning documentation proof. This is static documentation
work; no new journey or executable test is needed.

### FIND-TASK-005-9 — Delete the unused trace-wait lifecycle

**Obligation:** INV-008 complete replacement and deletion of unearned machinery.
**Evidence:** `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:375::AWAIT_TRACE_SQL`,
`:929::TraceWaitOutcome`, `:1644::await_trace` implement a trace-specific
refund/deadline/requeue path; its complete method calls terminate on expiry.
Its only caller/import is the dedicated
`tests/integration/pg_verifier_executions.rs:1834-1913` test. Real direct/queued
execution uses HostedBlocks → CallerTraces → VerificationEngine::trace once;
read failures become inconclusive. No runner trace-wait transition exists;
normal release/retry/lease owners already cover active queue behavior.
The dedicated self-test cannot earn unused production API and lifecycle cost.

**Correction:** delete only that constant, enum, method and dedicated test/import
at the existing queue owner. No replacement implementation or production wiring.
Preserve real trace readiness behavior, normal release/backpressure/retries,
leases/fencing and their active tests. This is deliberate source deletion, not
a dormant runtime bug or permission to remove genuine failing coverage.

**Acceptance/proof:** repository search finds no remaining source references to
the three deleted symbols; compile the SQL library/integration target and run
P6's active fencing and refunded-release checks. Do not add a replacement test
for the removed self-only API.

## Focused proof and execution discipline

Implement executable cases in this order: signed enqueue admission (P1),
evidence admission (P4), real capacity propagation (P2), optional scheduled
sources (P3), then attempt/result recovery (P5). Use one scenario's RED → GREEN
→ REFACTOR at a time. Record actual focused command, expected pre-fix failure,
post-fix result and relevant regression evidence. New regression test names
remain implementation-owned: after choosing them, record and run each exact
`mise exec -- cargo nextest run --locked -p <owner> --lib|--test <target>` command
with `-E 'test(=<fully-qualified-name>)'`; never count an empty selection.
The module selections below are the bounded lanes covering these new cases,
not substitutes for their recorded exact RED/GREEN commands.

If a test fails, rerun it with `WYRD_LOG=info,wyrd_server::verification=debug`
(or its owning tracing target) and inspect the trace before diagnosing; repair
an absent tracing subscriber at the owning harness. Do not change timing,
retries, assertions or skips simply to clear a failure. Use existing fixtures,
real server/SQL/worker seams for cross-boundary proof and controlled held work,
not synthetic host pressure. Build the real Monty worker for P2 using the
existing `mise run build:monty-worker` task.

P1 — FIND-1: extend existing `pg_verification_routes` for scope rejection,
absence of staged inputs/queue work, retained audit, scoped/unbound success and
delegated narrowing; cover MCP convergence in its existing unit/control slice.
Run the focused route module including those additions:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E 'test(/^pg_verification_routes::/)'"
```

P2 — FIND-2: use the existing hosted/runtime fixtures with the real Sandbox
instead of treating EngineScript's Deferred as proof. Cover code action/check,
Agent code.run, workspace bounds/cancellation and direct/non-Python behavior
at their owning seams. Run exact new regressions, then the existing runtime
module covering deferral, shutdown and recovery:

```bash
mise run build:monty-worker
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(/^pg_verification_runtime::/)'"
```

P3 — FIND-3: add the omitted-optional-slot versus explicitly-mapped comparison
to the existing real runtime/drift integration boundary. Use P2's runtime module
once after both corrections; require the exact newly added regression command
in the implementation record. Retained observations must exist for both slots;
a pure report mock cannot prove that omitted mappings prevent actual scoring.

P4 — FIND-4/6: extend P1's route module for exact grants, no-grant audited
refusal, verify/submit with missing local bytes and tenant/digest/size/idempotence
rules. Use existing backend fixtures for remote plan admission and local-route
absence. Preserve the existing storage regression and served OpenAPI proof:

```bash
mise exec -- cargo nextest run --locked -p wyrd-storage --lib -E 'test(=service::tests::local_evidence_plans_the_digest_route_and_writes_only_matching_bytes)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_openapi_contract::verification_contract_publishes_five_typed_operations_and_the_local_upload)'"
```

P5 — FIND-5: extend the existing PublicationFault/runtime fixture to force
staged-A/rolled-back-settlement/reclaimed-B and read the selected real result,
features and frozen dispatch context. Use P2's runtime module after the change
and exact new fault-path command; retain summary/feature mapping proof:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::results::tests::outcome_writes_requirement_features_then_summary)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::results::tests::outcome_without_drift_writes_only_the_summary)'
```

P6 — FIND-2/5/9: retain actual queue fencing, winning-pointer and refunded
release regression checks after deleting only the unused trace-wait self-test.
SQL changes need no new deployment migration:

```bash
scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E 'test(=pg_verifier_executions::reclaimed_lease_fences_the_stale_token)'
scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E 'test(=pg_verifier_executions::release_requeues_without_consuming_an_attempt)'
mise exec -- cargo check --locked -p wyrd-sql --lib --test integration
rg -n '\b(AWAIT_TRACE_SQL|TraceWaitOutcome|await_trace)\b' crates sdks
```

The last command must find no references (normal `rg` exit 1 means none).
Do not remove active coverage to make these commands green.

P7 — FIND-2: run only the existing real worker boundary lane in addition to
new capacity propagation regressions; it builds its own worker prerequisite:

```bash
mise run test:verification:sandbox
```

P8 — FIND-1–9: run format, required lints and dependency/tenant checks for this
Rust write set. Cover affected shared control, MCP, Vala block and result
modules with focused unit selections; these are not full journey suites:

```bash
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::/) and not test(/::pg_tests::/)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=mcp::tests::every_tool_schema_is_accepted_by_common_tool_callers)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(/^blocks::/)'
mise exec -- cargo doc --locked --no-deps --all-features -p wyrd-runtime -p wyrd-server -p wyrd-sql -p vala-eval
```

Inspect private-item rustdoc manually against the FIND-7 inventory: cargo doc
does not prove that obligation. FIND-8's changes are architecture Markdown,
not docsite source. Compare corrected prose against actual owners; there is no
new prose checker to add. `mise run check:docs` builds only wyrd-spec/auth docs
and does not prove these changed private-item or normative prose obligations.
Run `mise run docs:check` only if closure actually changes files under `docs/`;
no docsite edits are required by the selected corrections. Run
`mise run codegen:check` if the chosen minimal fix changes contract/generator
source or generated outputs; no wire-shape change is required. Served OpenAPI
is runtime-composed, so P4's real served-document test remains required even
when codegen runs.

Run each overlapping module lane once after its related fixes pass exact
RED/GREEN checks. Do not require `mise run gate`, full SDK/user-journey suites,
full Rust/SQL/family aggregates, live provider/cloud checks, benchmarks or
external example rebuilds. Final integrated change review owns broad journey
qualification. This bounded task does not waive that later completion gate.

## Completion evidence and stop conditions

Record each stable finding's changed owners, exact focused test commands/results
or static documentation/deletion proof, preserved adjacent behavior and any
remaining limits. Subsequent task review must inspect the full original
base-to-new-candidate range with this task and the prior findings, not only R1.
Keep the nine IDs, including FIND-6's revised shared authorization residual.

No new public/security/resource-ownership/persistent-data decision is left to
implementation here. If source changes invalidate the cited correction
boundaries, report that evidence rather than inventing a new API, queue,
authority, recovery service or loss guarantee. Do not reopen r9 approval,
DATA-001 or BEH-04. Do not modify the approved spec or original task status as
part of this remediation's implementation authority.

## Implementation evidence

Prefix: `PG=scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked ..."`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-1 scoped principal cannot enqueue for an unsigned subject; refusal before staging, audited | `components/verification/service.rs::authorize_run` (subject arm on `CardAttribution::Scoped`, `scope.uids()`) | `pg_verification_routes::enqueue_admits_only_signed_scope_subjects` RED (202 for an unscoped subject) → GREEN; routes module 9/9 | PASS |
| FIND-2 capacity refusal is `Busy` at every hosted consumer and a queued execution defers without spending an attempt | `vala-eval/src/blocks.rs::BlockError::Busy`; `blocks/engine.rs::EngineError::Busy` (no retry, no judge); `blocks/tools.rs::Refusal` + `ToolSession::halt_on_refusal`; `blocks/agent.rs::invoke`; `hosted/sandbox.rs::busy`; `hosted/workspace.rs::open` (`try_acquire_owned`); `hosted/tools.rs`; `verification/runner.rs::engine_failure` → `Deferred(RUNTIME_BUSY)`; `service.rs` direct execute → 503 | RED with HEAD `sandbox.rs`/`workspace.rs`: both new tests failed (`Err(Retryable { "the Python sandbox is at capacity" })`; no deferral). GREEN: `pg_hosted_artifacts::capacity_refusal_is_busy_at_every_hosted_consumer`, `pg_hosted_artifacts::busy_queued_execution_is_deferred_until_capacity_frees` (log line + `attempts == 1`); `blocks::engine::tests::capacity_refusal_stops_the_execution_without_judging`; `workspace::pg_tests::cancelled_open_returns_its_admission`; `mise run test:verification:sandbox` 12/12; runtime module 38/38 | PASS |
| FIND-3 omitted optional slot is not scored from retained rows | `verification/drift.rs::checks` filters by `slots`; `runner.rs` passes the frozen `slots` | Journey (the existing drift/observation fixture lives in wyrd-testing): `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::omitted_optional_slot_is_not_scored_from_retained_rows)'"` RED ("must be inconclusive, saw failed") → GREEN | PASS |
| FIND-4/6 r9 evidence admission: exact grant, audited no-grant refusal, local route, served contract | `service.rs::authorize_evidence` (stages Allowed/Denied before refusing); r9 prose in `spec.md`, architecture docs | `pg_verification_routes::exact_verifier_grant_stages_local_evidence` RED (403 `lacks Verifier/Run`) → GREEN; `wyrd-storage service::tests::local_evidence_plans_the_digest_route_and_writes_only_matching_bytes`; `pg_openapi_contract::verification_contract_publishes_five_typed_operations_and_the_local_upload` | PASS |
| FIND-5 reclaimed attempt selects its own result | `verification/results.rs` | `pg_verification_runtime::reclaimed_attempt_after_a_failed_settlement_selects_its_own_result` RED (shared result id) → GREEN; `results::tests::outcome_writes_requirement_features_then_summary`, `results::tests::outcome_without_drift_writes_only_the_summary` | PASS |
| FIND-7 substantive rustdoc on cited items | `outbox.rs` `Debug::fmt`; `submissions.rs` `Item`/`Error`/`NAME`; vala-eval `orchestrator_parity.rs::drive`, `scenario.rs` (`response_task`, two tests), `tasks/{assertion,agent,trace}.rs::executors`, `executor.rs` full-spec test; `engines.rs::tests::requirement`; `verifier_executions.rs::tests::request` (fresh subject corrected); `authority.rs` module and `BackgroundAuthority` docs corrected to scheduled-only SYSTEM reads | Static review; `cargo doc --no-deps --all-features -p wyrd-runtime -p wyrd-server -p wyrd-sql -p vala-eval` exit 0, no warning on a line this diff introduced (one introduced redundant-link warning fixed) | PASS |
| FIND-8 normative prose matches owners | `architecture/bifrost-design.md`, `wyrd-design.md`, `wyrd-security-posture.md`, `references/domain/telemetry-observations.md` | Static comparison against table/telemetry/authority owners | PASS |
| FIND-9 trace-wait lifecycle deleted | `wyrd-sql/src/queries/verifier_executions.rs`, `tests/integration/pg_verifier_executions.rs` | `rg -n '\b(AWAIT_TRACE_SQL\|TraceWaitOutcome\|await_trace)\b' crates sdks` exit 1; `cargo check -p wyrd-sql --lib --test integration`; `pg_verifier_executions::reclaimed_lease_fences_the_stale_token`, `pg_verifier_executions::release_requeues_without_consuming_an_attempt` | PASS |
| P8 hygiene | — | `mise run fmt`, `mise run lints`, `mise run check:deps`, `mise run check:tenant-isolation`; wyrd-server lib `verification::` (non-pg) 50/50; `mcp::tests::every_tool_schema_is_accepted_by_common_tool_callers`; vala-eval `blocks::` 22/22; `git diff --check`; untracked files are review artifacts only | PASS |

### Diagnosis — FIND-2 Agent `code.run` returned `Ok(())`

- **Symptom:** first P7 run; the Agent-with-`code.run` arm of `capacity_refusal_is_busy_at_every_hosted_consumer` returned `Ok(())` instead of `Busy`.
- **Evidence:** the Skald Agent loop treats a tool error as a tool result and continues; with the immediate mock provider the run finished within one poll, so the refusal observed by the tool never reached the block result.
- **Cause:** the refusal was raised inside a tool call but nothing ended the run or surfaced it after `run_prompt`.
- **Fix site:** vala-eval Agent session (`blocks/tools.rs`, `blocks/agent.rs`): the recorded tool records the refusal, a `before_model` hook aborts the next model call, and the post-run check returns `BlockError::Busy`. All Agent tool paths route through `RecordedTool`, so no other caller needed a change.

### Limits

- Direct execute `EngineError::Busy` → 503 has no end-to-end test: the test server's blocks use default limits, so saturating them would require host load. The mapping is a single match arm in `service.rs`.
- Non-Python block paths are unchanged and stay covered by the existing tests.
- Non-goals held: no migrations, no new public API beyond r9, no gate/test weakening, no platform-specific mechanism.

## Follow-up — queued attempts join the enqueuing caller's trace copy

Direct instruction after R1: a queued (enqueued or SDK `submit`) verification attempt must tie back to the caller's trace even though it runs later. Cause found: the runner linked the attempt with a hard-coded sampled flag after the span started, attached no tenant span sink, and bound no tenant route, so no attempt ever reached the tenant copy; gateway captures inside the attempt fell back to a call-id trace. The fix follows the read-only diagnostician (`wyrd-advise`): persist the caller's sampled bit with a typed cause, start the attempt span linked and under the sink before first entry, and bind the route from the accepted authority. Operator sampling stays independent, the attempt stays a linked trace (not a child), and the request path does no extra Postgres write. The migration was edited in place (nothing has shipped).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Caller's own sampled decision captured at enqueue and persisted with the cause | `wyrd-telemetry/src/context.rs` (`Continued { sampled }`, `caller_sampled`); `wyrd-sql` migration `20260601000029` (`cause_sampled`, CHECK); `verifier_executions.rs::Cause`; `service.rs::enqueue` | `context::tests::only_spans_of_a_continued_request_read_a_continued_context`; `pg_verifier_executions::every_attempt_claims_the_submissions_cause` (cause identical on first claim and on reclaim) | PASS |
| Attempt starts linked under the tenant sink, routed to the enqueuing principal with SYSTEM as actor | `context.rs::{serve_linked_work, adopt_span}`; `runner.rs::{claim, trace}` (attempt and `claim` spans created unentered before the claim SQL, link and sink installed from the claimed row, `claim` adopted into the attempt; `bind_tenant_route` from `Caller::accepted(..).attribution()`) | RED: `queued_attempt_follows_its_callers_sampling_decision` "sampled caller true: tenant copy []" → GREEN; asserts one `verification.attempt` root, descendants all `verification.*`, sub = writer, act = SYSTEM | PASS |
| Operator policy independent of caller | unchanged `ServerSampler` | `pg_verification_runtime::operator_rejected_queued_attempt_still_reaches_a_sampled_caller` (ratio 0: operator exports no `verification.*`, tenant copy still holds the attempt); unsampled caller links no copied attempt in both tests | PASS |
| Gateway captures inside a queued attempt join the attempt's trace | `serve_linked_work` marks the attempt continued, so `continued_context()` in `gateway/invocation.rs` resolves to the attempt's trace | RED: Rust journey "the judge call is in the timeline" missing → GREEN; `context::tests::linked_work_continues_its_own_trace_with_the_callers_decision` | PASS |
| SDK journeys (Rust, Python, TypeScript): submit under a caller span, wait, the caller's `trace_timeline` returns the linked attempt and its judge call below it | `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs`, `sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py`, `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts` (TS file now installs the process-global provider once, as `action-timeline.test.ts` does) | `verify_in_real_time::traced_sdk_submit_links_its_attempt_to_the_callers_trace` (+ inline `traced_sdk_verify_joins_the_callers_trace`, `submitted_answer_is_judged_in_the_background_and_resumes_by_id`); pytest `test_traced_sdk_submit_links_its_attempt_to_the_callers_trace` + `test_traced_sdk_verify_joins_the_callers_trace`; vitest `-t 'traced sdk'` 2/2 | PASS |
| Architecture states the behavior | `architecture/wyrd-design.md` trace-context section | Static comparison | PASS |

Commands: server tests run under `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_verification_runtime::<name>)'`; SDK journeys under the same wrapper after `mise run db:migrate:all:inner` (Rust `-p wyrd-ai --test integration -P journey --run-ignored=all`; Python `uv run python -m pytest -m integration <node>` after `mise run py:setup`; TypeScript `pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t 'traced sdk'` after `ts:build`/`ts:build:testing`).

Limits: cross-tenant isolation with a reused caller trace id is not re-proven for queued attempts; routing comes from each execution's persisted authority, and `same_inbound_trace_id_routes_each_tenant_separately` covers the shared route/sink path. Scheduled runs carry no cause and stay out of the tenant copy by design.

### Diagnosis — `attempts_record_queue_wait_phases_terminal_latency_and_one_trace`

- **Symptom:** after the first fix, the runtime module ran 39/40; the test failed with "claim joins the attempt trace: [verification.load, verification.engine, complete, verification.settle]".
- **Evidence:** `runner.rs` awaited `queue.claim` before creating `verification.attempt`; the SQL `#[tracing::instrument]` `claim` span closed before its parent existed. tracing-opentelemetry 0.32.1 takes a span's start time at creation and fixes its parent and links when it is first entered (`layer.rs` `on_new_span`, `on_enter`, `on_close`), and `TenantRoutes::start` / `ServerSampler` decide tenant eligibility when the span starts.
- **Cause:** moving the claim ahead of the attempt (needed so the link and the sampled flag exist before the attempt starts) moved the claim statement out of the attempt's trace.
- **Fix site (per a fresh read-only diagnostician, Codex with `wyrd-advise`, which kept the assertion as the acceptance criterion):** `runner.rs::claim` creates both spans unentered before the SQL. Once the row is claimed it records identity, installs link and sink, and adopts `claim` into the attempt with `wyrd_telemetry::adopt_span`, keeping its original start time. The SQL method's own `#[instrument]` was removed. Its only production caller is the runner; the wyrd-sql tests call it directly and need no span.
- **Proof:** the four focused runtime tests pass with `WYRD_LOG=info`; the runtime module passes 40/40; `pg_verifier_executions::` passes 22/22; the Rust submit journey passes; `fmt`, `lints`, `check:deps`, telemetry lib 22/22 and `git diff --check` all pass.
