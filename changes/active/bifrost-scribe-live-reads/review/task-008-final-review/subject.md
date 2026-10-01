# Immutable final-review subject
Candidate: 9c3d7ecb982435919924dfa8e6930352b27a9b7e (detached HEAD).
Parent: 23eafa368bca19208faf8311eb7b5421e3660b38.
Cumulative TASK-007 base: a7582db587c6170a290760f1741673125612b797.
TASK-008 base: f7bebf704d6f3b1dd20d041e70c6ca512c0da307.
Approved spec: ../../spec.md revision 20, REQ-014/015 AC-016/017.
Tasks: ../../tasks/TASK-007-one-parquet-scan-for-live-reads.md and ../../tasks/TASK-008-tenant-proven-per-file.md.
Include prior task-007-review, task-008-review, task-008-r1-review reports and remediation packets. R2: ../task-008-r1-review/TASK-008-R2-normalize-review-evidence-whitespace.md.
Review cumulative.diff and final.diff. TASK-006 work only where it touches TASK-007/008 behavior; no unrelated-scope finding.

## Constraints and decisions
Static only. NO cargo, nextest, mise, builds, tests, benchmark, commits, or source edits. Reports only here. Candidate must stay immutable. No CodeGraph index exists.
FIND-007-3 unchanged. Postgres data_tenant_id columns out of scope. Error code WYRD_VALA_500_QUERY_TENANT_INVARIANT.
Known remote_live_scribe_drop_releases_query reaches 120 s deadline because in-process kill does not reset peer connections; reported awaiting maintainer decision; do not raise as finding.

## Furnished exact-tree verification (not rerun)
User reports test:bifrost 8/9 lanes pass; sole failure oracle_ownership_snapshot_round_trips belongs to now-deleted process harness. Oracle journeys 40/40, server 26/26, Python and TypeScript pass.
After final edits: lints exit 0; fmt and git diff --check clean; wyrd-client 314/314 (Postgres wrapper); redux oracle::(live|follower):: 19/19; wyrd-testing --lib --bins 61/61; codegen:regen no drift.
Historical retained exact logs and task evidence available. Distinguish from user-furnished current results; no fresh runtime proof.
