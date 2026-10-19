---
id: TASK-003a-R1
kind: remediation
status: proposed
spec: SPEC-long-horizon-verification
spec_revision: 18
original_task: TASK-003a
---

# Record group handoff failures, validate durations and delete stale grouping code

## Subject and outcome

Implement with `$wyrd-implement` on the merged TASK-001–003 worktree. This is one
bounded remediation of TASK-003a, including the owner's explicit subsequent
instruction to delete unused, retired and stale grouping code. It does not reopen
the approved architecture or accepted tradeoffs.

- Worktree: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-001`.
- Branch: `change/long-horizon-verification-task-001`.
- Approved spec: `changes/active/long-horizon-verification/spec.md`, revision 18, including the owner-approved amendments below.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-003a-simplify-grouped-verification.md`, approved content at base `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`.
- Reviewed candidate/start: `7cda4e93159ab328de2a74068d5e1978a2590818`.
- Review authorities: `verdict.md` and `findings-validation.md` in this directory.

Read AGENTS.md and applicable repository authorities first. The revision-18 spec and
owner amendment below supersede original TASK-003a decisions for findings #1/#2;
preserve its historical approved body and all other obligations. Append remediation evidence and correct
implementation-owned claims that contradict the approved handoff contract; they
never amended that contract. Earlier superseded assessment packets are not
instructions for this remediation.

On completion, retryable blockage retains pending work, owner/auth failure records
a terminal errored execution, accepted after values never exceed one year and are
used exactly, missing composed SDK/scheduling scenarios have
focused proof, and confirmed unused code and stale names/descriptions are gone.
Preserve useful assertions and shared infrastructure. This is functional closure
before separate benchmarking, not a performance qualification.

## Diagnosis and selected corrections

Findings #3–#6 and cleanup retain their independent validation. Findings #1/#2
now follow the subsequent explicit owner decisions; their original validation
recommendations are historical and superseded. Each correction names the existing
owner, actual failure/proof gap, selected boundary and focused proof.
They do not invite new machinery.


### FIND-TASK-003a-1 — Distinguish retryable handoff blockage from terminal owner/auth failure

**OWNER-APPROVED REPLACEMENT.** The original validator recommended retaining every unavailable-owner/refused item. The owner rejected indefinite retry of inactive/retired Services and explicitly approved the following replacement. Historical discovery/validation reports remain evidence for the old candidate, not authority over this amendment.

**Diagnosis.** `scheduler.rs:361–388` deletes pending work even when no execution is created. Retaining every such item would repeatedly reconsider permanently inactive owners. `BackgroundAuthority::resolve` currently returns `None` for both owner rejection and missing issuer/SYSTEM; `claim_due_group` drops inactive principal identity; `GroupPollError::Read(String)` obscures typed authorization failures. Those losses of failure reason must not make infrastructure faults look like owner rejection or authorization failures look retryable.

**Approved outcome.**

| Handoff outcome | Observable handling | Pending item |
|---|---|---|
| Capacity exhausted, retryable Wyrd/database/query failure, degraded/truncated read, or existing non-auth enqueue refusal | Trace the actual reason; evaluate no partial inputs; normal polling can retry. Existing `EnqueueRefusal` target/readiness cases remain blocked-enqueue outcomes, not auth failures. | Retain |
| Owner activity is inactive, owner/Card-bound principal is disabled/deleted, owner authorization is rejected, or the snapshot query is explicitly permission-denied | Record a terminal errored group execution with the actual reason; run no Verifier/provider, create no verdict/result pointer, dispatch no Operator. | Delete only atomically with the error record |
| Execution inserted or identical snapshot already exists | Existing execution owns the work. | Delete atomically with successful handoff |
| Writing the terminal error record fails | Log the recording failure; ordinary transaction rollback applies. | Retain |

A missing issuer, SYSTEM principal or attributable owner principal, database/store outage, malformed internal state or runtime configuration fault is a Wyrd problem, not proof of failed owner authorization. Preserve the typed cause through existing authority/query owners; do not classify every `None` or every read error as auth failure. Keep actual permission/readiness enforcement intact. The owner's activity policy itself is unchanged.

**Selected persistence boundary.** Extend the existing queue's terminal execution state, not a new table or event entity. A pre-snapshot admission failure has this contract:

```text
origin: group; status: errored
binding_id, group_id, owner_card_uid, subject and Verifier: frozen binding/pending identities
sub_principal_id: actual stored owner principal identity (may be inactive; not an accepted grant)
act_principal_id: existing tenant SYSTEM principal recording the refusal
error: existing structured execution error carrying the actual auth/activity reason
settled_at: PostgreSQL time; result_id/verdict: absent; no retry deadline or running lease
inputs/route_overrides/group_snapshot_sha256/group_sources: absent
accepted authority: absent because owner authority was not accepted
```

The current group-shape check requires a snapshot for every group row, and the original authority column is non-null. Narrowly permit the above terminal pre-admission shape: no accepted authority, no inputs/snapshot, `errored` with an error, no result/verdict/lease. Keep mandatory accepted authority and valid pinned inputs for every runnable group execution and preserve existing shapes for ordinary submissions, schedules and already-running failures. An inactive owner's stored identity is attribution only and never becomes accepted authority. Do not fabricate an empty `AcceptedAuthority`, a SYSTEM-as-owner grant, a pinned empty snapshot, or a new claimable `ExecutionInput` variant. If the truthful required identities cannot be obtained, recording has failed and pending work remains. Follow existing pre-release in-place schema/reset policy.

