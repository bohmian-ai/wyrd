# TASK-001 independent behavior review

## Subject and assigned boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
- Base: `66f44aee193077ef3565aa76140a942d8eb593f9`.
- Candidate: `bf438a8d7985aadd29db3beb73ed0269c2b2f7ca`, 19 commits.
- Contract: `changes/active/bifrost-write-throughput-and-retention/spec.md`, revision 1; original task `tasks/TASK-001-fence-lifecycle.md`; maintainer-directed R1 at `review/TASK-001-maintainer-001/TASK-001-R1-bifrost-only-readers.md`.
- Primary scope: REQ-001–003 application consumers, R1, SQL claim-history projection, adjacent runner/inspection behavior and general repository rules. Separate specialists own safe recovery/frontier and cleaner/SQL/admission. This report does not replace their coverage.
- Source stayed at the candidate throughout this review. No production/test files changed and no Cargo command ran in this review.

Read root and `wyrd-sql` AGENTS, task/spec/R1, workflow, maintainer style, testing, relevant Bifrost and verification architecture, benchmark rules, cumulative diff and affected callers. CodeGraph was used before locating source. Reports from other reviewers were not used to reach these conclusions.

## Navigation and observed behavior

`InputResolver::latest_state` (`verification/inputs.rs:280–298`) selects the newest visible checkpoint with `ORDER BY sequence DESC LIMIT 1`; `RunState::accepted` (`state.rs:628–640`) decodes it, returns fresh state on no readable row and preserves errors as unavailable. Both runner recovery and inspection share this owner. `runner.rs:716–760` skips recovery only when the actual claim SQL proves no preceding lease existed; otherwise it admits recovery as Verification work and defers on refused capacity. `inspection.rs:1110–1127` makes the same optimization from the queue status, but still reads retained Bifrost state when the queue row is gone.

The proof behind `claimed_before`/`claimed` is sound in the inspected owner: claim alone assigns `lease_token`; release/retry/park/settle do not clear it. SQL uses `TenantConn` and PostgreSQL coordination time, adds no schema or extra statement, and returns columns by name. `pg_verifier_executions::a_released_execution_still_reads_as_claimed` checks both status and reclaim after an attempt refund, which prevents the tempting but incorrect inference from `attempts` or `status`.

Approval decisions, deferred evidence and GitHub outcomes read through the existing `InputResolver`, without querying receipts. The shared receipt re-export was deleted. Their durable write identities and outbox acknowledgement waits remain in the existing owners. A missing row cannot overwrite a recent held receipt: the second write conflicts or retries at Scribe. `runner::conclude` waits for `persisted` before emitting its terminal transition/result; the Stale path retries. This protects terminal settlement from a refused stale checkpoint. This does not establish the old promise of exact accepted recovery before computation; see the authority conflict below.

## Assigned requirement coverage

