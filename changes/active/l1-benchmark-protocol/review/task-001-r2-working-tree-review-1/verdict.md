# TASK-001-R2 working-tree verdict

**Substantive review: PASS for the frozen snapshot.** Existing findings
FIND-TASK-001-2 and FIND-TASK-001-3 close, including the interrupt correction.
No implementation defect or material evidence gap remains. No further
remediation task or interrupt hook is required.

**Formal task status: BLOCKED only pending a candidate commit.** The maintainer
explicitly requested review while these changes remain uncommitted. This
review performed the full substantive check without creating a commit; the
invoked task-review skill's formal gate still requires unambiguous base and
candidate commits. This identity gap is not another code finding.

## Subject and authority

- Base/HEAD: `98a58443afac1f74cfefd607f3cd429c3b1cdee1`.
- Original base: `2418dc7848375c00ac6a991cafca88f722bcfdfe`.
- Candidate: uncommitted working-tree snapshot; diff read directly with
  `git diff HEAD --`.
- Tracked binary diff SHA-256:
  `b6f072f75ebfe8414ec55313e6c5f00a955212e847e13ca2a868e3bfd7505a96`.
- Per-file source/spec/task SHA-256 identities: [subject.md](subject.md).
  All were rechecked unchanged during independent validation and final packaging.
- Approved [spec revision 2](../../spec.md) and
  [R2 task/evidence](../task-001-r1-98a58443a-review-1/TASK-001-R2-finish-lifecycle-deadlines.md).
- Prior [R1 verdict](../task-001-r1-98a58443a-review-1/verdict.md) and
  [validated findings](../task-001-r1-98a58443a-review-1/findings-validation.md).

Applicable authority remains root/benchmark AGENTS, spec-driven-development,
maintainer-style/Rust/agent-surface/testing guidance and Bifrost durability
authority. Full-size feasibility remains explicitly deferred under AC-005.
Old performance targets remain void and historical/L2 interpretation unchanged.

## Independent assignment and acceptance

Fresh `r2_worktree_rev` independently checked root-cause closure, the actual
runner-to-supervisor handoff, deadline/interrupt/exit/cleanup callers, assertions
and recorded execution. Its [validation report](findings-validation.md) contains
source and proof references. The orchestrator ran checks, recorded identities
and packaged this verdict without discovering or validating findings.

One independent remediation reviewer is sufficient for these previously
assigned lifecycle and cleanup boundaries; no new production authentication,
tenant SQL, domain durability or SDK/wire boundary needs separate expertise.
No new finding required a separate validator. Unchanged closed R1 evidence
was carried forward only after checking affected producer/consumer seams.

| Existing finding / scenario / authority | Checked implementation and proof | Snapshot result |
|---|---|---|
| FIND-TASK-001-2 / R2 scenario 1 / REQ-002/003, AC-003 | Lifecycle atomically announces before process finish; supervisor consumes announced start without a final record. Runner ordering test passes. Stepped test runs past 510 s without a record, writes it at 520 s, finishes at 530 s and retains VALID/30 s teardown. | PASS — closed |
| FIND-TASK-001-3 / R2 scenario 2 / REQ-002/003/005, AC-002 | One original deadline governs admission. Physical cleanup after expiry is recorded but cannot confirm or start another case. Test records cleanup at 555 s after a 550 s deadline as INVALID/unconfirmed and next case NOT RUN; timely invalid-case cleanup still permits continuation. | PASS — closed |
| Interrupt/cutoff/process-exit ordering / shared root correction | All terminal branches read the announcement before using fallback time. Interrupt preserves an already-observed start, then rereads the announcement before establishing one. Actual handoff/deadline behavior has executable proof; the exact between-polls interrupt schedule is a disclosed limit. | PASS |
| Genuine overrun, hung/missing evidence, Docker and saved outcomes | Stepped overrun and late/hung Docker tests retain bounded failure, saved reasons and no unsafe continuation. Runner stop/drain and retained evidence assertions remain valid. | PASS |
| Previously closed listing, persistence timing, snapshots, smoke integrity and approved no-target output | Prior source-backed proof carries forward; changed lifecycle/accounting seams inspected and connected harness evidence retained. | PASS |
| Formal immutable candidate identity | Source and task hashes stable, but no candidate commit exists. | UNRESOLVED — formal gate BLOCKED |

## Verification and limits

Fresh review execution on the hashed snapshot:

- `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(/^sweep::tests::/)'`: 10 passed, 0 failed, 0.217 s.
- `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::a_case_announces_teardown_before_it_stops_its_processes)'`: 1 passed, 0 failed, 0.012 s.
- `mise run fmt:check`: PASS.
- `mise run lints`: PASS (workspace all-features/all-targets and standalone server).
- `git diff --check`: PASS.

The task separately records 112 capacity tests, 27 runner tests, repeated sweep
tests and connected `mise run test:benchmark:l1` success in 221 s. Actual
assertions and the connected consumer path were inspected; the connected lane
was not rerun by this review. A dedicated test forcing interrupt inside the
polling interval was not added or run. Existing executable transition/deadline
proof plus the inspected short interrupt branch are sufficient for this bounded
correction; no new production/test hook is requested.

Historical TDD disclosures are preserved and nonblocking per maintainer
direction. No full-size performance or live-profiler run is claimed; AC-005
execution remains on hold.

To finish formal acceptance, identify a committed candidate and verify its
content corresponds to this snapshot, inspecting any intervening changes.
Reuse this independent source/proof evidence for unchanged content rather than
restart discovery. The identity requirement creates no production correction.
No implementation, task-status rewrite, staging, commit, merge, push or
deployment was performed by this review.