**Transaction and progress.** The existing scheduler transaction claims the pending row, classifies failure, writes either the real execution or terminal error and consumes pending only after that write, then commits once. Failure before commit rolls back. Log/trace tenant, binding, group, reason and recorded execution ID as applicable without secrets. Other tenants and the independent cron loop continue. Retrying the same oldest row repeatedly within one pass is prohibited: preserve the existing end-tenant-pass behavior for retryable blockage instead of blindly continuing its inner claim loop. No new retry counter, expiry, queue, recovery protocol, fairness promise or cursor.

**User visibility.** Use existing binding status `last_execution_id`, then existing execution status/SDK handle. `VerificationExecutionState` already exposes `status=errored`, structured `error`, `group_id` and owner; `group`/`result` are absent. This is a failed execution attempt, not a fabricated verification verdict or Bifrost result. Existing binding status shows only the latest execution; do not add a list/history endpoint or promise older failures can be rediscovered without their execution IDs. An authorized reader can inspect it; rejecting the owner never widens read grants. Reactivating an owner does not resurrect consumed pending work; new source activity creates ordinary new pending work.

**Affected owners.** Existing scheduler, background authority, group poller, queue and execution migrations; binding/public execution projection and current SDK discovery/readback; architecture/user docs. Preserve shared cron authority semantics when retaining failure reasons. Reuse existing structured errors and catalog metadata rather than parsing log strings or adding a parallel error catalog.

**Behavior / RED.** Extend P1 and P2 to fail on the current silent deletion, conflated auth/configuration cause and missing pre-snapshot terminal shape. In P3's real Rust SDK journey, inspect the generated failed execution via binding `last_execution_id`, including its group ID/reason and absent result/snapshot.

**GREEN.** Inactive/retired owner and genuine auth denial each produce one terminal error and consume pending; repeated polls create nothing further. Transient read/SQL/configuration blockage retains the item and can later succeed; failure inserting the terminal error leaves pending and no partial error record. Schema checks refuse this authority-free shape for runnable rows; source/permission checks and Operators remain unchanged.

**REFACTOR.** Keep classification and durable writes with their existing owners; remove redundant branches/decoding introduced by the correction while those assertions stay green. Add no workflow owner.

**Closure.** P1, P2 and P3 cover the changed server transaction, persisted shape and real client visibility. Include the existing claim/settlement assertions and prove no verdict-change reaction. Rust SDK coverage suffices for unchanged shared error/status wire fields; do not add new Python/TS contracts for the same state. Docs must explain the latest-binding-status discovery limit and terminal owner failure.

### FIND-TASK-003a-2 — Reject group_end.after above one year at write time

**OWNER-APPROVED REPLACEMENT.** The owner explicitly selected a write-time maximum of one year, replacing the earlier cap-removal-only recommendation. Define one year as 365 fixed days: `31536000s = 525600m = 8760h = 365d`. Positive values up to and including that boundary are accepted; every larger value is rejected before registration writes. No new configurable maximum or duration unit.

**Diagnosis.** Commit `ab6fd7295` introduced `GROUP_AFTER_MAX_SECONDS = 3155760000` and SQL `LEAST(..., $5)` at `verifier_executions.rs:191–210,1730` as an overflow workaround. It accepts one duration but schedules a different shorter duration. The `forever.yaml` fixture uses `999999999d`, roughly 2.74 million years; retaining that fixture would violate the newly approved write boundary.

**Selected correction.** Enforce the fixed limit in the existing `TriggerDuration::parse` boundary in `wyrd-spec/src/card/trigger.rs`, which already validates positive fixed-unit durations and is used for `GroupEnd.after`. Its serde decoder reaches standalone Trigger writes, inline binding Triggers and the server's `validate_request` decoding of raw HTTP Cards. Reuse the existing invalid-Card-spec HTTP 400 and error metadata, with a clear message naming the maximum; add no public error variant solely for this limit. Server-side validation remains authoritative. Keep authored text and exact accepted seconds.

Delete the SQL constant/comment, `LEAST` shortening, fifth bind and obsolete parameter docs. Calculate PostgreSQL statement time plus the accepted authored interval exactly. Do not replace the clamp with another SQL cap or defer invalid configuration until source ACK/polling. Refresh schema descriptions/docs through existing generators to state the maximum; shared format syntax need not become a complex numeric regex.

**Existing-data/fixture closure.** This parser also participates in reads, so do not promise oversized persisted values remain readable. Follow the already-approved unshipped schema/reset policy; add no compatibility read bypass or migration/recovery machinery. Search active fixtures/examples/tests for oversized group durations. Set the live long-wait fixture to `365d`, update largest-accepted/forever descriptions to its actual long-wait purpose, and keep the neighboring-pending and late-evidence progress assertions. Existing historical review evidence stays historical.

**Behavior / RED.** Extend existing P7 to accept the four equivalent one-year boundaries, reject the smallest over-limit value in each supported unit, preserve malformed/non-positive/removed-field rejection and authored round-trip. In P3, send raw HTTP registration for both an oversized standalone Trigger and an oversized inline group_end binding: each returns the existing clear 400 before any Card/binding projection persists; corrected one-year registration succeeds.

**GREEN.** Enforce the shared write boundary, delete the SQL workaround and fix the live fixture/docs. Extend P2 with an immediate exact database due-time assertion for an allowed one-year duration while retaining ordinary short-duration/debounce/claim/settlement assertions; P4 proves the adjusted long-wait neighbor still does not execute and late evidence progresses. There is no century-scale runtime test.

