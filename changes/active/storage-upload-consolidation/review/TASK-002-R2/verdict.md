# TASK-002 remediation review — FIX_REQUIRED

## Immutable subject and acceptance context

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Original base: `15f939fc917b22efa5e5e4d12fd1f97be807a0a7`.
Prior candidate: `5ef2981e66068e8ee5f96d3be15000eea9cbd94f`.
Candidate: `a80afd68e47d992e35f20931984d96524b2e7dbf`.

Reviewed `git diff 5ef2981e6 a80afd68e` (17 files), with cumulative `15f939fc9...a80afd68e` available as context. Requested commits `f4406862f`, `e4e9d5621`, `559349cb9`, `9a8733256`, `a80afd68e` form a direct descendant chain, with no rebasing or patch-correspondence issue. HEAD stayed at the candidate; tracked source stayed clean. Pre-existing untracked R1 and TASK-001-R2 reports remain preserved. Review writes reports and a remediation task only; no implementation, commit, merge, push or deployment. No CodeGraph index exists.

Authority: [approved spec revision 2](../../spec.md), [original TASK-002](../../tasks/TASK-002-shared-file-uploads.md), root/SQL AGENTS, applicable architecture and agent/testing guides. Acceptance context retains the [R1 verdict](../TASK-002-R1/verdict.md), all selected R1 reports, [prior stable ledger](../TASK-002-R1/findings-validation.md), and [R1 remediation task with implementation evidence](../TASK-002-R1/TASK-002-R1-file-session-lifecycle.md). [scope.md](scope.md) records the map, reopened paths, carry-forward evidence and topology.

## Round scope and independent review

This first remediation review checks three prior-finding closures, the incremental source/docs/tests, and affected path/session/inventory/reclamation consumers. It carries forward R1 source-backed acceptance for unchanged planner/dispatcher/completion ACK, limits/encryption/read checks, operation/exact invocation permissions, tenancy/MCP/audit/public-write boundaries, thin SDK/CLI/HTTP projections, removed routes/contracts, sole inventory and fresh-install behavior. File path producers/consumers and sweeper are reopened because the fixes change them. TASK-003 retains its explicitly allocated SQL/metadata/readback work. No comprehensive first-round rediscovery occurred.

| Selected fresh role | Evidence and coverage | Result |
|---|---|---|
| ponytail-rev | Independent all-three closure, full incremental diff, affected callers/consumers, mandatory item inventory, carry-forward reconciliation; [validation report](findings-validation.md) | FIND-1/2 closed; only FIND-3 remains |
| followup-rev | Separate focused source validation of inaccurate new recovery documentation and one omitted changed-test item; [follow-up](followup-review.md) | RESOLVED; confirms incomplete existing FIND-3 closure, no new runtime obligation |

All selected reports are complete. The default bounded remediation topology did not recreate the discovery/domain team: changes use the exact previously validated existing owners and private boundaries. The independent validator identified specific documentation closure questions, and the fresh follow-up traced complete affected bodies before the validator independently reconciled its evidence. No proposal is retained solely on the validator's uncorroborated discovery. No unresolved conflict or missing source/report remains.

This is the first unsuccessful remediation review, so the two-unsuccessful-review convergence threshold does not apply. The remaining scope contracts to accurate mandatory item documentation; it does not reopen accepted runtime behavior or expand expectations.

## Prior-finding closure and reconciled acceptance

