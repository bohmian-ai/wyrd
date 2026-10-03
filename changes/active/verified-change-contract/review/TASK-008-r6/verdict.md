# TASK-008 round-six closure verdict

## Immutable subject

- Approved specification: `changes/active/verified-change-contract/spec.md`,
  revision 57 (`approved`).
- Original task:
  `changes/active/verified-change-contract/tasks/task-008-closeout.md`.
- Prior review:
  `changes/active/verified-change-contract/review/TASK-008-r5/`.
- Prior remediation:
  `TASK-008-CLOSEOUT-R4-audit-handoff-and-owner-shape.md`.
- Base: `1d05642bf2c4d824de1aec27286048ec79b37e25`.
- Candidate: `f8d6945041467d52020024d311ce8831a45ff4a1`.
- Reviewed range:
  `1d05642bf2c4d824de1aec27286048ec79b37e25..f8d6945041467d52020024d311ce8831a45ff4a1`.

The candidate remained unchanged throughout review. The checkout has no
`.codegraph/` directory, so reviewers used Git, `rg`, and direct source
inspection. This closure review is limited by caller direction to whether
`FIND-TASK-008-CLOSEOUT-16` and `FIND-TASK-008-CLOSEOUT-17` are closed and
whether the remediation range introduces a regression. Earlier accepted code
was not reopened except where it makes the benchmark's PASS/FAIL result false.

`FIND-TASK-008-CLOSEOUT-13`, the complete unmodified default capacity run,
remains deferred to integration and supplies no empirical qualification here.
The intermittent
`verification_runtime::two_bindings_share_one_client_observation` failure is
on an untouched path, remains separately tracked for the integrated branch,
and is not counted against this range.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-16: replica-set shutdown belongs to an existing lifecycle owner while preserving blocking, order, error, logging, and cancellation behavior | `Benchmark::clean_up` invokes inherent `Deployment::stop_replicas`; the former free workflow is deleted; the method retains newest-first traversal, `spawn_blocking`, per-ordinal logs, error conversion, result reversal, and `LocalServer` ownership | Source trace; compatible-host R4 slow-stop and complete-capacity results; round-six non-environment capacity target | **PASS — CLOSED** |
| FIND-17 pre-commit owner is observable | `OracleQueryAudit::stage` increments the real pending owner and `audit_outbox_pending` before enqueue; full/closed refusal reverses both; the writer releases both only after commit or counted loss | Held-chain-head proof covers decisions already pending before the scrape; arithmetic test passes | **PARTIAL** |
| FIND-17 pending-to-durable handoff cannot false-pass | `Deployment::drain` takes one pending scrape before `Queue::backlog`; staged rows above `published_seq` are counted without the old stop-time cut | Follow-up traced a queued Drift run that can first stage its Oracle decision after that sole scrape, settle terminally before the SQL snapshot, and leave no staged row yet; the current proof starts its decisions before the scrape | **FAIL — OPEN** |
| FIND-17 durable unpublished work survives a post-stop commit | `Queue::backlog` counts every `vala.audit_staging` row above its tenant watermark, including rows committed after `stopped` | Held-chain-head proof asserts a late-created row remains nonzero until `AuditPublisher` advances the watermark | **PASS** |
| Range introduces no repository-rule regression | Production ownership, async/blocking, tenant, audit, metrics, and durability rules otherwise remain intact | The new Postgres/live-server proof is placed in ordinary `mod tests`, contrary to the required `mod pg_tests`/`pg_*` boundary | **FAIL — FIND-18** |
| Adjacent workload, SLO, report, deadline, publication, and process behavior remains unchanged | Range changes only audit pending evidence, capacity drain composition/proof, shutdown ownership, and matching architecture/remediation text | Capacity non-environment target passed 15/15 with 5 environment tests skipped; `cargo fmt --check` and `git diff --check` passed | **PASS within verification limits** |

## Independent review results

