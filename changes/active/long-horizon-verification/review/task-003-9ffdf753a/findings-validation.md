# TASK-003 independent findings validation

**Authority result: BLOCKED. Source/evidence ledger: six retained findings requiring bounded correction.** The ledger is complete despite the separate authority blocker. It neither approves draft revision 14 nor establishes a production starvation, duplication, or deployed shutdown incident.

## Subject and method

- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate: `9ffdf753ab11ce9b638c906388d3690a6aede249`.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-003-bounded-outbox-admission.md`.
- Read all seven discovery reports and `followup-review.md`, the cumulative three-dot diff across all 32 paths, and expanded source at the proposed correction owners. Navigation began with CodeGraph, followed by source/caller searches. No discovery agreement was treated as source proof.
- Applied AGENTS.md, agent rules, spec-driven development, maintainer style, the reference router, Rust/error and analytical reliability guidance, task-review and implementation diagnostic rules, and relevant Wyrd design/doctrine, Bifrost, security and operations contracts. The finite-admission and preserved-lifecycle conclusions use the unchanged TASK-003 / REQ-006 / AC-005/006 obligations.
- The requested approved revision is 13; supplied `spec.md` is revision 14/draft, records revision-13 approval, and says revision 14 adds pre-release schema rules. This does not provide an unambiguous approved subject artifact. Resolve that identity with the caller before final acceptance; do not silently approve 14. This is an authority blocker, not a `FIND-*` source defect or permission to skip the ledger.
- No prior `FIND-TASK-003-*` IDs exist. IDs below are newly assigned only to retained findings. Human-directed additions have not been relabeled as reviewer findings.

## Complete proposal disposition

| Discovery proposal or disputed assessment | Decision | Final disposition |
|---|---|---|
| B-001, INV-REV-001, MAINT-001, FOLLOW-001: paired shutdown counters | REVISED | FIND-TASK-003-1. Confirm the documented generic-owner regression; narrow unchecked-arithmetic and deployed-path claims. |
| B-002 progress part, INV-REV-002, FOLLOW-002a | CONFIRMED | FIND-TASK-003-2. Required continued-offering journey proof is missing; held-A activity remains valid narrower evidence. |
| B-002 identity part, FOLLOW-002b | REVISED | FIND-TASK-003-3. Fixed-key recovery/replay needs existing HTTP proof, without new SDK API or a production duplication claim. |
| STD-003-01; maintainer rustdoc calibration note | CONFIRMED | FIND-TASK-003-4. AGENTS §16 explicitly makes incomplete touched rustdoc blocking, resolving the maintainer report's non-blocking characterization. |
| STD-003-02 | CONFIRMED | FIND-TASK-003-5. Normative audit/operations prose contradicts the selected bounded overflow behavior. |
| STD-003-03 | REVISED | FIND-TASK-003-6. Required independent diagnosis reports are absent from supplied evidence. Keep this an evidence violation; do not infer unjustified test weakening. |
| Concurrency/system/durability reports' sequential deployed-shutdown assessment | CONFIRMED as scoped evidence | It does not rebut the base's expressly supported concurrent generic method contract. |
| Concurrency report's held-A neighbor progress assessment | CONFIRMED as scoped evidence | It proves A is active, not the additional TASK-003 continued-offering condition. |
| Invariant report's fresh-call recovery PASS | REVISED | Fresh-key recovery passes; exact enqueue identity recovery remains FIND-TASK-003-3. |
| System/durability reports: no additional source-local findings | Validated | No extra fault-injection, durable outbox, stronger receipt, availability, or fairness requirement is added. |

## Source and sibling closure map

| Authority / failing value | Producer to consumer and siblings inspected |
|---|---|
| Outbox admission pair | Full `Outbox::{new,stage,reserve,reservable,send,settle,shutdown}`, `Reservation::{stage,drop}`, `Admission::release`, `admit`, `OutboxWriter::{run,dispatch,is_ready,next_retry,finish,release}`. `send` produces pending/reserved-staged counts; retry retains them; ordinary writer completion and failed-send release know their pair. Only shutdown reconstructs it with competing independent swaps. Both production sink implementations, Scribe and SubmissionSink, share this owner. |
| Lifecycle | Server `app/server.rs:850–935` drains Scribe before Bifrost and SubmissionOutbox; test-server fallback at `server.rs:925–944` is sequential too. Scribe shutdown and peer-audit tests, runtime shutdown tests and repository call searches show no deployed same-instance concurrent shutdown caller. Public `Arc<Outbox<_>>` plus the explicit base/candidate concurrency guarantee makes the generic path required nonetheless. |
| Source pressure | Full `VerificationControl::{execute,enqueue,stage_direct}`, HTTP and MCP callers, `ScribeSink::write`, and full `SubmissionSink::write`. Direct reserves two before execution; enqueue reserves downstream then upstream before staging; Scribe ACK spends the carried downstream reservation. Runner result reservation and fenced settlement preserve adjacent successful-result semantics. Audit, gateway capture, tenant spans and other audit call sites keep unreserved non-blocking staging. |
| Journey progress | Complete Rust/Python/TS staging-pressure stories and their changed judge/server/assistant fixtures; runtime first-round/cap and supporting tenant-progress tests. All three stories await B before offering A2 or overflow traffic. |
| Enqueue identity | Full shared client `Target::{execute,enqueue,request}`; full SQL `RequestKey::{new,execution_id}`, `Submission::new`, `VerifierExecutionQueue::{accept_submissions,accept,insert_submission}`; full HTTP `post`/`enqueue` helpers and existing replay/conflict test. The shared client mints one new key per invocation; HTTP supplies an explicit key; the existing queue remains the only durable replay owner. Python and TS project the shared client, so they share this identity behavior. |
| Rustdoc | Full changed execute/enqueue, Scribe constructors, new bound/audit tests and HTTP retry mapper test bodies; adjacent `SubmissionSink::outbox` and HTTP payload-limit test demonstrate existing panic-doc patterns. |
| Audit operating contract | `Outbox::stage` counts/logs unreserved overflow then returns no error; audit adapter/callers retain this path. Security posture and operations overload paragraphs still promise unbounded retention. Updated design submission paragraph and unchanged Bifrost ACK/retry/loss prose provide the boundaries to retain. |
| Diagnosis evidence | Task evidence's five diagnoses and cumulative changes at their stated fix sites, including the complete runtime first-round helper and 200-run test, and TS `serverTest` fixture. Active packet file inventory contains no diagnostic reports; subject and standards report state none were supplied. A narrative saying a diagnostician confirmed a cause is not that report. |

The reuse ladder does not justify deleting admission, reservations or the claim cap: existing dequeue-only channel ownership and hosted-step permits did not supply the required whole-pending/task bound. Extending the existing Outbox, ClaimLoop and durable replay owners is sufficient. None of the retained corrections needs a dependency, new lifecycle owner, test framework, scheduler, public API, queue, SQL capability or platform mechanism.

## Retained findings

### FIND-TASK-003-1 — Final abandoned-work settlement has competing owners

**Status:** REVISED. **Classification:** REGRESSION / INCORRECT. **Sources:** B-001, INV-REV-001, MAINT-001, FOLLOW-001.

**Obligation/location:** TASK-003 preserves shutdown/loss semantics and whole-pending release. The pre-existing and retained `Outbox::shutdown` rustdoc expressly supports concurrent callers with one loss reporter. Changed `crates/shared/wyrd-runtime/src/outbox.rs:348–350`; arithmetic consumer `:438–441`.

**Proof:** Reserved staging increments both pending and staged-reserved under the sender fence. A hanging/failed slice can reach deadline abandonment with both at 1. After writer exit, caller A can take pending=1, another worker's caller B can take pending=0 and staged-reserved=1, then B calls `release(0,1)`. `count-reserved` underflows after reserved was already decremented. Atomics on separate fields do not jointly elect their owner. Normal writer and failed-send paths already retain the pair and do not have this defect. No await between swaps is needed for two worker threads to interleave.

**Consequence:** With checked arithmetic a supported shutdown call panics instead of returning the promised results. Unchecked arithmetic has invalid intermediate accounting; permanent final counter corruption is not established because modular updates may cancel if both callers finish. Ordinary deployed shutdown is sequential, so this is not a demonstrated race in the server coordinator.

**Smallest safe correction:** Reuse the existing nonzero pending-swap winner as the sole owner of staged-reserved capture and final release/loss settlement. Zero-loss callers must not consume the associated staged-reserved count. Keep the existing close fence, writer termination wait, late-stage loss policy and unspent-reservation Drop. Do not mask the defect with saturating arithmetic, remove the concurrency guarantee, add a lifecycle service, or change the server coordinator.

**Closure:** Add focused reserved-item deadline/concurrent-shutdown coverage at the existing inline runtime tests: callers return without panic; exactly one returns the total; loss and gauge settle once; pending and staged/share accounting settle correctly, with separately held unused reservations released only by their owner. Use an ordinary bounded test, no synthetic host load or raised thread counts. Existing successful-drain/deadline regressions remain green. Planned exact selector and preserved regression commands appear below; source review must additionally show the pair can only be consumed by the winner.

### FIND-TASK-003-2 — Continued offered pressure is absent from the required progress journeys

**Status:** CONFIRMED. **Classification:** MISSING acceptance proof. **Sources:** B-002 (progress), INV-REV-002, FOLLOW-002a.

**Obligation/location:** TASK-003 Scenario 3 requires accepted B completion while A continues offering calls. Rust `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:1228–1261`; Python `sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py:478–503`; TS `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:449–467`.

**Proof/consequence:** The common sequence is A1 held → B submitted/completed → A2 held → overload refusals. A makes no further request while B is pending. With capacity 6, A1 holds two of four reservable slots; B uses the remaining room and finishes before overflow begins. This proves coexistence with active A, but cannot catch regressions specific to continuing offered traffic through SDK/server admission. Supporting sink-isolation and pre-seeded SQL tenant-round tests do not drive that producer seam. No starvation defect is established.

**Smallest safe correction:** Extend each existing pressure story with finite controlled A traffic that continues while already-admitted/runnable B completes, including offered pressure. Observe continued requests and typed responses; then stop A and reconcile accepted/refused source work after drain. Reuse current server/judge fixtures and normal bounded requests. Preserve the resources-permit qualifier: do not require B admission/completion while every resource is unavailable, strict shares, a latency SLA, larger runtime machinery, benchmark or host stress.

**Closure:** Each focused SDK selector below must establish B completion before A's offered requests stop, retain refusal-before-source-staging and drain assertions, and select a nonzero test count. The changed test body should make the overlap explicit rather than infer traffic from A1 remaining unfinished.

### FIND-TASK-003-3 — Recovery proof changes the enqueue identity

**Status:** REVISED. **Classification:** MISSING acceptance proof. **Sources:** B-002 (identity), FOLLOW-002b.

**Obligation/location:** AC-005/J-005 step 2 requires retrying the same enqueue identity after drain; TASK-003 Scenario 3 requires replay/recovery. Rust recovery at `verify_in_real_time.rs:1295–1308`, mirrored Python/TS recovery; producer `crates/shared/wyrd-client/src/verify/mod.rs:444–450`.

**Proof/consequence:** Every shared `Target::enqueue` invocation generates a fresh `verification-submit-{Uuid::now_v7()}` key. Reusing `refused_run` after a failed submit therefore proves fresh-key recovery, not exact-key replay. The key is stable within transport retries of one call, which does not bridge two exhausted calls. Existing `pg_verification_routes::enqueued_executions_replay_conflict_and_read_back` proves fixed-key replay without preceding pressure. Current production key derivation/queue replay is not shown defective; the missing composed proof can conceal such a regression.

**Smallest safe correction:** At the existing HTTP enqueue/replay integration owner, refuse a fixed request/key/authority under staging pressure; verify the refused attempt adds no source row; drain pressure; retry the identical tuple; replay that accepted tuple and prove equal execution identity plus one durable queued execution. Reuse explicit `Idempotency-Key` support and `VerifierExecutionQueue` acceptance, with the existing test-server bound control. Do not expose a new SDK idempotency argument or introduce producer SQL, receipt, durable outbox or grouped semantics. Keep all SDK projection/recovery assertions. Grouped array-element evidence remains the expressly deferred TASK-001 integration seam.

**Closure:** Add the focused HTTP test identified below (planned name); prove durable execution cardinality, not only response equality. Preserve the existing replay/conflict test. Source-input absence applies to the refused call; do not invent a new general accepted-replay source-row deduplication contract beyond current semantics.

### FIND-TASK-003-4 — Touched rustdoc omits failure and panic contracts

**Status:** CONFIRMED. **Classification:** VIOLATION. **Sources:** STD-003-01 and maintainer calibration note.

**Obligation/location:** AGENTS §16 explicitly includes private items/tests and makes incomplete touched rustdoc a hard blocker. `components/verification/service.rs:453,604` Errors sections; `scribe_outbox.rs:652,663` constructors; runtime new tests at `outbox.rs:1069,1129`; extended mapper test at `http/error.rs:410`.

**Proof/consequence:** Execute/enqueue now propagate `VerificationStagingFull` before source staging, absent from their Errors lists. Both Scribe constructors spawn through `Outbox::new` and can panic outside Tokio, absent from their docs. Both new runtime tests and the materially extended mapper test retain assertions/expectations but no Panics section. The adjacent SubmissionSink constructor and payload-limit mapper test document these same categories. Generated error docs do not fulfill the touched-item rule. This is a repository-required contract correction, not a suggested API redesign.

**Smallest safe correction/closure:** Amend only these touched docs to name staging refusal and its boundary, Tokio runtime precondition, and assertion/setup failures as applicable. Batch with the substantive remediation. Reinspect all listed bodies/docs together; run Rust formatting/lints. No behavioral code, test framework or new documentation generator is required. Do not create a style-only review round.

### FIND-TASK-003-5 — Normative audit/operations prose still promises unbounded retention

**Status:** CONFIRMED. **Classification:** VIOLATION with operational/data-loss contract consequence. **Source:** STD-003-02.

**Obligation/location:** Wyrd design introduction requires superseded decisions updated in the cohesive change; spec-driven workflow requires durable contracts in their owners. New bounded behavior at runtime `outbox.rs:227–239`; conflicting `architecture/wyrd-security-posture.md:469` and `architecture/operations/reliability-and-recovery.md:73`.

**Proof/consequence:** Unreserved staging now loses/counts/logs overflow immediately while authorization remains unaffected. Security posture says the outbox is unbounded. Operations explicitly exempts it from bounded queues because retention is preferred to dropping. Those statements promise retained outage work that the implementation now intentionally sheds. Changing only the design submission paragraph does not reconcile audit/capture operation contracts.

**Smallest safe correction/closure:** Update the two existing normative paragraphs to the selected finite process staging and counted/logged non-blocking overflow loss. Preserve identical admitted retry slices, one Scribe writer, request independence, acknowledged versus unacknowledged authority, and abrupt-process/deadline loss. No unbounded restoration, alternate writer or durable audit relay. Inspect applicable audit/outbox prose for the same contradiction and ensure consistent scope; static consistency proof suffices for this architecture prose. Approval identity must still be resolved; this recommendation does not approve revision 14.

### FIND-TASK-003-6 — Independent failure diagnoses are claimed without their required reports

**Status:** REVISED. **Classification:** VIOLATION of required evidence. **Source:** STD-003-03.

**Obligation/location:** Task-review requires recorded diagnosis and diagnostician report for failure-driven test/timeout/assertion/etc. adaptations; implementation skill requires a fresh diagnostician before such fixes and its report beside the diagnosis. Task evidence Diagnoses at `TASK-003-bounded-outbox-admission.md:235–239`; affected examples are `pg_verification_runtime.rs:2668–2743,2865` and TS `tests/support/server.ts:157`.

**Proof/consequence:** The candidate changes prior test setup to 70/200 execution slots after failures and scopes the TS provider like the server after a full-file failure. Source supports those explanations: the new 64-task cap cannot enter 70/200 simultaneously held executions; file/test provider lifetimes now agree. Existing outcomes are preserved and an independent cap test is added. However, only summaries saying independent confirmation occurred are supplied, with no inspectable diagnostician report. The active packet and review inputs have none. That prevents the mandated independent failure-cause audit; it does not establish a faulty fix or test weakening.

**Smallest safe correction/closure:** Recover and attach the actual independent reports tied to each failure-driven adaptation/diagnosis, including the failing command, trace/source evidence, cause, correction owner and affected callers. If unavailable, explicitly label historical evidence unavailable and obtain fresh independent read-only diagnosis of the relevant immutable failing transition with tracing and the diff, recording its limits honestly; do not manufacture a historical pre-fix report. The original temporal process omission cannot be retroactively erased, but independently inspectable current cause validation can close the review's evidence gap. Review the five diagnosis entries together, not only the two examples. Preserve correct test setup/assertions and production behavior. Do not add another runtime mechanism or rerun full unrelated suites merely to create evidence.

## Focused closure commands

These are recommendations, **not executions by this validator**. New test names are explicitly planned and must be confirmed with collection/nextest list after addition. Existing selectors were verified from source. Use tracing for any failing diagnostic iteration. No broad aggregate or full SDK user-journey suite is required for remediation.

```sh
# Planned reserved-loss regression and existing deadline/drain regressions (FIND-1).
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::concurrent_shutdown_releases_reserved_losses_once) | test(=outbox::tests::shutdown_flushes_items_that_recover_before_the_deadline) | test(=outbox::tests::shutdown_counts_items_unwritten_at_the_deadline_as_lost)'