**REFACTOR.** Reuse existing parsing, error mapping and schema generation; no new settings or validation framework. Keep all accepted durations exact.

**Closure.** P7, P2, P3, P4 and P8, plus contract generation checks. This finding is independent of #1's scheduling-failure handling.

### FIND-TASK-003a-3 — Prove one conversation composed from all three real SDK runtimes

**CONFIRMED; MISSING; acceptance-proof gap.** Discovery B-03. Locations: Rust `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:1335`, Python `sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py:470`, TypeScript `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:521`.

Task Scenario 1 expressly requires a real shared conversation populated by Rust, Python and TypeScript under the same server/tenant/owner/group. Each complete current C1 body creates its own server/support state; four direct turns, submit and replayed HTTP enqueue all originate in that single language host. No other C1 caller invokes the other SDK runtimes against that common owner. Three parity tests therefore do not prove the required composition; no production cross-runtime defect is asserted.

**Smallest correction.** Extend the existing Rust C1 host's shared conversation with real Python and TypeScript SDK calls against its existing bound server/credentials/bundle/group. Retain six exact facts, ordinary verify/submit, replay, ownerless/denied negatives and pinned readback. Reuse the existing SDK runtime/process support and public hydration/client APIs; existing cross-runtime process patterns already occur in Python local-development integration support. Do not create another server, general adapter API or test framework, and do not initialize Python/Node lifetimes inside Rust. Let foreign code run in its own process. Reversible local orchestration remains implementation-owned.

**Closure.** P3 selects one host test which must fail if either foreign SDK contribution is missing or assigned to another owner/group. Existing separate parity/negative journeys stay. No full SDK suites are required.

### FIND-TASK-003a-4 — Make C13's mandatory concurrency observations non-vacuous

**REVISED; MISSING; important proof gap.** Discovery B-04/INVREV-003/SYS-001. Locations: `crates/wyrd/wyrd-server/tests/integration/pg_verification_groups.rs:584–600,630–662,692–704`.

Task C13 explicitly requires actual query invocations, a source item ACKed during held handoff, and ordinary cron work scheduled while a group snapshot blocks. The full test and Support setup show: support.yaml has no schedule binding; the final `batched.pass` merely returns Ok and no queue state is checked; the manually joined pass methods bypass `VerificationScheduler::run`'s new separate-loop composition. Empty group-pass vectors do not observe absence of reads. The concurrent arrival directly invokes record_group_activity using a second TenantConn, proving the SQL row-lock seam but bypassing the Scribe writer/ACK ordering required by the scenario. C8 proves result ACK rescheduling separately, C11 proves pressure/drain separately, and C12 scheduled scoring separately; none closes these combined seams.

Production source currently has separate loops (`scheduler.rs:136`) and capacity acquisition before claim (`:353`); this is a missing regression proof, not a claim they currently fail. The production topology shares the runner semaphore with group preparation, while ordinary cron admission has its own future.

**Smallest correction.** Strengthen the existing C13 owner using current server, scheduled fixtures, queue readback and query-fault/trace controls. Exercise the actual scheduler run owner; while its group query is observably stalled, make a real ordinary cron occurrence due and assert its durable queue execution before releasing/cancelling the group read. This must distinguish simultaneous progress from a cron occurrence that ran before the stall. Observe query activity for full-capacity and idle/post-handoff cases rather than only returned vectors. Send/ACK a real matching source item during the held transaction and verify its background upsert leaves fresh pending work after handoff; retain the useful direct SQL lock/claim exclusion proof. Keep rollback/cancellation/32-item assertions. No new fault API, instrumentation framework, harness or production mechanism is prescribed.

**Closure.** P1 alone. The current independent P1 PASS in verification.md proves current assertions execute, not these absent observations.

### FIND-TASK-003a-5 — Couple Python/TypeScript pressure recovery to the observed group

**CONFIRMED; MISSING; runtime-owned proof gap.** Discovery B-05. Locations: Python test `:1021–1057`, TypeScript test `:1055–1095` in the files above.

Task Scenario 7/C11 requires grouped pressure refusal, retry/drain and resulting group execution. The complete Python/TS bodies pressure and retry assistant answer-check, which lacks a group-end binding; after pressure ends they create support's binding and submit a new turn. Python even uses another group string; TS reuses the string with another owner/source. Exact frozen tuple matching excludes the pressured source from support's group, so this tail cannot establish the required retry-to-pending/snapshot path. Rust C11 establishes support before pressure, retries that source and asserts its one pinned fact; it is the existing sibling pattern to reuse.

**Smallest correction.** In each existing Python/TS C11, register/hydrate the eligible support source before pressure, pressure its actual grouped verify/submit, retry the same eligible source/group after drain, then read that group's execution and assert accepted retry once/refused attempts absent. Preserve typed retry timing, neighbor progress, ungrouped coverage and accepted-row accounting; remove redundant independent happy-path tails. No production change or extra harness.

**Closure.** P5/P6 only, in their owning interpreters. Rust C11 needs no new run solely for these test changes.

### FIND-TASK-003a-6 — Complete changed-item rustdoc

