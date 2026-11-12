---
id: TASK-010-R1
kind: remediation
status: proposed
spec: SPEC-github-verifier-pipelines
spec_revision: 7
approved_spec_revision: 7
agreement: ../design/task-010-benchmark-remediation.md
remediates: TASK-010
requirements: [REQ-015, REQ-016, REQ-017, REQ-018]
acceptance: [AC-010, AC-011, AC-012, AC-013]
invariants: [INV-001, INV-002, INV-004, INV-005, INV-006, INV-007]
depends_on: []
blocks: [TASK-010-acceptance, TASK-006-acceptance, TASK-005, TASK-007, TASK-008, TASK-009]
---

# Restore direct verification and batching without benchmark regressions

## Outcome and agreement

Direct verification again runs through the shared engine without resumable
execution state and records exactly the resource-scaling input/output/result
projections. Its records drain in destination batches rather than one output and
summary frame per result. Queued verification retains resumability and bounded
background terminal custody. The entire packet remains failed if any L1
verification, gateway or Bifrost benchmark regresses against resource-scaling.

The maintainer explicitly agreed the
[independent recommendation](../design/task-010-benchmark-remediation.md) and
requested this remediation, with **L1 measurement only**. This user amendment
governs conflicting direct-state and L2 wording in approved spec revision 7 and
Task 010. The design's "Proposed flow and implementation" carries the selected
approach; "Verified baseline" and "Diagnosis and evidence" explain its source.
No terminal-only direct checkpoint, new engine or stronger durability policy is
selected. This task is proposed, not implementation or review evidence.

Before affected production edits, reconcile the approved amendment in
`spec.md`/`contracts.md`, Task 010, and the owning verification sections of
`architecture/wyrd-design.md` and `architecture/bifrost-design.md`. Record the
maintainer's actual agreement; do not fabricate approval of additional behavior.
Preserve unrelated approved contracts. No unsettled design decision is delegated
to implementation; a newly discovered material conflict returns to design.

## Inputs, owners and delivered flow

The input is the packet's existing implementation, including the failing
`b709eaa21` benchmark candidate. It does not depend on Task 010 passing first.
Reuse the actual resource-scaling worktree's direct workflow, checked against
clean benchmark source `2ce7caec7`; do not revert the whole packet or delete
queued functionality. Existing declared Verifiers, authorization, preparation,
outbox and queue owners supply all required runtime inputs.

**Direct:** authenticated request/declarations → existing validation, target
preparation and two-slot recording admission → `VerificationEngine` without
the optional checkpoint adapter → normal owned outcome → `VerificationResults`
projection → process Scribe outbox → destination-batched background delivery →
existing result/status/details consumers.

- Remove direct `RunState`, checkpoint attachment and state-based terminal/failure
  selection. Preserve live graph execution, dependencies, Agent/code/tools,
  budgets, retries, cancellation/deadline and isolated direct capacity.
- Preserve the two logical outbox items projecting to `execution_inputs`
  (`origin=direct`, `queue_execution=false`), `execution_outputs` and compact
  `results`, with normal attribution/audit and gateway capture. No direct
  checkpoint/history, SQL execution row or final state snapshot.
- Restore direct output/summary absorption into destination frames through the
  existing `Frame::absorb` workflow. Preserve detail-before-summary ordering,
  tenant isolation and stable slice retry identities. Do not remove queued
  result keys or acknowledgements to achieve direct grouping.
- Keep baseline recording admission and staged-row preparation. Arrow/frame
  construction, durable delivery and acknowledgement remain background. Add no
  persistence wait to engine transitions or request completion; moving every
  baseline preparation allocation/serialization off-thread is not extra scope.

**Queued:** existing batched queue admission/claim → prepared `RunState` and
checkpoint adapter on the same engine → memory progress and coalesced background
persistence → compute release at terminal selection → retained bounded claim and
renewal → accepted terminal state/results → token-fenced batched settlement.
Preserve exact accepted recovery, revision/result identities, publication replay,
conflict checks, cancellation and lease-loss cleanup. A shed terminal offer never
settles the queue. Do not hold compute permits over persistence.

