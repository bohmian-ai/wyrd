# TASK-001 cumulative R1 verdict

**FIX_REQUIRED** — FIND-TASK-001-1 and FIND-TASK-001-4 remain open, and FIND-TASK-001-6 is newly confirmed. FIND-TASK-001-2, -3 and -5 are closed. Original performance qualification remains independently failed/unqualified.

## Immutable subject

| Input | Identity |
| --- | --- |
| Repository | `/home/thorrester/Documents/GitHub/wyrd-worktrees/bifrost-source-consistency` |
| Base | `1c5edf72958af8f47c63eb8ce39eb316672e170a` |
| Candidate | `339c9b8d0484421b5fd67b71194eed8fcc60cadb` |
| Prior reviewed candidate | `0d3223696f091fb16bb2e0ec36040a249d39b61f` |
| Full subject | Complete base...candidate range, 78 changed paths including prior review artifacts |
| Latest fix locator | Prior candidate...candidate, five local commits |
| Approved spec | `changes/active/bifrost-source-consistency/spec.md`, revision 2 |
| Original task | `changes/active/bifrost-source-consistency/tasks/TASK-001-source-consistency.md` |
| Prior verdict / ledger | `review/task-001-0d3223696-20261021/{verdict,findings-validation}.md` beneath the active change |
| Remediation reviewed | Prior review directory's `TASK-001-R1-retained-capture-lifecycle.md`, including appended evidence |

HEAD remained the pinned candidate throughout review, and the tracked source remained unchanged. Only this new review directory was added. No implementation, Git staging/commit, push, merge or deployment occurred. This review audits original acceptance, not only the latest fix diff or the implementation summary. The task now correctly names revision 2 and remains in progress.

## Independent evidence and follow-up

| Role | Report | Discovery result |
| --- | --- | --- |
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL |
| System resilience | [system-review.md](system-review.md) | FAIL |
| Concurrency | [domain-review-concurrency.md](domain-review-concurrency.md) | FAIL |
| Durability/persistent data | [domain-review-durability.md](domain-review-durability.md) | FAIL |
| Security/tenancy | [domain-review-security-tenancy.md](domain-review-security-tenancy.md) | PASS for its domain |
| Focused accounting follow-up | [followup-review.md](followup-review.md) | RESOLVED: persistence edge omitted |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; IDs 1, 4, 6 retained |

Fresh discovery agents received the same [subject/navigation map](subject.md), original/prior/R1 authority and immutable ranges, without another current reviewer's conclusions. [Claims comparison](claims-comparison.md) records grouping and routing. Follow-up was required because complete FIND-1 closure assertions conflicted with source-traced persistence omissions. It resolved the disagreement: capture/read/stream propagation is correct, while persistence conversions lose the same owner. The mandatory fresh validator independently proved that conclusion and every other proposal; agreement was not proof.

All required reports are complete. The validator preserves IDs for residual causes and assigns ID 6 only to the newly confirmed recovery defect. The new test-module rustdoc omission continues the cumulative documentation gate under ID 4; the five original method sites themselves are fixed. The orchestrator writes this verdict and R2 task from that independently validated ledger, without adding findings.

## Retained findings and prior closure

Full source traces, affected siblings, correction alternatives and focused proof are in [findings-validation.md](findings-validation.md).

| ID | Disposition | Evidence and consequence |
| --- | --- | --- |
| FIND-TASK-001-1 | OPEN, CONFIRMED / INCORRECT | Redux `scribe/persistence.rs:485,528,557,568,581`: `ImmutableGeneration` shallow-copies Arrow without the original charge; its frozen snapshot starts an unrelated empty owner. Normal/owned-replay completion can retire the memtable while these payloads remain alive, returning memory capacity prematurely. Query-facing ownership is fixed, so this is a narrowed residual of the same cause. |
| FIND-TASK-001-2 | CLOSED | `tail_rpc.rs::retain` installs before timer activation and refuses elapsed installation; `RetainedCapture.deadline` and `open_capture` enforce original expiry before projection. Readers already opened retain their own resources. |
| FIND-TASK-001-3 | CLOSED | Failed-acquisition and final-route Drop cleanup carry the original deadline to the shared server discovery release, which bounds its RPC join with `timeout_at`. |
| FIND-TASK-001-4 | OPEN, REVISED / VIOLATION | Original five method contracts are fixed. Newly added server `oracle/tail_discovery.rs:440–441` test module lacks required module rustdoc under actual AGENTS §16. Batch this correction with runtime work. |
| FIND-TASK-001-5 | CLOSED | All cited current architecture/product/diagram/generated statements and dispatcher/terminal guidance now distinguish listing degradation from fatal accepted-capture loss and remove publication-duplication permission. |
| FIND-TASK-001-6 | NEW, CONFIRMED / REGRESSION | Redux `scribe/file_list_writer.rs:553–563` requires a revision row after fully validated exact replay. The migration creates no revision rows for older file-list entries. Recovering a surviving committed publication manifest can therefore fail forever and keep Scribe unready. |