**CONFIRMED; VIOLATION; explicit merge gate.** Discovery STD-001. AGENTS §16 and agent-rules explicitly require substantive rustdoc on touched private items/aliases/tests and # Panics when a panic remains possible. `groups.rs:407` introduces Firsts with ordinary // only; `Visible::input/result` at `:385–401` merely repeat their operation names despite different execution/result identity keys and shared Earliest retention; test `Support::activity` at `pg_verification_groups.rs:252–257` accepts &str and expects GroupId validation without documenting its panic.

Full bodies, Earliest::keep, Sources::decode, Visible::executions/pin and activity's C13 callers confirm these exact contracts. Document the alias's two optional time anchors, the insertion methods' respective keys/earliest-payload retention and the invalid-group panic. No behavior/refactor/test is required for prose. Batch with the same remediation, inspect against source and run Rust format/lint checks. This explicit rustdoc rule, unlike optional naming preferences, can block by itself.


## Required owner-directed cleanup

These mandatory human additions were independently validated separately; no retrospective FIND IDs are assigned.

### HUMAN-CLEANUP 1 — delete unused DueGroup projection fields

Locations: `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:989` (`DueGroup::due_at`), `:993` (`card_uid`), `:995` (`verifier_card_uid`); their construction in `claim_due_group` at `:1758–1784`; `GROUP_BINDING_SQL` at `:236–242`; `DUE_GROUP_SQL` at `:229–234`.

These fields have producers and no consumers. A repository-wide `DueGroup` reference search finds only its definition, `claim_due_group`, `GroupPoller::poll`, and `Visible::pin`. The scheduler uses binding/group identity, owner principal, owner Card and settings. The poller reads owner Card/settings/group identity; `Visible::pin` uses binding/group identity. The SQL integration claim assertion at `pg_verifier_executions.rs:2681` reads binding/group/settings/owner, not these fields. `due.due_at` at query line 1673 belongs to the distinct **DueBinding** schedule path and must remain.

Smallest deletion boundary:

- Delete these three fields and their rustdoc/constructor assignments from **DueGroup** only.
- Stop selecting/deserializing `due_at` in the pending claim's returned projection. Keep the pending table column, its update, due-time filtering/order and due indexes: those are active scheduling state.
- Narrow `GROUP_BINDING_SQL` and its result tuple to owner Card and frozen `group_end`; remove only the discarded subject/Verifier identity decoding in this claim path.
- Keep `resolve_binding` and its exact target/readiness checks in `enqueue_group`; they own admission. Keep the exact owner/subject/source-Verifier matcher in `RECORD_GROUP_ACTIVITY_SQL`. Do not delete persisted Card/Verifier columns from verification bindings or GroupActivity.

Proof: compile/typecheck the existing SQL/server owners and run the existing focused `group_snapshots_are_idempotent_and_react_in_settlement_order` and `pending_groups_debounce_and_stop_after_handoff` tests when implementing. No new test is needed to mirror field deletion. Inspect the remaining DueGroup producer/consumers and confirm the three fields are absent.

### HUMAN-CLEANUP 2 — remove stale descriptions of retired behavior

1. `crates/wyrd/wyrd-server/src/verification/runner.rs:402–403` says a group snapshot runs "unless a source exceeded the read ceiling, which completes inconclusive without running." That branch and its outcome helper were deleted. Replace that clause with the actual behavior: a group snapshot runs over its pinned source lists. Keep the rest of dispatch's description and its actual live branches (`ExecutionInput::Grouped { inputs, .. }` at `:445`).
2. `docs/src/content/docs/understand/how-verification-works.md:166` says "`schedule` is the only automatic Trigger." Delete that sentence; retain its following Bind-a-Verifier link. The same page's table at `:163` and group-end explanation at `:168` describe the other automatic Trigger. This is a stale contract explanation inherited immediately beside the changed grouped documentation.
3. `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:1652` still describes a media assertion as "review, coverage, or refusal". Remove the retired coverage term and describe the live selected-execution/provenance assertions, while doing the terminology cleanup below.

Proof: read the corrected documentation against the real dispatch branches and after-only Trigger; normal scoped docs/format checks suffice. Do not recreate the retired ceiling/verdict behavior to make the comment true.

### HUMAN-CLEANUP 3 — retire grouping's stale review names, preserve the live tests

The six fixtures below are **live**, not unused: Service fixture references reach them and the SDK/MCP tests register those Services. Delete the old filenames/metadata names through an atomic rename, not by deleting their useful Verifier behavior or keeping compatibility aliases. The original task's "Verification terminology" paragraph already explicitly requires this rename and all callers. Suggested concrete names use the task's existing Verifier vocabulary:

| Old fixture basename / metadata.name | Replacement basename / metadata.name | Direct fixture callers |
|---|---|---|
| `conversation-review` | `conversation-verifier` | `support.yaml`, `support-ambiguous.yaml`, `forever.yaml` |
| `coding-review` | `coding-verifier` | `coding.yaml`, `coding-external.yaml`, `coding-unbound.yaml` |
| `event-review` | `event-verifier` | `frames.yaml` |
| `late-review` | `late-verifier` | `late.yaml` |
| `refund-review` | `refund-verifier` | `refund.yaml`, `refund-on-failure.yaml` |
| `undeclared-review` | `undeclared-verifier` | `support-undeclared.yaml` |

All paths in that table are under `fixtures/cards/verify_in_real_time/`. Rename each YAML and its metadata.name together; update every relative YAML reference listed. Update accompanying fixture comments/descriptions in `coding-external.yaml`, `coding-unbound.yaml`, `support-ambiguous.yaml`, `frames.yaml`, `late.yaml`, `reserved-source.yaml`, `undeclared-review.yaml`, `late-review.yaml`, and `refund-alert.yaml`. Preserve the source grammar and exact version/source identities.

