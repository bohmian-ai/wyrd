---
id: TASK-003-R1
kind: remediation
status: ready
spec: SPEC-long-horizon-verification
spec_revision: 13
parent_task: TASK-003
remediates: [FIND-TASK-003-1, FIND-TASK-003-2, FIND-TASK-003-3, FIND-TASK-003-4, FIND-TASK-003-5, FIND-TASK-003-6]
requirements: [REQ-006, AC-005, AC-006, INV-001, INV-002, INV-003, INV-004, INV-005]
depends_on: []
---

# Close bounded-outbox admission review

Execute directly with `$wyrd-implement`. This is one bounded remediation, not another planning round. Do not implement until operating as the fresh implementation agent; the review that produced this file made no production changes. Record implementation evidence in this remediation task, preserving the original task and review reports. No commit is authorized by this packet.

## Contract and immutable subject

- Approved behavior: `changes/active/long-horizon-verification/spec.md`, TASK-003 text explicitly authorized by Steven, the spec owner, on 2026-10-17. The file's revision-14 draft pre-release schema addition is excluded. The revision-13 TASK-003 obligations remain the contract; no spec edit or further approval is needed for these corrections.
- Original task: `changes/active/long-horizon-verification/tasks/TASK-003-bounded-outbox-admission.md`.
- Review: `changes/active/long-horizon-verification/review/task-003-9ffdf753a/verdict.md`; source findings and caller traces in `findings-validation.md`; new diagnostic evidence and its reassessment in this directory.
- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/long-horizon-verification-task-003`.
- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Reviewed candidate: `9ffdf753ab11ce9b638c906388d3690a6aede249`, branch `change/long-horizon-verification-task-003`, complete `git diff ef3a57b07...9ffdf753a` (commits `2348d34d8` through `9ffdf753a`). Locations below refer to this candidate.
- Authority: AGENTS.md; architecture/agent-rules.md; architecture/references/languages/{spec-driven-development,implementation-execution,maintainer-style,testing-workflows}.md; applicable Rust, error, Python/TypeScript, security, Wyrd/Bifrost and operations references routed through architecture/references/README.md. Follow CodeGraph before code navigation.

The intended result is the same bounded source admission, non-blocking audit and existing durability boundary, with correct preserved shutdown ownership and credible proof of the required pressure/progress/replay seams. Findings 4–5 are batched documentation corrections. Finding 6 is evidence closure only and does not authorize undoing source-supported fixes.

## Diagnosis and selected correction boundaries

### FIND-TASK-003-1 — Pair abandoned-work settlement under the existing shutdown owner

**Violated obligation:** preserve existing shutdown/loss semantics and correct whole-pending accounting. Both base and candidate `Outbox::shutdown` document supported concurrent callers, with one loss reporter. Current server teardown is sequential; the finding concerns that explicitly preserved generic-owner contract, not an alleged deployed incident.

**Evidence and consequence:** `crates/shared/wyrd-runtime/src/outbox.rs:348–350` independently consumes `pending` and `staged_reserved`; `Admission::release` at `:438–441` subtracts reserved from total. A reserved item produces both counts in `send`, retains them through retry/in-flight ownership, and can survive unwritten to deadline. After writer termination, concurrent callers can split the two counts and call `release(0,1)`, causing a checked-arithmetic panic. Ordinary writer completion and failed-send release already own their pair. Existing passing shutdown tests use one caller and unreserved items, so do not prove this state.

**Recommendation:** correct final abandoned-work ownership in the existing Outbox lifecycle. Its existing single loss-reporting winner must also exclusively own the corresponding reserved count and release; the two values must never be settled by different callers. Preserve the close fence, writer wait, unused Reservation ownership/Drop, late-stage loss and retry identity. No new lifecycle owner, lock service, coordinator, queue, saturating subtraction or removal of the concurrent-call contract. Local implementation mechanics remain implementation-owned.

**Proof:** add one focused inline reserved-work/concurrent-shutdown regression that proves no panic, exactly one returned loss total, once-only lost count and final pending/share settlement; independently held unspent reservations remain owned until dropped. Preserve successful drain and deadline-loss regressions. Source inspection must establish exclusive paired settlement even if a test cannot force every scheduling interleaving. Do not use synthetic host load or raised thread counts.

### FIND-TASK-003-2 — Prove progress while traffic continues to be offered

**Violated obligation:** Scenario 3 requires accepted/runnable B to finish while A continues offering calls, when resources and dependencies permit; AC-005/006 require pressure/progress and drain proof.

**Evidence and consequence:** the complete `verification_staging_refuses_and_recovers` stories in Rust `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:1228–1261`, Python `sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py:478–503`, and TS `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:449–467` all await B before offering A2 and refusal traffic. A held first call proves activity, but no A requests are offered while B waits. Supporting sink-isolation and pre-seeded queue tests do not exercise this producer seam. This is a proof gap, not a demonstrated starvation defect.

**Recommendation:** extend these existing three SDK stories with finite, controlled normal A requests overlapping already-admitted/runnable B work and its completion, including offered pressure. Demonstrate that B completes before A stops offering traffic, then stop A and reconcile/drain accepted work against accepted/refused responses. Reuse the existing real server, language SDK, local judge and assistant fixtures. The retained production admission/tenant-round owners need no redesign to close missing proof.

Preserve the resource-availability qualifier. Do not demand B admission after every slot is occupied or B completion while its dependencies are blocked. Do not add strict fairness, latency guarantees, a new load harness, benchmark, random sleeps or platform load. Retain typed refusal/timing, no-source-row-on-refusal and recovery assertions in each language.

### FIND-TASK-003-3 — Prove pressure recovery with the same enqueue identity

**Violated obligation:** AC-005/J-005 step 2 and Scenario 3 require same-identity recovery/replay after drain, without duplicate execution.

**Evidence and consequence:** Rust recovery at `verify_in_real_time.rs:1295–1308` and the Python/TS mirrors reuse a Run. However `crates/shared/wyrd-client/src/verify/mod.rs:444–450` generates a fresh UUID key on every Target::enqueue invocation. Transport retries within one invocation keep a key; a later submit does not. Current journeys prove fresh-key recovery. The existing `pg_verification_routes::enqueued_executions_replay_conflict_and_read_back` proves stable-key replay without prior staging pressure; neither alone proves the composed seam. No production duplication defect is established.

**Recommendation:** extend proof at the existing real-server HTTP enqueue/replay owner in `crates/wyrd/wyrd-server/tests/integration/pg_verification_routes.rs` (explicit-key helpers at `:159–200`, replay test at `:381` onward). Refuse a fixed body/key/authority tuple under staging pressure, prove that refusal staged no source row, drain pressure, retry the identical tuple successfully, then replay it and prove equal identity plus one durable queued execution. Reuse current `Idempotency-Key`, Submission/RequestKey derivation and `VerifierExecutionQueue` replay owner; no production identity change is required by this coverage finding.

Do not add a public SDK key option, producer SQL dependency, synchronous queue commit, durable receipt or outbox. Do not impose new accepted-replay source-row deduplication semantics. Durable execution cardinality is the standalone proof; grouped array-element cardinality remains the expressly deferred TASK-001 integration seam.

### FIND-TASK-003-4 — Complete touched Rust failure/panic documentation

**Violated obligation:** AGENTS.md §16 requires substantive documentation of materially changed Rust items, including private items/tests, with relevant Errors/Panics contracts. This is the explicit rustdoc gate.

**Exact evidence:** `crates/wyrd/wyrd-server/src/components/verification/service.rs:453,604` omit the new staging-full error from execute/enqueue Errors sections. `crates/wyrd/wyrd-server/src/scribe_outbox.rs:652,663` constructors delegate to Outbox::new and spawn without documenting the Tokio-runtime panic precondition. New tests at `crates/shared/wyrd-runtime/src/outbox.rs:1069,1129` and the materially extended `crates/wyrd/wyrd-server/src/http/error.rs:410` mapper test retain assertions/expectations without Panics documentation. Generated error docs do not satisfy these item contracts.

**Recommendation and proof:** amend the existing item docs for before-source-staging refusal, the runtime precondition and assertion/setup failure conditions as applicable. Include docs for the remediation's newly changed items. Reuse nearby SubmissionSink constructor and mapper-test documentation patterns. This requires source inspection and Rust format/lint checks, no new runtime test or documentation tool. Batch it with the substantive correction; do not open a style-only round.

### FIND-TASK-003-5 — Reconcile normative audit retention promises

**Violated obligation:** durable security/operational contracts must describe approved behavior and superseded architecture must be updated in the same cohesive change.

**Evidence and consequence:** bounded unreserved stage at `crates/shared/wyrd-runtime/src/outbox.rs:227–239` counts/logs immediate overflow loss and never fails authorization. Yet `architecture/wyrd-security-posture.md:469` declares an unbounded queue, and `architecture/operations/reliability-and-recovery.md:73` exempts the Scribe outbox from bounded queues because retaining work is preferred to dropping it. Operators/security readers are promised outage retention the implementation intentionally sheds. The candidate's submission-only Wyrd-design paragraph does not reconcile those audit/capture promises.

**Recommendation and proof:** update those two existing normative paragraphs to finite process staging with logged/counted non-blocking overflow loss. Check related outbox/audit prose for the same contradiction. Preserve one Scribe writer, identical admitted retry slices, non-blocking operation/audit independence, Scribe ACK meaning, and unacknowledged abrupt-crash/deadline loss. Do not restore unbounded buffering or add an audit relay, writer or authorization refusal. Static consistency inspection suffices; no schema/pre-release addition, policy change or unrelated operations rewrite.

### FIND-TASK-003-6 — Supplied diagnostic evidence and remaining scope

**Violated obligation and locations:** the implementation skill independently triggers a fresh diagnostician for failure-driven fixes touching tests and for timing/ordering/concurrency. Original task diagnoses at `TASK-003-bounded-outbox-admission.md:235–239` initially lacked supplied reports. New `diagnostician-reports.md` is preserved verbatim; independent `evidence-reassessment.md` narrows this finding.

**Closed portions:** Report A closes diagnosis 3, the TS provider scope correction at `sdks/wyrd-sdk-ts/wyrd/tests/support/server.ts:157`. Report B closes diagnosis 5, the runtime cap/test adjustments at `crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs:2668–2743,2865`. Do not recreate those reports or rerun the full TS file/two runtime scenarios solely for missing-report closure.

**Remaining evidence and consequence:** diagnosis 1 (Rust admission ordering/share split: `outbox.rs` Admission/Outbox::reserve and server direct/runner reservations) says a report existed but was not retained. Diagnosis 2 (Python neighbor baseline/key, mirrored TS key) has no report: `sdks/wyrd-sdk-python/tests/integration/conftest.py:275–290`, `support.py:144`, `crates/wyrd/wyrd-testing/src/server.rs:3885`, and `sdks/wyrd-sdk-ts/wyrd/tests/support/assistant.ts:74–83` (`registerAssistant(client)` and tenant-aware key issuance). Diagnosis 4 (Clippy large_futures) has no report for Rust journey `Assistant::start_on` at `verify_in_real_time.rs:76` and boxed callers at `:68,1217`. These fixes touched test helpers/bodies; absence of timing/assertion changes does not waive the test trigger. Plausible source and passing Green cannot establish missing independent cause evidence. No wrong fix, tenancy weakening or unnecessary boxing is inferred.

**Recommendation:** recover the original Rust admission report and original failing trace/compiler evidence where available. For missing reports, obtain fresh independent read-only diagnosis of admission, neighbor baseline/credential and compiler future-size causes using available failing commands/diagnostics and cumulative/historical diff evidence. Supply diagnosticians command, trace/compiler output and diff without the implementer's hypothesis. Cover related baseline/key subfailures together, affected default callers and the mirrored TS key caller. Each report identifies source-backed cause, existing correction owner, affected callers, preserved assertions and evidence limits. Label fresh validation retrospective; never claim it establishes historical pre-fix compliance. State missing historical evidence honestly. Use git source/diff inspection, not baseline checkout, bisect or old-code reruns to establish authorship. No production change or new mechanism is required solely by this finding.

Link retained Reports A/B and recovered/new reports in supplemental review evidence and the remediation evidence table beside references to the five original diagnoses. Leave the original task unchanged. The traced current-candidate SDK selectors below provide shared Green/consumer proof without repeated execution; focused compile-only Clippy covers diagnosis 4. Current passes alone do not replace independent cause reports. Acceptance requires inspectable reports for diagnoses 1/2/4 with honest temporal/provenance limits, while 3/5 remain closed.

## Preserved behavior, write set and non-goals

Expected correction owners are shared-runtime outbox, the three existing SDK pressure tests, existing server HTTP replay tests, the listed Rust documentation, two architecture paragraphs, and review/remediation evidence. Fixture changes earn scope only if necessary to those exact outcomes. Extend existing owners; no new dependency, feature, alternate admission owner, lifecycle service, transport or test harness.

Preserve finite queued/waiting/retrying/in-flight accounting; downstream submission reservation through Scribe ACK; before-staging atomic source refusal; typed pressure/timing; non-blocking countable audit/span/capture; active execution cap; existing lease/fence/idempotency and tenant boundaries; accepted crash-loss semantics; all macOS/Linux x86_64/arm64 portability rules. No producer Postgres access, distributed quota, strict allocation/fairness guarantee, synchronous durable receipt, durable audit outbox, grouped API/schema behavior or TASK-004 capacity qualification. Do not weaken tests, gates, payload validation or error handling. No unrelated code/doc cleanup.

## Acceptance criteria

| ID | Finding | Required observable closure |
|---|---|---|
| R1-AC-1 | FIND-TASK-003-1 | Supported concurrent reserved-work shutdown cannot split final settlement or panic; exactly one loss report/count; correct pending/share release; unspent reservations remain independently owned; normal drain/loss preserved. |
| R1-AC-2 | FIND-TASK-003-2 | Each real Rust/Python/TS SDK story proves B completion while A continues finite offered pressure, followed by accepted-work drain/reconciliation; refused calls stage no source rows. |
| R1-AC-3 | FIND-TASK-003-3 | Real HTTP pressure refusal, drain, exact-tuple retry and accepted replay produce one durable execution with equal returned identity; no source row from the refused attempt. |
| R1-AC-4 | FIND-TASK-003-4 | Listed touched Rust Errors/Panics contracts and newly modified items have substantive matching docs; format/lints pass. |
| R1-AC-5 | FIND-TASK-003-5 | Security/recovery authorities consistently describe finite staging and counted non-blocking overflow loss, preserving admitted retry/ACK/crash boundaries. |
| R1-AC-6 | FIND-TASK-003-6 | Accepted supplied reports remain preserved; only the independently identified residual diagnostic evidence is obtained honestly and linked, with historical limits; correct prior fixes remain intact. |

## Focused proof and execution discipline

Use RED/GREEN/REFACTOR for the shutdown behavior; extend existing focused stories for missing proof. Static rustdoc/architecture corrections need no artificial RED. Test names marked planned below must be confirmed by collection/nextest list after addition, with nonzero selection. If a local name differs, record and run its actual exact command. Diagnose unexpected failures with tracing and the `$wyrd-implement` independent-diagnostician rule before changing a test, timeout, retry, assertion or other trigger. No sleeps as synchronization, synthetic load, raised thread counts, full journey suites or broad aggregates.

If native artifacts need rebuilding after the runtime correction, use existing `mise run py:setup`, `mise run ts:build`, and `mise run ts:build:testing` once before the focused language selectors. Do not infer that an existing native binary contains the correction.

```sh
# Planned new runtime regression plus exact bound/retry/tenant/drain regressions.
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::concurrent_shutdown_releases_reserved_losses_once) | test(=outbox::tests::bounded_staging_covers_retrying_and_inflight_work) | test(=outbox::tests::bounded_audit_staging_remains_nonblocking) | test(=outbox::tests::a_retry_writes_the_identical_failed_slice_without_later_items) | test(=outbox::tests::a_failing_tenant_does_not_delay_another_tenant) | test(=outbox::tests::shutdown_flushes_items_that_recover_before_the_deadline) | test(=outbox::tests::shutdown_counts_items_unwritten_at_the_deadline_as_lost)'

