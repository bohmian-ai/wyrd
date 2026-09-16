# Forge recovery domain review — PASS

## Immutable subject and boundary

- Base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`; candidate `1b2db8634bf83c03ba210ebf55700983dd9091e6`, tree `52b3fe3243df7f74ba793eaa5be6caa84fba78ad`.
- Reviewed the cumulative Forge diff, with particular attention to `FIND-TASK-003-R1-3`: `crates/vala/vala-bifrost-redux/src/forge/{worker,planning_scheduler}.rs`, `tests/integration/forge/compaction_admission.rs`, the Forge task migration and query, and the supervisor fixture. This report does not stand in for the separate whole-repository review.
- Governing authority: approved `changes/active/surfaces-oracle-integration/spec.md` revision 9 (`REQ-035A`, `REQ-064`, `INV-025`); original `tasks/TASK-003-close-repository-integration.md`; R1 finding and R2 remediation packet; `AGENTS.md` §§11–12; `architecture/agent-rules.md`; `architecture/bifrost-design.md` maintenance and recovery sections; `architecture/references/domain/{iceberg,analytical-operations-reliability}.md`; `architecture/operations/reliability-and-recovery.md` Forge failure boundaries.

## Domain checks

| Obligation | Source and reachability | Result |
|---|---|---|
| Restore the original Forge recovery assertion without accepting a publication refusal | `compaction_admission.rs:2596-2606` accepts only errors containing `shut down`, the same predicate at the base commit. `acceptance_unknown_recovers_from_durable_state` calls the modified `unresolved_commit_resets_once_absence_is_provable` directly. The prior `refused before commit: Cancelled` alternative is gone. | PASS |
| Observe exact durable recovery before stopping, without letting the successor create a fresh rewrite effect | The test arms `hold_after_next_rewrite_settlement_for_test`, then waits for the hold and for the specific unresolved operation IDs to reach `reset`, then cancels stop and releases the hold. `ForgeWorker::plan_rewrite_attempt` invokes the pause only after `rewrite_settlement_barrier` has reconciled all live table operations and refused any pending/unresolved result, and before `managed_rewrite(...).plan()` can create a new rewrite. The pause is one-shot and passive. | PASS |
| Preserve production cancellation and publication semantics | Every new observer field and method and the call in `plan_rewrite_attempt` are guarded by `#[cfg(feature = "test-support")]`; the feature existed before this remediation and is absent from the crate's default feature set. No production publication or settlement decision was changed by R2. `worker_stop().cancel()` is issued before the test releases the hold, so normal worker shutdown is still exercised. | PASS |
| Keep the broader Forge recovery and task authority intact | The cumulative scheduler change uses `ForgeTablePolicy::extract` to refuse impossible rewrite geometry before enqueue while leaving other strategies untouched; the SQL `orphan_cleanup` partial unique index and `ON CONFLICT DO NOTHING` coalesce nonterminal cleanup demand atomically. These changes remain within the approved Forge task/recovery authority and do not supply an alternative settlement path for the test above. | PASS |

## Verification limits

The R2 packet records the exact Postgres-backed ignored journey as passing once on final implementation commit `117f668f6046f15dcfb7b197fc53f8557f4c1757`, with 16/16 repeated runs during implementation; it also records the full `mise run gate` and focused Bifrost lanes passing on that commit. The subsequent candidate commit changes only that evidence packet, so the tested source tree is unchanged. I inspected code and recorded evidence but did not rerun a disk-heavy suite; the available-space warning makes another broad gate disproportionate to this review. The new one-shot barrier can hang a failed test if never released, but its waiter is bounded and this is test-only fixture behavior, not a reachable production semantic regression.

## Findings and result

No material Forge-domain findings. **PASS.** The deviation from the R2 instruction to reuse an existing barrier is justified: the existing observer holds are after handoff, returned attempt, or durable dispatch settlement, none at the required post-reconciliation/pre-new-effect point. The new test-support-only hold is the smallest correct seam for preserving the original assertion.
