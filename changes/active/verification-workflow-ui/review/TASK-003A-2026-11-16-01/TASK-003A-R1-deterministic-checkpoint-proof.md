---
id: TASK-003A-R1
kind: remediation
status: implemented
spec: SPEC-verification-workflow-ui
spec_revision: 4
requirements: [REQ-015, AC-010]
findings: [FIND-TASK-003A-1, FIND-TASK-003A-2]
---

# Restore deterministic hosted checkpoint proof and complete panic contracts

Route directly to `$wyrd-implement`. This is one bounded test/documentation correction, not a redesign or production status-event change. Do not commit unless separately instructed.

## Authority and immutable context

Approved specification: `changes/active/verification-workflow-ui/spec.md`, revision 4 REQ-015 / AC-010.
Original task: `changes/active/verification-workflow-ui/tasks/TASK-003A-execution-status-events.md`.
Design: `changes/active/verification-workflow-ui/design/verify-status-events.md`.
Original review base: `43c77a219fd10554e8c8663e980788b70f1a6296`.
Reviewed candidate: `c233b75456fad4f45c772ec242a9d115b4d35c6b`.
Review authority: this directory's `verdict.md` and independently validated ledger `findings-validation.md`. Preserve original task, reports and candidate identities for cumulative remediation review.

## Diagnosis and intended outcome

Engineers must be able to trust the existing hosted fault tests: while a checkpoint is unacknowledged, the real Python operation has released compute/resources and its execution retains the required claim. This remediation makes that proof deterministic without changing production behavior.

**FIND-TASK-003A-2:** the two changed tests in `crates/wyrd/wyrd-server/tests/integration/pg_deferred_evidence.rs:621–636` and `:725–740` install two successive table-only stalls and assume the first captures running status and the second captures state. This is false for writes in the same slice. Running is staged before engine work, but the independent state writer may stage a checkpoint before the outbox's 50 ms linger ends. `ScribeSink::frames` builds separate status/checkpoint frames and `submit` polls up to eight concurrently. Native Scribe takes one table stall then releases its mutex while awaiting the barrier; another frame can pass before the test arms the second stall.

A reachable schedule lets checkpoint acknowledge while running is held. A later waiting/completed status then takes the second stall and the retained running assertion fails, or no remaining intended write enters the barrier. Valid production scheduling can therefore fail/hang required owner proof. No runtime reproduction or production integrity defect is claimed; the missing ordering edge is independently source-validated. Passing once cannot supply it. Both scripted acknowledgement fixes are sound because they hold their producer and drain running before arming.

**FIND-TASK-003A-1:** `Flusher::run` in `crates/wyrd/wyrd-server/src/verification/completions.rs:187` now calls the deliberately panic-capable test-support completion crash seam but has no `# Panics`. New `stored_cancellations` in `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs:4348` awaits the expect-based status lookup without its own panic section. Both violate the explicit changed-item rustdoc gate. Two documentation additions close this gap; no behavior rewrite is warranted.

## Selected correction and existing owners

Correct ordering in the two existing hosted fixtures using existing `WyrdTestServer` controls: `occupy_hosted_memory`, `wait_for_hosted_waiters`, and `settle_bifrost_outbox`. Hold the actual hosted operation before it runs, observe its waiter, drain the earlier running write, arm one events stall, and release the operation. Preserve real Python execution and its workspace/scratch allocation; keep all existing resource, running/claim and eventual wait/resume or completion assertions. This makes a held state write authoritative without adding a new Scribe barrier or parser.

Use the existing 64 MiB leave budget demonstrated by `pg_hosted_artifacts.rs:1542–1546`, and guard its relationship to the fixture's actual session envelope with the existing `Sandbox::envelope_bytes` capability. Both current fixtures have `monty_min_workers=0`, a tiny measure source and no code input mappings; 64 MiB admits retained request/shared Scribe work while staying below the default 256 MiB allocator-derived session envelope. Observe the hosted waiter before draining so a future configuration change cannot silently produce refusal instead of the intended wait. Leaving zero memory is invalid: retained source/globals must be admitted first.