# Exact existing SDK stories, extended for continued offered pressure.
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_runtime=debug,wyrd_server::verification=debug mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::verification_staging_refuses_and_recovers)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && WYRD_LOG=info,wyrd_server=debug mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_verification_staging_refuses_and_recovers'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && WYRD_LOG=info,wyrd_server=debug mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^verification_staging_refuses_and_recovers$"'

# Planned new same-key pressure test plus exact existing replay/conflict regression.
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_verification_routes::staging_refusal_recovers_the_same_enqueue_identity) | test(=pg_verification_routes::enqueued_executions_replay_conflict_and_read_back)"'

# Diagnosis 4: focused compile-only proof; an independent diagnosis report is required too.
mise exec -- cargo clippy --locked -p wyrd-ai --test integration --all-features -- -D warnings

# Required touched-language static checks for the expected write set.
mise run fmt
mise run lints
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:format
mise run ts:lints
mise run ts:typecheck
git diff --check
```

These are closure commands for the fresh implementer, not results of this review. No public schema/error/declaration or SQL capability change is needed; codegen/tenant-isolation/dependency gates are conditional only if the actual correction write set touches their protected boundaries. Architecture prose uses static contract inspection; no docs-site change is required. Keep broad suites, grouped J-005 and capacity benchmarks at their original change/integration owners.

Append a compact acceptance/evidence table for R1-AC-1..6 with exact commands, selected counts/features and actual results; distinguish static proof, tests, diagnostic reports and unrun limits. Audit the final tracked/untracked diff and non-goals. Return IMPLEMENTED with this task path and immutable diff identity for a subsequent cumulative `$wyrd-task-review`; do not approve, merge, push or deploy.