Live helper/caller boundaries to rename rather than remove:

- Rust `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:2495` `group_reviews` → `group_executions`, its calls at `:2540` and `:2659`; `assert_no_new_review` at `:2532` → `assert_no_new_group_execution`, with all calls at `:2722`, `:2865`, `:2985`, `:2994`. Update grouping-local `review`/`reviews` variables, assertion messages and rustdoc in the grouped refund, coding/media, conversation, late-evidence, and authority scenarios in this file. They read retained ordinary executions; their real readback/no-new-execution assertions must remain.
- Python `sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py:854` `group_reviews` → `group_executions`, calls at `:929` and `:954`; update grouped result variables/messages and refund history assertions. Update the coding fixture-name description in `tests/integration/support.py:32`.
- TypeScript `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:897` `groupReviews` → `groupExecutions`, calls at `:951`, `:955`, `:978`; update grouped result variables/messages and fixture-name descriptions (including `:53`).
- MCP `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:1331` `group_review` → `group_execution`, calls at `:1466`, `:1747`, `:1783`, `:1825`; update its helper parameter/result names, grouped comments/rustdoc and grouped assertion messages, including `ids` at `:1312`.
- The fixture-only webhook `/hooks/refund-review` in `refund-alert.yaml:11` becomes `/hooks/refund-verification`; change both receiver lookup occurrences in Python (`:604`, `:607`) and both expected-path values in TypeScript (`:644`) together. Keep both webhook assertions and the Operator action.

There are no current production SDK exports or generated declarations named GroupReview to delete. These stale names are fixture/test vocabulary. Actual code assessment language such as the coding Agent checking whether spans were reviewed live (`spans-reviewed`, prompt statements, LIVE_REVIEW provider matching), the unrelated customer-facing sentence "We will review your request", and software assessment workflow paths do not denote a separate grouping entity. Preserve that behavior and vocabulary where it describes the real check rather than naming a group execution. Do not blindly delete every lexical occurrence of "review" repository-wide.

Proof: source search for the six old fixture basenames and the four old helper names finds no remaining active caller/reference after the atomic rename; read the grouped local names/messages in the four journey owners. Run the existing affected focused grouped journeys in their owning Rust/Python/TypeScript/MCP lanes, retaining their evidence, no-repeat and webhook assertions. Do not introduce a permanent grep gate or new test framework.


## Constraints and preserved behavior

- Keep inline verify/staged submit, authenticated owner attribution, background-only
  pending SQL, exact source identities, permission checks and RLS unchanged.
- Reuse existing scheduler/queue/Scribe/query owners, SQL transactions, fault
  controls and runtime-owned journeys. No new retry/recovery owner, ledger,
  source completion tracking, visibility barrier, resource budget, transport,
  dependency, compatibility alias or test framework.
- Keep ordinary cron skip rules, manual idempotency, leases, submission reservations,
  non-blocking audit, TenantConn/OperatorPool ownership, shared runner capacity
  and current bounded group/tenant passes.
- Genuine auth/owner-activity rejection produces the terminal error and consumes
  pending atomically; retryable runtime/configuration blockage retains pending.
  Neither path authorizes forbidden execution or promises fairness.
- Accepted limits remain: ACK may precede complete query visibility; crash between
  ACK and pending scheduling can lose the signal; retention/outbox loss applies;
  continuous activity postpones expiration; distinct snapshots overlap and react
  in settlement order; durable SQL backlog may grow. No strict quota/global
  fairness is promised. Do not add safeguards for these accepted limits.
- Reject group_end.after over 365 days at write time; accepted positive values
  remain exact. Remove the SQL clamp, fix oversized active fixtures and preserve
  their useful assertions. No configurable limit, infinity or overflow protocol.
- The fixture/helper rename is atomic, with no old-name aliases in active callers.
  Preserve source grammar/version pins, active Audit vocabulary, actual Agent
  trace-analysis text, negative rejection fixtures and historical records.
- Revision 18 records these owner-approved behavior changes; no UI or benchmark
  implementation. REQ-015/AC-009 remain pending subsequent benchmarking.

## Acceptance criteria

| Closure | Required outcome | Proof |
|---|---|---|
| FIND-1 | Owner inactivity/retirement/auth rejection creates one visible terminal error and consumes pending; retryable Wyrd blockage retains it. Error-write failure rolls back. No evaluation/verdict/Operator, fabricated authority/snapshot or repeated owner-failure polling. | P1/P2/P3 |
| FIND-2 | Write-time standalone and inline Trigger registration rejects values above 365d; equivalent boundaries succeed. Accepted due times are exact, SQL cap is deleted and adjusted long-wait/late-neighbor assertions remain. | P7/P2/P3/P4/P8 |
| FIND-3 | One six-fact conversation contains actual Rust/Python/TS contributions under one server/tenant/owner/group; existing verify/submit/replay/negative/pinned-readback assertions remain. | P3 |
| FIND-4 | Actual scheduler loops admit real cron work during observed blocked snapshot; observed query activity proves no reads when idle/at capacity; real source ACK during handoff leaves fresh pending work. Existing rollback/cancel/claim/batch assertions remain. | P1 |
| FIND-5 | Python/TS pressure, retry and readback use the same bound source/group; accepted retry appears once, refused attempts do not; existing admission and neighbor progress remain. | P5/P6 |
| FIND-6 | Cited changed Rust items have substantive rustdoc and documented remaining panic contracts. | Inspection/fmt/lints |
| HUMAN-CLEANUP 1 | Unused DueGroup fields/projections deleted; live persisted due state, source identity, admission and cron fields retained. | P1/P2 plus caller inspection |
| HUMAN-CLEANUP 2 | Runner/guide/MCP descriptions reflect current behavior; retired ceiling/coverage and only-automatic-Trigger claim removed. | Source comparison/docs/format |
| HUMAN-CLEANUP 3 | Six fixture names, grouped helper/local names/messages and webhook paths atomically updated with all active references and useful assertions preserved. | Focused commands below and active-reference search |