| Requirement / scenario | Source and executable assertions inspected | Assessment |
|---|---|---|
| REQ-001 / lasting history | Shared client enqueue and Scribe acknowledgement protocol are unchanged in this consumer portion; approval/evidence waits remain outbox reserve plus ACK. Runner terminal custody still calls `persisted` before settlement. Domain helper `expire_receipts` first flushes real Bifrost publication and asserts actual SQL removals. Existing transport/recovery coverage is assigned to specialists. | PASS for assigned portion, conditional on specialist coverage. |
| REQ-002 / permanent history vs retry expiry | Domain consumers no longer use receipt existence as lasting history; original decisions and times are read from Bifrost after deletion. SQL/cache expiry and cross-pod authority are specialist scope. | PASS for assigned portion, conditional on specialist coverage. |
| REQ-003 / execution revisions | `execution_recovery_outlives_its_revision_receipts` accepts real state, publishes, removes each receipt, and checks recovered state equals accepted state; both it and `execution_recovery_requires_exact_accepted_state` insert an invisible next receipt, recover the newest readable state, change it and assert `persisted()` is Stale. `restart_recovers_an_acknowledged_unpublished_checkpoint` retains WAL-only coverage. | Implemented and asserted against amended REQ-003; UNRESOLVED against owning architecture. |
| REQ-003 / approvals | `pg_approvals::a_decision_outlives_its_retry_receipt` drives approval to completion, publishes/deletes receipts, checks unchanged result, actor/time, identical decision replay and exact contradictory-decision conflict. An invisible held receipt reads pending and another write answers unavailable. | Implemented and asserted against amended REQ-003; UNRESOLVED on absent-vs-unavailable authority. |
| REQ-003 / evidence | `pg_deferred_evidence::accepted_evidence_outlives_its_retry_receipt` checks replayed evidence ID/time after deletion, exact conflicting-content code, and consumption by a later PR. SELECT revocation around a replay proves the reader does not touch receipts. A fresh publication is intentionally excluded from that assertion because Scribe itself still needs receipts. | PASS for retained-visible history and failed-read propagation; visibility-miss policy subject to authority clarification. |
| REQ-003 / GitHub | `pg_github_delivery::a_recorded_outcome_outlives_its_retry_receipts` checks identical result/outcome and unchanged outbound request count after publication/deletion, then serves the outcome with receipt SELECT revoked. Existing lookup-before-create remains unchanged. | PASS for retained-visible history; visibility-miss policy subject to authority clarification. |
| Adjacent claim/capacity/query-budget behavior | Claim-history integration assertion described above; `connected_work_waits_out_pressure_with_its_own_attribution` starts the status read under pressure, observes actual Bifrost wait, then releases pressure and requires completion. Completed lookup keeps its existing 2-query/4-file budget. Source-outage test now fails a real internal Bifrost query rather than revoking unrelated receipt access. | Credible assertions and recorded execution. |

R1 records GREEN execution for 139 server integration tests, 15 lifecycle journeys, 35 verification-runtime journeys, 41 queue SQL tests and 12 unit tests, plus clean format/lints. Original task records broader owner runs. These are committed implementation execution records inspected against assertions, not independent reruns. The report does not claim those commands ran during review. The task omits exact per-test selectors in parts of its implementation evidence despite its own instruction to record them; this is execution-record discipline, not proof that named tests were skipped (the recorded module filters include them).

## Material authority conflict requiring maintainer disposition

This is a contract clarification, not a prescribed code remediation or a finding demanding renewed receipt reads.

1. `architecture/bifrost-design.md:733–735` requires a recent fenced acceptance whose row no query sees to be **unavailable, never absent**. The latest spec REQ-003 instead expressly permits absence and relies on fixed write IDs. `ApprovalDecisions::decided`, `DeferredEvidence::accepted`, `GithubDeliveries::recorded` and `RunState::accepted` implement that new behavior.
2. `architecture/wyrd-design.md:2065–2067,2117–2123` requires exact accepted recovery before engine start, the fenced prefix on queued redelivery, and explicit unavailability instead of absence. The new `latest_state` query cannot detect a newer accepted invisible revision and deliberately resumes an earlier state, or fresh state if none is visible. The two lifecycle assertions deliberately changed to require this behavior. This is observable preparation/recovery behavior, even though the fixed identity prevents overwriting accepted state.
3. R1 itself says the changed assertions and REQ-003 wording are pending approval; the spec still identifies revision 1 with its original approval date, and its acceptance mapping and task scenario 1 retain the former unavailable-on-miss sentence. These stale records support locating the conflict but are not alone proof the maintainer never approved it elsewhere.

The latest user summary describes R1, but does not unambiguously settle this architectural contradiction. Current explicit maintainer approval, if obtained by the orchestrator, takes precedence. Without it, behavioral acceptance cannot choose between the materially different contracts: return `SPEC_REVISION_REQUIRED`. If the new behavior is explicitly approved, reconcile the owning documents and derivative packet text rather than introducing parallel promises. `verification/evidence.rs:7–9` also still claims receipt bridging and unavailable-on-miss despite its `accepted` method explicitly doing the reverse; include that bounded required contract-doc correction when reconciling the approved behavior.

