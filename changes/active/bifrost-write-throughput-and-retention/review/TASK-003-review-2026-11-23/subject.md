# TASK-003 immutable review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
- Base: `74034459d468fd0adf277e3b51c32b1a676e8e55`.
- Candidate: `c0900bff523e83d3666658440daa81355067cb34`.
- Diff: `git diff 74034459d...c0900bff5`; first TASK-003 review, not a remediation review.
- Approved authority: SPEC-bifrost-write-throughput-and-retention revision 4, current `../../spec.md`.
- Original task and implementation evidence: `../../tasks/TASK-003-bounded-write-pipeline.md`.
- Spec SHA-256: `fca219efc01e8300dfb36641e603c7bfb0bec49f5e83f7068643b79a89e6da6f`.
- Task SHA-256: `0b139b2b21ad3d9d414568e6d1e51e117f58fb645638a6e64b76ea89e2439a91`.

The packet is intentionally uncommitted and supplied by the maintainer. Its
contents are fixed by these hashes for this review. Unrelated working-tree
benchmark and capacity edits are excluded. Review does not modify candidate
source, spec or original task.

The maintainer explicitly requests code review before independently running
the final benchmark. Review assesses code and deterministic correctness now;
REQ-020 performance qualification and task completion remain pending. No
benchmark execution is authorized in this review.

## Navigation and assignments

- `behavior_rev`: primary requirement, scenario, implementation, proof and
  repository-rule review; collection, overlap, independent ACK, slot bounds,
  architecture and maintainability. Report `task-review-behavior.md`.
- `invariant_rev`: durable acceptance and SQL ambiguity, shared group state,
  WAL cancellation and recovery, resource custody, rotation/seal/pressure,
  shutdown/FlushAll and receipt frontier. Report `task-review-invariants.md`.
- `repo_rev`: collateral hosted Sandbox admission, test controls and SDK
  consumers, Forge reservation assertion. Report `task-review-repo.md`.
- A fresh validator will check all reports, requirement coverage and proposed
  findings after discovery completes. The orchestrator does not review source
  or validate findings.

The existing owners are shard scheduling and `GroupWalState` in
`scribe/shards.rs`, WAL IO in `scribe/execution_lanes.rs`, tenant-bound fence
queries, insertion/visibility and committed mirror, and recovery/cleaner custody
consumers. Collateral consumers include Rust/Python/TypeScript hosted-capacity
journeys and Forge production-route assertions. Reviewers read the actual
range and follow unchanged dependencies rather than treating this map as proof.