## Focused verification and evidence

Extend the existing focused selectors below; no full journey suites or broad gate
are required by this remediation. Run build prerequisites only if native artifacts
need refreshing. Record exact command, feature set, nonzero selection count,
result and RED/GREEN proof for changed production behavior (#1/#2). The missing
scenario proofs (#3–#5) may pass against unchanged production code; record that
as verification-only evidence rather than manufacture a failure. Deletions/docs
need source/static proof, not synthetic RED tests. Trace unexpected failures before
diagnosis and never weaken assertions or gates merely to clear a run.


These are proposed remediation proof commands, not new runs by this validator. Use tracing on failure and confirm nonzero exact selection counts. Existing prerequisites in TASK-003a apply only when native builds change; do not rebuild every SDK merely to rename local text.

**P1 — existing C13; FIND-1/4 and cleanup 1**

```bash
WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_groups::pending_groups_debounce_and_stop_after_handoff)"'
```

**P2 — existing C9; FIND-1 persisted failure shape, FIND-2 exact accepted duration and cleanup 1**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_verifier_executions::group_snapshots_are_idempotent_and_react_in_settlement_order)"'
```

**P3 — existing Rust C1 host; FIND-3, FIND-1 public error readback and FIND-2 raw HTTP write rejection**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_support_resumes)"'
```

**P4 — existing Rust C6; FIND-2 live fixture closure**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_executions_include_late_evidence)"'
```

**P5/P6 — existing Python/TS C11; FIND-5**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_verification_staging_refuses_and_recovers'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^verification_staging_refuses_and_recovers$"'
```

For explicit cleanup 3's additional affected fixtures/webhooks, reuse only these original-task exact selectors (each retains its original managed-Postgres command): C2 Python `::test_grouped_refund_changes_verdict`, TS `^grouped refund changes verdict$`; C3 Rust `test(=verify_in_real_time::grouped_coding_uses_declared_sources)`; C5 Rust `test(=verify_in_real_time::grouped_conversations_coexist_with_drift)`; C10 MCP `test(=verification::pg_tests::grouped_frames_use_visible_outcomes)`. Their complete command forms are already fixed in the original TASK-003a C2/C3/C5/C10 catalogue. No new test names/selectors/harness are prescribed.

Static lanes for the actual bounded write set: Rust fmt/lints, check:tenant-isolation for changed SQL; Python format/lints/typecheck for touched Python; TypeScript format/lints/typecheck for touched TS; docs:check for guide correction; git diff --check. Reuse existing mise tasks. The owner amendments change internal execution constraints and shared Trigger validation/descriptions; run codegen:regen/codegen:check and P8 for those contracts. No dependency change or broad gate is independently demanded. Keep any new/changed Rust rustdoc complete. Add a specialized owning lane only if the implementer's actual native support write set changes that boundary. Run Cargo-backed lanes sequentially on this checkout.


### Added focused owner-amendment commands

**P7 — existing Trigger contract test; one-year fixed-unit boundaries**

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::trigger::tests::group_end_decodes_its_duration)'
```

**P8 — existing served group contract; description/projection closure**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_openapi_contract::grouped_verification_contract_is_served)"'
mise run codegen:regen
mise run codegen:check
```

### Expanded additional cleanup commands