| Stable finding / obligation | Source and proof | Result |
|---|---|---|
| FIND-TASK-002-1: independent File uploads | File upload ID is minted before the session-qualified final path, and the same ID/path is persisted. Unchanged active-path index/locks/signers operate on distinct locations; Card replacement remains. Fresh overlap test completes both, replays one, verifies inventory-backed reads and aborts one without breaking the other. | CLOSED |
| FIND-1 affected strict-path and read invariants | Exact canonical tenant/digest/upload-ID shape is reconstructed and validated; inventory and hosted reads retain recorded-location/byte validation. Adapted tamper/tombstone fixtures use actual completed locations. Fresh path/reuse/authority/tamper selectors pass. | PASS |
| FIND-TASK-002-2: File expiry and shared queue progress | Existing discovery projects persisted nullable Card owner; sweeper selects strict owner-kind path validation then unchanged backend abort, tenant mutation, terminal-state guard and commit. Fresh pending/orphan/File-before-Card batch-one and negative owner/tenant cases pass. | CLOSED |
| FIND-2 existing lifecycle/race boundaries | Existing completed-row protection and leader/idempotency tests pass; SQL owner projection/round-trip tests pass. No expiry/grace/order/batch/provider cleanup policy changed. | PASS |
| FIND-TASK-002-3: required item documentation | Most prior inventory is now documented. New cancellation prose misstates already planned sessions/current terminal outcomes and sweeper eligibility; one changed sweeper test still lacks required purpose/Panics. Separately source-validated. | OPEN, REVISED |
| Shared upload/ACK/limits/auth/tenancy/public surfaces and non-goals | Unchanged R1 evidence carries forward; incremental existing-owner use and affected path/permission/byte checks independently inspected. No new cache/worker/index/API/promotion/hash/retention machinery. | PASS within original task split |

The full original acceptance and authority matrices remain in R1; the current validator reconciles their carried-forward rows and all affected evidence. There is no new specification conflict or independently validated runtime defect in this remediation.

## Remaining validated ledger

**FIND-TASK-002-3 — REVISED / VIOLATION.** Discovery source is prior RSTD-001, corroborated for the remaining occurrences by this round's fresh focused report. It retains the stable identity and one correction boundary.

`RunEvidence::pin` at `crates/shared/wyrd-client/src/verify/mod.rs:484–489` says later Files are unplanned after transfer failure, but the full plan request/response precedes serial transfer and server initialization already committed those sessions. It also promises the interrupted session remains open, although it can be Failed or already Completed before a lost response. Card/File initializer documentation at `wyrd-storage/src/service.rs:185–190,232–235` promises sweeper reclamation after failures including terminal Failed, while the existing discovery SELECT accepts only Pending/Initiating. Shared initialization prose must describe the same phases consistently. The changed `pg_sweeper::pg_tests::tick_sweeps_expired_uploads_and_reaps_idempotency_rows` at line 14 still lacks purpose/Panics despite the explicit changed-item gate.

The smallest correction is local prose: document actual plan-before-transfer effects and open/terminal uncertainty; restrict grace/expiry sweep promises to open states; preserve provider-expiry limits; add the omitted test item docs. Do not change lifecycle behavior to match inaccurate documentation. This is inaccurate required recovery semantics and an explicit mandatory-gate omission, not optional style polish; the instruction against optional style-only rounds cannot waive the existing rustdoc closure obligation. Full source locations, producer/consumer traces, materiality, correction burden and static closure proof are in [findings-validation.md](findings-validation.md). No new FIND ID or broader runtime requirement is introduced.

One bounded task packages the remaining correction: [TASK-002-R2-recovery-documentation.md](TASK-002-R2-recovery-documentation.md), routed directly to `$wyrd-implement`. FIND-1/2 and their passing proof carry forward on the next review unless that correction changes their behavior.

## Verification and limits

[verification.md](verification.md) records fresh repository-Postgres exact-selector runs: five server File overlap/lifecycle/reuse/authority/tamper tests, all three sweeper tests, two SQL storage tests and the strict File-path unit test pass. Wyrd/Vala migration/idempotency setup also passes. Incremental `git diff --check` passes. Linker compact-unwind warning did not prevent execution. These runtime passes support closing both behavioral findings; they cannot waive the mandatory documentation gate.

Implementation-recorded format/lints/boundary/docs/unit/client and emulator transfer checks were inspected as evidence, not claimed as fresh broad reruns. Historical REDs, full language suites, broad aggregates and cloud credentials were not rerun. Provider-emulator File sweeper abort and real-GCS multipart remain disclosed runtime limits. The validator traces accepted File paths and persisted handles through the unchanged backend-abort seam; lack of a provider-emulator cleanup case is not a source-proven defect or authority to expand provider cleanup semantics. No runtime/backend suite is required for the remaining documentation-only task.

**Verdict: FIX_REQUIRED.** Both lifecycle findings are closed. Only FIND-TASK-002-3 remains, with no specification revision or new runtime correction required. TASK-002's completion gate is not yet satisfied; change completion is not invoked.
