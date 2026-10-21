# Immutable review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/bifrost-source-consistency`
- Base: `1c5edf72958af8f47c63eb8ce39eb316672e170a` (merged main, PR #189).
- Candidate: `0d3223696f091fb16bb2e0ec36040a249d39b61f`.
- Range: `git diff 1c5edf72958af8f47c63eb8ce39eb316672e170a...0d3223696f091fb16bb2e0ec36040a249d39b61f` (58 files).
- Approved specification: `changes/active/bifrost-source-consistency/spec.md`, revision 2.
- Original task: `changes/active/bifrost-source-consistency/tasks/TASK-001-source-consistency.md`. Its header still names revision 1; stronger approved revision 2 governs conflicts.
- No previous task review or remediation packet exists for this change.
- Source was clean when pinned. Reviewers may write only their assigned reports. No source edits, commits, benchmark runs, or concurrent Cargo builds.

## Shared starting navigation map

This is a starting map, not coverage or a conclusion. Every reviewer must independently expand it, inspect the complete cumulative range, and verify source. Use CodeGraph first (`.codegraph` exists), then normal repository navigation.

| Surface | Owners and likely consumers | Relevant proof |
| --- | --- | --- |
| Published authority | `vala-sql/migrations/20261021000000_bifrost_publication_revision.sql`, `OracleTableCuts::read` in `queries/oracle_reader_authority.rs`; `BifrostCatalog`, `CutCache` | `pg_oracle_membership::file_list_publications_advance_their_node_revision`, cut-cache unit tests |
| Source handoff and recovery | Redux `scribe/claim_publication.rs`, `file_list_writer.rs`, `persistence.rs`, `staging_runtime.rs`, `hot_source.rs::ScribeHotSourceRegistry::advance_all_atomic`; boot `replay_wal_async` | Rollover/staging tests; publication/recovery tests |
| Memory and retained captures | Redux `scribe/memtable.rs`, `shards.rs`, `tail_rpc.rs::FetchLiveTailService`, `CaptureTable`, `RetainedCaptures`; `ScribeImpl::tail_service` owns shared state | `active_rollover_during_capture_is_read_once`, staged-before-shard case, retained-file and capture authority/expiry tests |
| Query acquisition and execution | Redux `oracle/mod.rs::build_physical_root`, `discover_live_routes`, `list_live_routes`, `live.rs`, `follower.rs`, `exec.rs`, `planner.rs`, `tail_discovery.rs`; existing `participant_cut.rs` roster | Published exact-row and invalid-capture journeys, distributed catalog query proof, late group evidence regression |
| Peer authority | `wyrd-spec/src/vala/{api,assignment_authority}.rs`, `wyrd-tonic/{proto/wyrd.v1.proto,src/private_conversion.rs}`, server `grpc/scribe_tail.rs`, `oracle/{tail_discovery,peer_service}.rs` | Follower tests, peer-network security, distributed failure paths |
| Capacity | `wyrd-testing/src/bin/capacity/{query/{workload,report},verification/{mod,report}}.rs`, `mise.toml` | Report boundary tests; retained write → verification → query final sequence |
| Other range contents | Workflow skill/reference prose; current architecture and product docs; span-permit test-only fix; verifier draft specs; long-horizon supersession note; verification cache and ingest fixture call sites | Audit against explicit handoff scope and stronger authority; preserve independent checks |

## Verification inputs

Read task's Verification and Evidence and Implementation Evidence directly; completion summaries are not source proof. The packet records focused correctness/static checks at `051be28a3`, capacity at `05186c1e4`, and final evidence-only commit at candidate. Independently check equivalence where relevant.

Retained artifacts are available outside the repository at `../bsc-baseline` and `../bsc-rev2`, including `commit`, `dirty`, `status`, logs, `{write,verification,query}-artifacts`, profiles and raw data. Inspect actual reports and coverage; do not assume their recorded interpretation. Do not rerun expensive capacity qualification during static review.

## Required authority

Read `.agents/skills/wyrd-task-review/SKILL.md` for your role requirements, `AGENTS.md`, `architecture/agent-rules.md`, `architecture/references/languages/{spec-driven-development,maintainer-style}.md`, and applicable architecture/testing references. Revision 2 of the approved spec wins over original task mechanics. Repository reference router is `architecture/references/README.md`. Bifrost design, Wyrd design/doctrine, SQL isolation, resource/lifecycle, deployment and security authority apply where reached.

## Independent discovery roles

- behavior-rev: full task acceptance, caller-to-result lens.
- invariant-rev: full task acceptance, producer-to-consumer invariant lens.
- repo-rev: complete applicable standards coverage across every changed surface.
- maintainer-rev: owning structure, reuse versus replacement, documentation and maintainability.
- system-rev: deployed failure, cancellation, restart, outage and recovery behavior.
- domain-rev concurrency: local capture/rotation/publication locking, query lifetime, resource budgets and cancellation.
- domain-rev durability: SQL revisions, commit order/replay, startup/recovery, retained files and persistent-state reconciliation.
- domain-rev security-tenancy: capture/query/tenant/table/deadline/epoch binding, signed peer authority, RLS and admission authorization.

Discovery reports remain independent. Findings require source-local IDs, violated obligations, exact evidence, observable consequence and smallest testable correction. No optional hardening or unrelated debt. Reports include complete scoped coverage and overall PASS/FAIL/BLOCKED. Implementation reviewers include obligation matrices. Reviewers must not read other discovery reports. A fresh structured Ponytail validator follows discovery.