The validator confirmed BEH-R1-001, INV-R1-01, M-R1-001, CONC-R1-001, DUR-R1-001 and FOLLOWUP-R1-001 as one FIND-1 cause, correcting the fixture provenance shorthand. REPO-R1-001 maps to FIND-4, and unique SYSTEM-R1-001 maps to FIND-6. No optional import/placement cleanup, broader pre-existing SQL redesign, extra security mechanism or speculative performance optimization is retained.

## Reconciled original acceptance

PASS means task-scoped source plus available proof, not a claim that every original suite was rerun in this review. The independent ledger contains the full matrix and sources.

| Obligation | Implementation and verification evidence | Result |
| --- | --- | --- |
| REQ-001 / AC-001 exact rows through source changes | Coherent capture and exclusive registry selection; recorded exact-row, rollover and staged-reader proof | PASS |
| REQ-002 acquire and validate before output | Oracle stale-cut retry before execution/admission; recorded invalid-capture/transition proof | PASS |
| REQ-003 recovery / INV-004 replay-exact revisions | Transactional trigger/counter ordering and uncertainty marks remain; valid pre-revision committed recovery fails | FAIL — FIND-6 |
| REQ-004 / AC-003 preserve shared group reads | Same Oracle reader and recorded late-evidence proof; integrated first-class checklist remains change-review work | PASS at task scope |
| REQ-005 / AC-004 one current protocol and correct current docs | Fresh execution-time acquisition and old pre-row fallback removed; authored/generated guidance corrected | PASS |
| INV-001 server ownership | Existing Oracle/Scribe/Vala SQL owners extended; no caller workaround | PASS |
| INV-002 capture authority | Query/table/epoch/digest/monotonic deadline checks at owner; recorded authority/expiry proof | PASS |
| INV-003 ingestion continues | Brief local guards only; no append SQL/global or remote barrier | PASS |
| INV-005 invalid/lost sources cannot complete successfully | Stale attempts discarded, uncertainty refused, accepted loss fatal; recorded negative matrix | PASS |
| INV-006 / AC-002 governed retained lifetime | Expiry and cleanup fixed; persistence payload can outlive its charge, and valid older recovery fails | FAIL — FIND-1, FIND-6 |
| INV-007 / AC-005 accurate performance judgment | Real acquisition cost, samples/errors/refusals/headroom and inclusive boundaries preserved | PASS for measurement integrity |
| INV-008 / AC-007 warm no-SQL / stale repin | In-memory comparison and generation-safe eviction; recorded warm and one-repin proof | PASS |
| REQ-006 / AC-006 actual final qualification | Earlier runtime reports fail absolute targets; R1 has no final capacity sequence | FAIL — independent acceptance blocker |
| Repository changed-item rustdoc | Original five method docs corrected; new module omitted | FAIL — FIND-4 |
| Non-goals, tenant/peer/audit/integrity and reader safety | No extra reader/history/WAL/selector/recovery service; existing checks/leases/reconciliation retained | PASS |

## Verification and qualification limits

The orchestrator reran all four new focused regression tests sequentially on this candidate with tracing enabled: the charged-retirement proof, both elapsed-capture proofs, and the real mTLS stalled-release proof. **Four selected tests passed.** Exact commands, counts and run IDs are in [verification.md](verification.md). Base-to-candidate `git diff --check` also passed. The charge test does not construct the real persistence payload; its passing result does not close FIND-1. The release test's watchdog is looser than the query deadline and its second case calls the shared release directly; source tracing confirms the actual last-route Drop deadline propagation.

Other R1 owner/module/static results were assessed as recorded evidence, without broad reruns. Discovery/follow-up/validation were static and made no source changes. No benchmark, broad aggregate, cloud probe or integrated SDK/agent suite was run. No new regression was reproduced dynamically in this review; the validator proved the reachable source paths and specified focused closure proof.

Actual retained qualification at `../bsc-rev2` identifies `05186c1e443096e6c33d6b5f08831b613fa2caa2`, with write exit 0 and verification/hot-query/Parquet-query exits 1. Hot selective p99 is 2.352 ms, Parquet 2.745 ms versus ≤1.000 ms; direct verification at 64 clients reports the 0.9 ms overhead bucket versus ≤0.5 ms. Baseline comparison does not waive these limits. R1 changes runtime code and no new final capacity sequence is supplied, so the older build cannot qualify this candidate.

The performance obligation remains failed separately from the three bounded findings: headline measurements do not establish a sufficient optimization. No new performance FIND or invented optimization task is added. Original investigation and final comparable write → verification → Bifrost query qualification remain required, with query last after affected checks. Meeting the unchanged targets does not require a specification revision on current evidence; choosing different targets would require explicit approval. This review neither relaxes them nor claims R2 can close them.

## Remediation route

Route [TASK-001-R2-persistence-charge-and-publication-replay.md](TASK-001-R2-persistence-charge-and-publication-replay.md) directly to `$wyrd-implement` for FIND-TASK-001-1, -4 and -6 under approved revision 2. It fixes one shared accounting conversion, one initial-state replay failure and the local rustdoc omission without new machinery or a broad suite. It does not authorize implementation during this read-only review turn. Subsequent review must retain the cumulative original task, both prior verdicts, R1/R2 evidence and the still-open capacity obligation.