Use batching/group commit throughout PostgreSQL and Scribe work. Retain existing
set-based admission/claim/renewal/completion, Scribe WAL groups and tenant-batched
fence inserts. Independently fenced queued revisions keep their logical identities;
do not replace exact accepted recovery with `MAX(sequence)`. Low traffic may
produce a one-item batch; do not construct a transaction per record. No Bifrost
WAL/catalog redesign or new persistence mechanism is selected.

**Consumers and capability boundary:** direct result/status/details lookup reads
the baseline input/output/result records without accepted `ExecutionState`.
Retain `verifier:read`, authorization/audit, tenant isolation and honest
missing/unavailable details. Queued inspection uses accepted state. Keep newly
introduced resumable publication receipts/mounts and execution-history features
queued-only, using existing eligibility validation before any direct step.
Preserve ordinary baseline direct tools/file behavior. PR extraction, deferred
inputs and approval remain queued-only as already specified. Close affected
HTTP/MCP/SDK behavior, documentation and journeys without inventing new wire
shapes or compatibility paths.

Eligibility must include declared Agent `publish` tools and mounts that depend
on execution publications, using pinned preparation before graph execution.
Do not merely remove the direct state binding and let `HostedTools::open` fail
after earlier steps have run. Ordinary filesystem tools and declared baseline
input mounts must remain direct-eligible.

Existing owners: `components/verification/{service,results}.rs`,
`components/verification/service/inspection.rs`, `verification/{state,inputs}.rs`,
`verification/hosted/`, `vala-verify/src/blocks/engine.rs`, `scribe_outbox.rs` and
the shared outbox. Resolve exact private paths through current source; these
references guide ownership rather than limit complete caller/consumer fixes.
Do not add a queue, finalizer, state archive, SQL payload mirror, dependency,
feature or tuning knob. Buffer inflation or weaker loss accounting is not repair.

## Acceptance observations

| Obligation/source | Trigger → required observation |
|---|---|
| R1 — REQ-015/017, AC-010/012; approved direct amendment | Real SDK executes an ordinary direct graph with persistence acknowledgement held → shared engine selects the normal result and releases compute; no direct `RunState`, checkpoint or SQL execution record. After drain, actual baseline input/output/result records support correct result/details reads and attribution. |
| R2 — REQ-018, AC-013; approved batching amendment | Many direct outcomes reach the real projection/outbox owner → destination-grouped output/summary frames, correct row counts, detail-before-summary and stable retry behavior; no per-result singleton-frame amplification. PostgreSQL/Scribe group ownership remains batched. |
| R3 — REQ-015/017, AC-010/012; INV-002/004–006 | Hold queued checkpoint/terminal acknowledgements and fill existing claim tracking, including already-running work → running computations can finish and release compute, retained bytes/claims remain bounded, and exhausted tracking pauses new claims. While queued acknowledgements remain held and direct delivery remains available, an ordinary direct SDK call still completes. Restore queued delivery → the same terminal state/results are acknowledged before batched fenced settlement without recomputation; capacity is released. Recovery, cancellation, replay/conflict and lease loss preserve exact accepted work and authority. |
| R4 — REQ-015/016, AC-010/011; approved consumer boundary; INV-001/007 | Direct result/details reads consume the actual baseline records; queued inspection consumes accepted state. Foreign/unauthorized access stays refused. Direct graphs declaring a queued-only `publish` tool or publication-dependent mount refuse before an earlier observable step can run, not at tool-session creation midway through execution. Existing extraction/deferred/approval exclusions remain. Ordinary direct tools/file graphs still work. Exercise affected shipped surfaces at their owning journey tier. |
| R5 — REQ-018, AC-013 as explicitly amended to all three engines and L1 only | Full clean L1 verification, gateway and Bifrost on an immutable candidate → every experiment VALID, every existing target met and no latency, throughput, CPU-efficiency, memory or correctness regression against comparable clean resource-scaling records. Every repetition meets existing integrity criteria; evidence is retained with no lost writes or unexpected refusals. |