Running is already staged before the hosted waiter can be reached. A state writer staging after the drain is harmless for this boundary: it is checkpoint custody, and the execution's single in-flight frozen revision prevents a later park/terminal revision from acknowledging past it. There is no competing execution producer in these fixtures before the assertions. Retain the real hosted operation and the existing five-minute execution limit; do not widen timeouts.

This local sequencing correction is smaller than new checkpoint-discriminating ingest machinery and leaves production ownership intact. Deleting assertions loses required proof; sleeping or serializing production frames does not correct the test's source of invalid assumptions. Do not change the two sound scripted producer/drain corrections.

Add accurate panic sections at the two named item owners. Describe the injected flush panic and failed/missing execution lookup respectively. Update the original task's incorrect lane-order diagnosis/evidence to cite the current independent reports and actual correction. Preserve the historical TDD violations and unavailable implementation-time diagnostician report; today's diagnosis is not evidence of past consultation. Correct the owner-lane chronology if restating it: 126/126 preceded a53a91094/c233b7545, with focused reruns afterward.

## Constraints and non-goals

No production status logic, SQL result, migration, queue/fence, checkpoint/recovery, Scribe batching/lane/durability, public API, endpoint, dependency or feature change. No new test harness, shared helper or checkpoint-aware stall. Do not weaken assertions, add sleeps/retries/ignores or enlarge timeouts. Preserve all accepted lifecycle, revocation/admission/PR cancellation, tenant attribution and group/fallback behavior. Do not invent historical RED or diagnostic evidence. Review reports remain immutable records; append new implementation evidence through the normal task workflow.

## Acceptance and focused proof

| Finding | Observable closure |
|---|---|
| FIND-TASK-003A-1 | Both named changed items have accurate `# Panics` sections matching existing bodies; static inspection plus format/diff check; no logic change |
| FIND-TASK-003A-2 | Each hosted fixture observes the held operation's waiter and drains earlier writes before its single checkpoint-custody stall. Original pre-ack compute/scratch/worker/model/running assertions and post-release outcomes pass. Session-envelope guard proves the leave budget creates a waiter. No double-stall ordering assumption remains |

Run Cargo-backed commands sequentially through `mise` with repository Postgres. Run each exact named selector, recording actual results and diagnosis for any failure:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_deferred_evidence::a_parked_execution_releases_compute_before_its_park_is_acknowledged)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_deferred_evidence::a_finished_execution_releases_compute_before_its_terminal_is_acknowledged)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::terminal_failures_use_one_acknowledged_settlement)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::a_parked_attempt_keeps_its_claim_until_acknowledged)"'
```

Run the narrow format/diff checks and required targeted Rust checks for this write set; do not require full owner/public-journey suites, broad aggregates or benchmarks for this task remediation. Full integrated verification belongs to change review. The original review could not run Docker/Postgres because its sandbox denied the socket; absence of dependency access is not a passing test. The already-passed shared-status outbox unit needs no rerun unless its owner changes.

## Evidence

- FIND-TASK-003A-1: PASS. `# Panics` added to `Flusher::run`
  (`completions.rs`) for the armed completion flush crash, and to
  `stored_cancellations` (`pg_verifier_executions.rs`) for a failed or
  missing status read. Documentation only; format, lints and
  `git diff --check` pass.
- FIND-TASK-003A-2: PASS. Both `pg_deferred_evidence` fixtures now assert
  `Sandbox::envelope_bytes(&hosted) > 64 MiB`, hold hosted memory leaving
  64 MiB, open the PR, wait for one hosted waiter, settle the Scribe outbox,
  arm one `execution_events` stall, and drop the hold. All pre-ack
  running, scratch, worker and model-call assertions and the post-release
  outcomes are unchanged. No double-stall assumption remains.
- The four named selectors each pass 1/1. The two changed hosted tests
  then passed 5 further consecutive runs together (2/2 each).
- No production code, assertion, timeout or sleep changed. The owner lane
  last passed 126/126 before a53a91094 and c233b7545; it was not re-run
  for this remediation, per the task's scope.
- TDD: NOT_APPLICABLE for the documentation; the fixture change corrects
  test sequencing, so there is no missing-behavior RED to observe.
