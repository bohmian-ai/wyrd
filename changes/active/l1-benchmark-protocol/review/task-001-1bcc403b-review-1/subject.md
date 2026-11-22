# TASK-001 review subject

Post-review scope update: the maintainer approved spec revision 2 voiding all
previous performance targets for refactored L1 cases. Finding 5 is withdrawn;
the remediation task now references revision 2 and six remaining findings.
The identities and original source-review statements below describe the
immutable revision 1 audit. Later packet-only edits do not claim a new source
review or change the reviewed commits.

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
- Base: `2418dc7848375c00ac6a991cafca88f722bcfdfe`.
- Candidate: `1bcc403bf2c81c3cfa2c78a78f114cb9c5a49aaf`.
- Diff: `git diff 2418dc78...1bcc403b` (read directly; no archived source).
- Approved authority: `changes/active/l1-benchmark-protocol/spec.md`, SPEC-l1-benchmark-protocol revision 1, approved 2026-11-22. Candidate blob `f5f18f8b9a9f05d86596621bf3b3c7efa5d250bf`.
- Original task: `changes/active/l1-benchmark-protocol/tasks/TASK-001-standardize-l1-benchmark-sweeps.md`. Candidate blob `e82e229a6030ec90b004d2554ce2f3db884436ae`.
- Checkout HEAD: `6e158cd39407a5c25e5edad163633765d99fdad9`. Its sole difference from candidate is an unrelated task-status record under `changes/active/bifrost-write-throughput-and-retention/`. All reviewed paths match candidate. Initial working tree clean.

## Review assignments

- `behavior_rev`: primary requirement/scenario coverage, selection, report/summary, protocol boundaries, mise/documentation, reuse and unassigned repository rules. Report `task-review-behavior.md`.
- `system_rev`: subprocess isolation, fixture ownership, setup/cleanup, interruption/deadlines, persistence failure, continuation and connected harness proof. Report `task-review-system.md`.
- `invariant_rev`: measured-window populations, CPU/counters, drains, aggregation, pooled histograms, targets and their report/record consumers. Report `task-review-invariants.md`.
- A fresh `ponytail_rev` will independently validate all reports and coverage, including specialist applicability. Report `findings-validation.md`.

The orchestration agent writes the reconciled verdict and any remediation task; it does not discover or validate findings. Separate repo/maintainer/domain reviewers are not initially selected: primary covers owner structure and repository rules, invariant reviewer covers shared accounting, and system reviewer covers subprocess recovery. No production authentication, tenancy, SQL capability, or persistent domain-data behavior is changed. The validator must challenge this allocation if inspection exposes a missing boundary.

## Navigation and evidence inputs

CodeGraph navigation: category/report dispatch in `capacity/main.rs` → `cases.rs` selection → `sweep.rs` supervision → `experiment.rs` runner; existing workload producers in `capacity/{query,verification,gateway}`; recorder pooling in `wyrd-benchmark/src/registry.rs`; saved summary consumption in `capacity/summary.rs`. Lifecycle dependencies include `release_server.rs`, `release_server/os.rs`, and `scripts/postgres/with-test-postgres.sh`. Assertions are inline and in `capacity/tests/pg_tests.rs`.

Implementation evidence is recorded in the task's existing acceptance entries and C1–C3 command results. Scenarios 1–4 explicitly record TDD VIOLATED. Scenario 5 and AC-005 explicitly defer full-size feasibility while performance execution is held; no full-size benchmark will be launched by this review. The approved spec takes precedence over older general benchmark preferences for this task.

Relevant authority: root AGENTS.md, benchmarks/AGENTS.md, changes/spec-driven-development.md, agent_docs/{index,maintainer-style,rust,agent-surfaces}.md, TESTING.md, architecture/README.md and architecture/bifrost-design.md § Durability and visibility. Reviewers read additional narrow authority as needed. Shared memory was searched for task-specific precedent; returned notes/preferences are context, not replacement authority.