These are the complete existing command forms for the five additional cleanup
selectors above. They cover webhook paths, coding/negative fixture registration,
shared coexistence helpers and MCP media, rather than repeating all C1–C13.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_grouped_refund_changes_verdict'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^grouped refund changes verdict$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_coding_uses_declared_sources)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_conversations_coexist_with_drift)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=verification::pg_tests::grouped_frames_use_visible_outcomes)"'
```

## Completion record

Append source locations, exact focused commands/counts/results, RED/GREEN records,
failure diagnoses and final diff/non-goal audit to this task. Map each FIND-1–6
and HUMAN-CLEANUP 1–3 acceptance row to its proof; retain the full stable FIND IDs
in the finding dispositions. Record deleted fields/projections, atomic renames
and absent active old references, preserving source/authz/readiness/assertions.
Explicitly leave accepted limits unchanged and REQ-015/AC-009 pending. Return the
committed cumulative candidate to `$wyrd-task-review`; this task alone does not
approve the integrated change or claim benchmark readiness.

## Readiness validation of owner amendments

Independent task-handoff review on 2026-10-18: **Ready**, architecture PASS,
executability PASS, independent system perspective and cold rehearsal PASS,
zero Critical/Major findings. The whole amended task was read, including
remaining findings/cleanup and the production-TDD versus verification-only proof
distinction. No additional overkill or material implementation choice was found.

Artifact: [owner-amendment readiness review](../../../../../.dev/review/task003a-r1-owner-amendment/review.md).
This is plan readiness only; implementation, focused tests and later cumulative
task acceptance still remain.

## Implementation evidence

Candidate: `c2009cf16..4984fdadf` on `change/long-horizon-verification-task-001`
(base `7cda4e931`). Commits: `c2009cf16` (revision 18 and review packet),
`cd405bd36` (one-year cap and refusal shape), `77a23b3d3` (scheduler/authority/
groups retain blockage and record refused owners), `1f2d6b301` (C13 refusal,
retained blockage, concurrent scheduling), `af3898082` (inline Card decode
error), `6df5085d1` (C1 three-runtime conversation, duration and owner
refusals), `496477ab3` (typed input slot decode error), `ba6553d6f` (fixture,
helper and webhook renames; negatives moved out of the valid corpus),
`4984fdadf` (Python/TS C11 bound-group pressure and recovery).

### Acceptance matrix

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-003a-1 | `wyrd-server/src/verification/scheduler.rs` (handoff classifies retryable blockage vs owner/auth refusal; refusal goes through `refuse_group`), `authority.rs`, `groups.rs`; `wyrd-sql/src/queries/verifier_executions.rs` `VerifierExecutionQueue::refuse_group` (one errored group execution + pending delete in the caller transaction); migration `20261018000000_verification_group_pending.sql` | P1, P2, P3 (revoked Service owner → `Errored`, `WYRD_AUTH_401_CREDENTIAL_REVOKED`, group `conv-2`, no result/group/dispatches) | PASS |
| FIND-TASK-003a-2 | `wyrd-spec/src/card/trigger.rs` `TriggerDuration::parse` rejects > 365d; SQL clamp deleted; `wyrd-spec/src/reference.rs` manual `InlineableRef` decode surfaces the inline error | P7 (`card::trigger::tests::group_end_decodes_its_duration`, `reference::tests::inlineable_ref_reports_the_inline_spec_error`), P2, P3 (raw HTTP 366d/8761h → 400 with `365d`, Card absent; 365d/31536000s accepted), P4, P8 | PASS |
| FIND-TASK-003a-3 | `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs` `Grouped::foreign_turn` spawns real Python (`uv run --no-sync python`) and Node (`wyrd-ai`) clients against the same server/tenant/Service key/group; turn 1 Rust, 2 Python, 3 TypeScript | P3 (six-fact evidence, replay, conflict, ownerless/denied, pinned readback retained) | PASS |
| FIND-TASK-003a-4 | `wyrd-server/tests/integration/pg_verification_groups.rs` C13: real cron admission while a snapshot is held, observed query activity idle/at capacity, real source ACK during handoff leaves fresh pending | P1 | PASS |
| FIND-TASK-003a-5 | Python `test_verification_staging_refuses_and_recovers` and TS C11 pressure the bound `support` source on group `pressured`, retry after drain, read the group execution (`[pressured]` evidence once) and count `4 + offered` rows | P5, P6 | PASS |
| FIND-TASK-003a-6 | Rustdoc with `# Errors`/`# Panics` on every changed item in scheduler, authority, groups, `verifier_executions.rs`, trigger, reference, verifier `InputSlot`/`TypedInputSlot`, C1/C13/MCP tests | Inspection; `fmt:check`, `lints` | PASS |
| HUMAN-CLEANUP 1 | `DueGroup` loses `due_at`, `card_uid`, `verifier_card_uid`; `claim_due_group` SQL projects only binding, group, owner, settings, principal, active. Persisted `due_at`, source identity (`GroupActivity`), admission and cron fields retained | P1, P2; `git grep` callers of `DueGroup` | PASS |
| HUMAN-CLEANUP 2 | `runner.rs`, `docs/understand/how-verification-works.md`, MCP `verification.rs` docs and `# Panics`, `card-types.ts` describe current behavior; retired ceiling/coverage and only-automatic-Trigger claims removed | `docs:check`, `ts:typecheck`, `fmt:check`, source comparison | PASS |
| HUMAN-CLEANUP 3 | Renames below, applied atomically with all YAML refs, `metadata.name`, webhook path and Rust/Python/TS/MCP helpers | C2 Py/TS, C3, C5, C10, P3, P4, P5, P6; active-reference search empty | PASS |

### Renames (HUMAN-CLEANUP 3)

| Old | New |
|---|---|
| `conversation-review`, `coding-review`, `event-review`, `late-review`, `refund-review`, `undeclared-review` | `conversation-verifier`, `coding-verifier`, `event-verifier`, `late-verifier`, `refund-verifier`, `undeclared-verifier` |
| `/hooks/refund-review` | `/hooks/refund-verification` |
| `group_reviews` / `groupReviews` (row struct) | `group_executions` / `groupExecutions` (`Row`) |
| `assert_no_new_review` | `assert_no_new_group_execution` |
| MCP `group_review`, `review_id` | `group_execution`, `group_execution_id` |

`git grep -nE "(conversation|coding|event|late|refund|undeclared)-review\b|group_reviews|groupReviews|assert_no_new_review|hooks/refund-review" -- ':!changes' ':!.dev'`
returns nothing. Live-test names `HeldReview`, `REVIEWED`, `LIVE_REVIEW`,
`spans-reviewed` and `LATE-REVIEW` belong to non-grouping review behavior and
are kept.

### Focused commands, counts and results (final candidate unless noted)