| Review | Result | Reconciliation |
|---|---|---|
| Behavior | PASS | Revised by the focused producer-race trace. |
| Invariants | PASS | Local gauge and watermark invariants stand; the composed observation interval is incomplete. |
| Repository standards | FAIL | Test placement confirmed as FIND-18; proposed missing `# Errors` finding rejected because stop failures are returned as per-replica data rather than an outer operation failure. |
| Maintainer | PASS | No separate maintainer finding. |
| System resilience | PASS | No crash, shutdown, or availability regression; its audit-drain conclusion is revised by FIND-17. |
| Durability domain | FAIL | Proposed the reachable false-zero interval retained under FIND-17. |
| Concurrency domain | PASS | Revised because it assumed all step-owned audit producers finish before drain; queued verifier execution does not. |
| Process-lifecycle domain | PASS | FIND-16 is closed with no process-lifecycle regression. |

## Follow-up decision

A focused follow-up was required because the durability review identified a
reachable producer after the sole scrape while the other reviewers concluded
that scrape-before-SQL was sufficient. The follow-up traced the actual queued
Drift request, runner, audited Oracle query, result publication, settlement,
and independent audit writer. It resolved the conflict in favor of the
durability proposal: the load driver joins enqueue acknowledgements, not
queued execution, so the between-observations false-zero path is reachable.

## Validated finding ledger

### `FIND-TASK-008-CLOSEOUT-17` — REVISED — INCORRECT

Revision-57 REQ-171 requires every step-caused audit backlog to drain within
60 seconds before a step passes. The range correctly observes decisions that
are pending at the pre-query scrape and every committed unpublished staging
row. It can still miss a queued Drift run whose audited Oracle decision is
created after that scrape, whose run settles before the SQL snapshot, and
whose audit writer has not committed. The drain may then combine a stale zero
gauge with zero run and staged-audit rows and record a false PASS.

Correction: keep the pre-query scrape and durable query, but before accepting
an otherwise empty snapshot, scrape every serving replica again and require
the current `audit_outbox_pending` total to be zero. Prove the interval through
the real queued-verifier path with the audit commit held until after settlement,
then prove staging remains nonzero through publication.

### `FIND-TASK-008-CLOSEOUT-18` — CONFIRMED — VIOLATION

The new real Postgres/live-server audit-handoff proof and its helpers live in
`capacity/evidence.rs`'s ordinary `mod tests`. Repository rules require tests
that need Postgres or a live server to live in `mod pg_tests` or a `pg_*` file.
`#[ignore]` gates execution but does not satisfy that source-classification
boundary.

Correction: move only the environment-owned helpers and handoff proof into an
in-source `#[cfg(test)] mod pg_tests`, keep pure arithmetic tests in the
ordinary test module, retain the environment gate, and update the exact test
selector.

No other proposed finding survived validation.

## Prior-finding closure

| Finding | Result |
|---|---|
| `FIND-TASK-008-CLOSEOUT-13` | **DEFERRED** to integration by caller direction; non-blocking here. |
| `FIND-TASK-008-CLOSEOUT-16` | **CLOSED.** Replica shutdown is owned by `Deployment` and retains the accepted lifecycle behavior. |
| `FIND-TASK-008-CLOSEOUT-17` | **OPEN.** Pending-before-scrape and durable publication are covered, but the queued-verifier producer race can still false-pass. |
| `FIND-TASK-008-CLOSEOUT-18` | **OPEN.** The range introduced an environment-test placement violation. |

## Verification limits

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`:
  15 passed, 5 environment tests skipped.
- `mise exec -- cargo fmt --all -- --check`: passed.
- `git diff --check 1d05642bf..HEAD`: passed.
- The R4 record reports the held-chain-head proof and complete capacity target
  passing on a compatible host. Round-six attempts to start the Postgres and
  systemd-dependent proofs were blocked by the sandbox before the behavior
  under test; they do not contradict the recorded results.
- The existing held-chain-head proof does not cover FIND-17's newly validated
  queued-producer ordering, so a green rerun cannot close that finding.
- The full default `mise run bench:capacity` remains deferred as FIND-13.
- The separately tracked intermittent verification-runtime journey remains an
  integrated-branch blocker, not a finding against this range.

## Verdict

**FIX_REQUIRED**

The range closes `FIND-TASK-008-CLOSEOUT-16`, but
`FIND-TASK-008-CLOSEOUT-17` remains open and the range introduces
`FIND-TASK-008-CLOSEOUT-18`. Both corrections are bounded within the existing
capacity drain and test-organization owners and require no specification or
architecture revision.