# Existing pressure stories extended with continued offered traffic (FIND-2).
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::verification_staging_refuses_and_recovers)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_verification_staging_refuses_and_recovers'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^verification_staging_refuses_and_recovers$"'

# Planned exact-key pressure/recovery test and existing replay regression (FIND-3).
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_verification_routes::staging_refusal_recovers_the_same_enqueue_identity) | test(=pg_verification_routes::enqueued_executions_replay_conflict_and_read_back)"'

# Exact existing failure-driven runtime adaptations if their diagnosis must be re-established (FIND-6).
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server::verification=debug mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_runtime::every_tenant_is_claimed_in_the_first_round) | test(=pg_verification_runtime::two_hundred_released_runs_complete_on_their_first_attempt) | test(=pg_verification_runtime::the_runner_claims_no_more_than_its_execution_slots)"'
# The reported TS fixture failure occurred only in the full file; the single selector cannot validate that diagnosis.
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && WYRD_LOG=info mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts'
```

Build native testing dependencies through the existing `py:setup`, `ts:build` and `ts:build:testing` tasks only when the remediation requires rebuilding them. For the expected source/test/doc write set, run repository Rust formatting/lints and changed-language format/lint checks (`mise run fmt`, `mise run lints`, `mise run py:format`, `mise run py:lints`, `mise run ts:format`, `mise run ts:lints`). Do not rerun codegen, SQL tenancy, package-wide suites, broad gate, benchmarks or live storage unless the actual correction write set newly requires them. Rustdoc/architecture prose needs source consistency proof, not invented runtime tests. Report each executed command and nonzero selection honestly.

## Rejected claims and corrections — preserved disposition evidence

These rejected extensions are omitted from the actionable ledger; they are not optional advice:

- **REJECTED:** ordinary production shutdown concurrently calls the same outbox. Inspected server/fallback teardown is sequential. FIND-1 relies on an existing documented generic method guarantee instead.
- **REJECTED:** unchecked shutdown interleaving necessarily permanently corrupts final counters. The two modular updates can cancel if both complete; the checked panic and split ownership are the proven defect.
- **REJECTED:** the passing progress stories prove ongoing A request offers. An unfinished held A1 proves activity only; the source contains no further offers while B waits.
- **REJECTED:** missing progress proof establishes starvation or justifies strict fairness/new scheduler. It establishes a required coverage gap only.
- **REJECTED:** retrying the same Run means retrying the same enqueue key, or current production replay is proven to duplicate executions. The UUID producer disproves the first; deterministic SQL identity/replay source gives no proof of the second.
- **REJECTED:** closing fixed-key proof requires new SDK key parameters or standalone grouped array-element semantics. Existing HTTP key support and durable acceptance are sufficient; grouped proof remains deferred by task authority.
- **REJECTED:** missing Errors/Panics sections are non-blocking merely because generated public error documentation is current. AGENTS §16 explicitly makes the touched rustdoc obligation blocking.
- **REJECTED:** reconcile normative docs by restoring unbounded audit retention, adding another write path or refusing authorization on audit overflow. Those corrections violate TASK-003/non-blocking audit constraints.
- **REJECTED:** absence of diagnostic artifacts itself proves the 70/200 setup or provider-scope fixes are wrong, or requires deleting the new cap. Complete source supports preserving those corrections; the retained finding is evidence-only.
- **REJECTED:** a newly written report can claim the required independent pre-fix process occurred historically. Recover actual artifacts or honestly record fresh validation and the historical limit.

## Verification and immutable-source limits

This validator performed source, diff, caller, authority and packet-inventory inspection and HEAD checks. It did not execute runtime tests, lints, codegen or SDK/server suites, and did not reproduce the shutdown race. Independent discovery reviewers recorded six exact runtime selectors passing; their actual runs remain their evidence. Implementation-reported wider green results remain claims, not independently reproduced by this validator. Those green tests do not cover the newly identified shutdown schedule or the two missing journey seams.

HEAD remained `9ffdf753ab11ce9b638c906388d3690a6aede249`. Only this `findings-validation.md` was written. No production source, tests, configuration or Git state was mutated.

Skipped broad reruns, benchmarks, live infrastructure, new APIs and scheduler changes. Risk: final acceptance remains blocked by specification identity independently of the six bounded corrections.

## Superseding evidence and owner-authority note — 2026-10-17

Steven has explicitly authorized the current `spec.md` TASK-003 text for this review, excluding revision 14's draft pre-release schema addition. The earlier authority BLOCKED result is resolved for this bounded review, without approving that excluded addition. Findings 1–5 are unchanged; **FIX_REQUIRED** is now the applicable source/evidence result.

New verbatim [diagnostician reports](diagnostician-reports.md) have been independently reassessed in [evidence-reassessment.md](evidence-reassessment.md). Preserve FIND-TASK-003-6 as REVISED: task diagnoses 3 (TS fixture) and 5 (runtime cap) are closed; diagnoses 1 (Rust admission), 2 (Python neighbor/TS key) and 4 (Clippy test futures) still need inspectable independent reports. The reassessment supersedes the initial blanket missing-report statement and its closure scope. Historical discovery reports remain unchanged.