| Proof | Selector | Result |
|---|---|---|
| P1 | `WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_groups::pending_groups_debounce_and_stop_after_handoff)"'` | 1 passed, 275 skipped |
| P2 | `... -p wyrd-sql --test integration -E "test(=pg_verifier_executions::group_snapshots_are_idempotent_and_react_in_settlement_order)"` | 1 passed |
| P3 | `... -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_support_resumes)"` | 1 passed (14.4s) |
| P4 | `... -E "test(=verify_in_real_time::grouped_executions_include_late_evidence)"` | 1 passed (40.7s) |
| P5 | Python `test_verify_in_real_time.py::test_verification_staging_refuses_and_recovers` | 1 passed |
| P6 | TS `verify-in-real-time.test.ts` C11 exact `-t` | 1 passed |
| P7 | `mise exec -- cargo nextest run --locked -p wyrd-spec` (crate, after decode fixes); exact `card::trigger::tests::group_end_decodes_its_duration` rerun on final HEAD | 768/768; 1/1 |
| P8 | `pg_openapi_contract` `grouped_verification_contract_is_served` | 1 passed |
| C2 | Python and TS `grouped refund changes verdict` | 1 passed each |
| C3 / C5 | `grouped_coding_uses_declared_sources` / `grouped_conversations_coexist_with_drift` | 1 passed each |
| C10 | `wyrd-mcp --test mcp verification::pg_tests::grouped_frames_use_visible_outcomes` | 1 passed |
| Static | `codegen:regen` + `codegen:check` (no diff), `fmt:check`, `lints`, `check:tenant-isolation`, `py:lints`, `py:typecheck`, `ts:format:check`, `ts:lints`, `ts:typecheck`, `docs:check`, `git diff --check 7cda4e931..HEAD` | all exit 0 |

Native artifacts were rebuilt (`py:setup`, `ts:build`, `ts:build:testing`) before P5/P6/C2.

### RED/GREEN

- FIND-1/FIND-2 production: at `7cda4e931` the refusal API, the pending
  constraint and the one-year check were absent; P7 RED observed (366d accepted),
  then GREEN.
- Inline refs: `reference::tests::inlineable_ref_reports_the_inline_spec_error`
  RED with `data did not match any variant of untagged enum InlineableRef`, GREEN
  after the manual decode.
- Input slots: the existing `card::verifier_card_tests::removed_scheduled_samples_fields_fail_at_their_path` was RED and turned GREEN.
- FIND-3/4/5 are verification-only test strengthening; each new assertion was
  observed against the implemented behavior.

### Diagnoses

1. **Inline duration message.** *Symptom:* an inline `runs_on` with 366d failed
   with a generic untagged-enum error, not the `365d` limit. *Evidence:* serde's
   untagged `InlineableRef` discards variant errors. *Cause:* derived untagged
   `Deserialize`. *Fix site:* `wyrd-spec/src/reference.rs`, the shared owner for
   every inline Card ref. The manual decode tries ref, then sibling, then inline,
   then path, and returns the inline body's own error. Outside the listed write
   set, but needed for FIND-2's write-time message on the inline path.
2. **InputSlot regression.** *Symptom:* `removed_scheduled_samples_fields_fail_at_their_path`
   failed in wyrd-spec. *Evidence:* the fresh diagnostician traced it to the
   untagged `Source` variant added in `a835c53aa`, which masks the tagged
   variant's `deny_unknown_fields` error. *Cause:* untagged fallback. *Fix site:*
   `wyrd-spec/src/card/verifier/mod.rs`. A visitor dispatches strings to
   `SourceText` and maps to a `#[serde(remote = "InputSlot")]` tagged mirror.
3. **Fixture sweep.** *Symptom:* the wyrd-spec fixture sweep failed on a varying
   Card. *Evidence:* the diagnostician found intentionally invalid
   `reserved-typed`, `reserved-source` and `coding-child` under
   `fixtures/cards/`, in HashMap order. *Cause:* negatives sat in the valid corpus.
   *Fix site:* moved them to `fixtures/invalid/`, following the existing
   precedent. *Side finding:* all three journeys listed `coding-child` as kind
   `Service`, which made its absence assertion vacuous. They now use `Verifier`.
4. **Concurrent Postgres lanes.** *Symptom:* `role wyrd_platform already exists`
   and EOF/SSLRequest errors when C10 and the C2 lanes ran together. *Cause:* the
   with-test-postgres lanes contended when run at the same time. They are
   infrastructure, not product code. *Fix:* the lanes were rerun one at a time and
   all passed. No code changed.

Diagnostician report (fresh read-only agent; given the failing command, trace
and diff): the cause of 2 is the untagged `InputSlot::Source` from `a835c53aa`;
the cause of 3 is the negative fixtures in the swept valid directory; the fix
sites are as recorded above. Its affected-callers list was every consumer of
`InputSlot` and of the `fixtures/cards` sweep.

### Diff and non-goal audit

- Tracked diff is limited to the R1 surfaces above plus the two shared-owner
  decode fixes in `wyrd-spec`, which were needed for correct error reporting. The
  tree is clean, with no untracked files.
- Accepted limits and non-goals are unchanged: no new kinds or routes, no
  fabricated authority or snapshot, and no platform-specific code.
- REQ-015/AC-009 remain pending.
- Material risk: decode errors for inline refs and input slots now report the
  inner error instead of the generic untagged message. This is an improvement
  in text, and the wire shape is unchanged. Negative fixtures moved to
  `fixtures/invalid/`.

Disposition: **IMPLEMENTED**. The cumulative candidate goes to
`$wyrd-task-review`. This task neither approves the integrated change nor claims
benchmark readiness.