No independent implementation defect is proposed within this primary consumer boundary beyond that unresolved contract. Do not turn stale wording into extra product requirements or restore receipt coupling simply to satisfy this report.

## Added/moved wait inventory for the primary boundary

| Operation | Change and governing sanction |
|---|---|
| Recovery's PostgreSQL receipt probes | Removed, including the linear fenced-prefix walk and approval/evidence/delivery probes. No replacement request-side SQL is added. |
| Recovery/status Bifrost state read | Exact-row query becomes one ordered latest-readable checkpoint query; previously no receipt meant no state read, whereas retained receipts are no longer required. R1's explicit Bifrost-only read decision and REQ-003 authorize the consumer query, subject to architectural approval above. First-claim/pending status optimization avoids an unnecessary read. Oracle's normal memory admission, memtable/Parquet/object reads remain inside the existing query owner. |
| Recovery workload admission | Added `as_one_operation` around recovery with an immediate deadline; refusal defers without computing. Existing execution/admission policy in `wyrd-design.md:2045–2054` sanctions refusing work before reads. This is a local resource wait/refusal, not additional PostgreSQL/Scribe/Vault work. |
| SYSTEM read authority resolution | Existing reader construction is reused for resumed executions; first claims now skip it. Permission checks/cold authority loading remain the sanctioned existing authority mechanism. |
| Approval/evidence/GitHub publication ACK | Unchanged existing outbox reserve and Scribe acknowledgement. Bifrost architecture's approval/evidence publication contract sanctions these waits. No new ACK-path SQL/fsync/cleanup wait appears in this consumer diff. |
| State/result terminal ACK | Unchanged background `persisted`/delivery waits sanctioned by `wyrd-design.md:2101–2106`. |

No new Vault wait or client/pool/runtime rebuild was found. Frontier persistence, cleaner/replay SQL and startup lifecycle waits belong to specialists; combine their inventories for whole-task coverage.

## Performance and process evidence

Inspected through the supported `capacity report` reader: `target/benchmarks/bifrost/writes-parquet/20261121T180126Z-31af0fe55.json`, `hot/20261121T180910Z-31af0fe55.json`, and `parquet/20261121T180428Z-31af0fe55.json`. All are VALID diagnostic subsets with dirty override at commit `31af0fe55`; the records do not establish a clean full qualification of `bf438a8d7`. Their recorded diff digest is `67fc5d1a11563bb3ec78e673b9647ebb0d82c4f6b90dd7553ffda06bef8e05f0`; correspondence to final R1 was not independently proven here. No benchmark was rerun or reinterpreted with a custom parser. The user's current summary explicitly leaves the measured hot-read regression open by maintainer decision; it is not retained as a newly discovered defect.

Task README and spec place clean full writes/hot/Parquet/mixed qualification at final CPU-task completion and describe these subsets as intermediate progress. The shared memory preference requires clean full benchmark evidence before hot-path PASS; record the current task-specific deferral/exception and avoid claiming the subset closes that full proof. This review does not claim the final CPU target or full surface qualification.

TDD is separate: the task openly records after-production tests for the connected recovery journeys, health type, cleaner Postgres cases and cleanup journey. R1 records expected RED for changed recovery/read assertions; step 5 predates R1. Those documented TDD violations persist even if behavioral acceptance ultimately passes. Later mutation checks strengthen assertions but do not repair test-first history.

## Limits

No broad aggregate, Cargo lane or benchmark rerun; reviewed recorded execution and source assertions. Full shipped-surface and final CPU qualification remain the integrated/CPU-task scope. Final acceptance requires the independent specialists and validator plus resolution of the conflicting recovery/visibility contract.