## Verification and handoff

Use existing unit owners in `scribe_outbox.rs`, the shared outbox and engine for
frame grouping, retry identity and optional checkpoint behavior. Add focused
regressions in the existing server integration and Rust SDK/server journey
targets for R1/R3/R4. Unit frame checks do not replace the SDK → real
server/repository Postgres → SDK result/details and queued recovery journeys.
Preserve negative, authorization and cancellation coverage when adapting tests
that incorrectly require direct execution state. Record actual new/renamed
selectors before running them; do not invent test names in this task.

Existing focused tests to retain/adapt and run through the managed Postgres runner:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked \
  -p wyrd-server --test integration --test-threads=1 \
  -E 'test(=pg_verification_routes::details_preserve_retained_execution_evidence) | test(=pg_verification_routes::details_report_absence_when_retained_records_are_missing) | test(=pg_verification_routes::details_preserve_visibility_and_read_authority)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked \
  -p wyrd-server --test integration --test-threads=1 \
  -E 'test(=pg_durable_verifier_work::batched_checkpoints_recover_exactly_the_accepted_batch) | test(=pg_durable_verifier_work::cancelled_work_keeps_checkpoints_and_fences_stale_writes) | test(=pg_durable_verifier_work::terminal_selection_and_inspection_use_run_state)'

mise run test:wyrd
mise run test:rust-sdk:journey
mise run test:bifrost:journey:server
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise run docs:check

mise run bench:l1:verification
mise run bench:l1:gateway
mise run bench:l1:bifrost
```

Run Cargo/benchmark commands sequentially. Add affected gateway or language/MCP
owner checks when the implementation changes those consumers; integrated change
review owns complete shipped-surface lanes and any broad aggregate. Regenerate
and run `codegen:check` only if a generated projection is affected. Record every
added/moved hot-path PostgreSQL/Scribe/object-store/Vault wait against its allowed
boundary; do not restore synchronous persistence as a performance fix.

L1 qualification requires five full experiment records from **one final immutable
candidate source revision**: verification direct-verify, gateway overhead, and
Bifrost writes-parquet/hot/parquet. If production or test source changes after
measurement, rerun all three L1 suites on the final revision; do not combine
records from before and after a fix. Compare the newest
compatible clean resource-scaling records for each experiment under
`benchmarks/history/`, preserving immutable source IDs, host/build/features,
limits/fixtures and all three repetitions. Missing compatible baselines require
an isolated resource-scaling run, not a degraded packet baseline. Keep the
existing baseline intact. Do not reset the user's database or alter the current
checkout to obtain baseline evidence; Git commits remain caller-owned.

Run on a quiet host without concurrent builds/tests/benchmarks. Smoke, subset,
profile and dirty diagnostics do not qualify. VALID/exit zero is necessary but
not sufficient: report existing performance targets and per-case baseline
comparisons. Noise, INVALID or unsettled comparisons are missing proof, not PASS.
No invented tolerance, weakened benchmark, reduced workload, hidden loss counter
or baseline reset is authorized. Any regression fails the entire packet.
**Do not run or require L2 measurement for this remediation.** Queued correctness
and recovery tests remain required; L1 does not imply L2 performance proof.

Handoff maps R1–R5 once to actual commands/results, source/baseline record IDs and
remaining limitations. Include the final diff/`git diff --check`, authority
reconciliation and independent task review. Task 010 and downstream acceptance
remain open until all required evidence passes; a proposed task is not a verdict.

Independent comprehension at the initial handoff: a bounded read-only reader checked the
spec/contracts, agreed design, this task and its supersession/dependency text.
It found no material comprehension gap, confirmed all five L1 experiments and
the L2 exclusion, and distinguished the agreed user amendment from pending
authority reconciliation. This is not implementation or performance verification.

Goal-alignment review clarified existing proof obligations: queued-only direct
eligibility before any step, direct progress with queued claim tracking full,
and all five L1 experiment records on the same final candidate revision. These
clarifications preserve the agreed behavior and add no L2 measurement or feature.
