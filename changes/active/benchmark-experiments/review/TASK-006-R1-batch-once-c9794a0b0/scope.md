# Review scope — TASK-006-R1 approved revision "build batches once" (Steps 1–5)

- Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments
- Branch: change/benchmark-experiments
- Base: `55f87cef1`; candidate: `c9794a0b0` (HEAD). Range `git diff 55f87cef1...c9794a0b0`.
- Commits in range: `b47082554` (Step 1), `cd26d66d3` (design owners), `c5d0ed5ad` (Steps 2–3),
  `f50fdd9fb` (Step 4), `c9794a0b0` (evidence).
- Caller-named `e39c9d4e5` is not an ancestor of the candidate. `git range-diff` shows `b47082554`
  is the same patch with one rustfmt line wrap in a test (`variant.rs`); evidence for `e39c9d4e5`
  is carried to `b47082554` on that basis.
- Spec: `changes/active/benchmark-experiments/spec.md` (REQ-021, AC-014, INV-001/002).
- Task: `changes/active/benchmark-experiments/tasks/TASK-006-R1-simplify-verification-data-plumbing.md`,
  section "Approved revision — build batches once and remove shared Scribe copies" (line 1095)
  and its "Approved revision execution evidence — Steps 1–5".
- Round: first review of this revision (full base-to-candidate range). Earlier TASK-006 sections and
  evidence are context; their acceptance is not re-litigated except where this range changes it.

## Navigation map (starting point, not coverage)

| Area | Changed symbols / files | Likely consumers / tests |
|---|---|---|
| Step 1 | `crates/shared/wyrd-queue/src/variant.rs::EncodedVariant::validate_bytes`; `vala-bifrost-redux/src/tables/mod.rs::validate_variant_values` | `variant::tests`, `tables::tests` |
| Step 2 | `vala-bifrost-redux/src/tables/staged.rs` (new: `Cell`, `StagedRow`); `tables/audit/projection.rs`; `wyrd-server/src/verification/results.rs::ResultRow`; `wyrd-server/src/scribe_outbox.rs` (`Part::dense`, weights, lane keys) ; `components/gateway/capture.rs` | `scribe_outbox::tests`, `tables::staged::tests`, verification fault/results |
| Step 3 | `scribe_outbox.rs` `FramePayload::{Ipc, Rows}`, local route `IngressPayload::Canonical`; `grpc/capture_peer.rs`; `gate/attribution.rs`; `scribe/execution_lanes.rs` | `local_canonical_and_peer_ipc_preserve_frames_and_replay`, `scribe_persistence_path::canonical_nested_batches_share_one_managed_wal_path`, peer listener test |
| Step 4 | `scribe/parquet_writer.rs::ParquetBatchEncoder::prepare_sorted_candidate`, `sort_keys` | `gathered_sort_*`, existing parquet_writer tests, scribe journeys |
| Step 5 | benchmark records under `benchmarks/` (untracked dirs preserved) | evidence table in task |
| Docs | `architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`, `spec.md` | — |
| Test fix | `wyrd-server/tests/integration/pg_verification_groups.rs` (flush before snapshot) | diagnosis in evidence |

## Reviewer selection

- `behavior-rev` (required): full acceptance, rules, maintainability, failure paths.
- `invariant-rev`: outbox staging lifetime, memory charging, frame identity/splitting, lost-ACK replay,
  lane-key ordering — shared state across producer, outbox and Scribe.
- `domain-rev` security/trust boundary: canonical-ingress eligibility; peer/native IPC must keep full
  validation (INV-001).
- `domain-rev` persistent data: gathered sort equivalence, persistence admission, durability/recovery.
- Not selected: `repo-rev` (one tier pair, behavior-rev covers rules), `maintainer-rev` (behavior-rev
  covers maintainability; no cross-tier ownership move), `system-rev` (no new failure propagation or
  recovery mechanism; retry/defer behavior is fixed by the task and covered by invariant-rev).

## Verification constraints

L1 benchmarks are not rerun (records under `benchmarks/`). Focused selectors are rerun by the
orchestrator sequentially; results in `verification.md`. Reviewers do not run cargo.
