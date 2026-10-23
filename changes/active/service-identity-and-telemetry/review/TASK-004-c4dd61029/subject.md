# TASK-004 immutable review subject

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-004`
- Base: `3e7c59db7` (parent of first TASK-004 implementation commit `3bd495ce7`).
- Candidate: `c4dd61029a54c44017c63bd4ff469472c406bc13`.
- Review range: `git diff 3e7c59db7...c4dd61029` (complete cumulative range).
- Approved authority: `changes/active/service-identity-and-telemetry/spec.md`, revision 12, and `supplements/table-storage-and-correlation.md`; README records approval. Original task: `tasks/TASK-004-shared-table-storage.md` in that packet.
- No previous task-review reports or remediation packet exist in this checkout.
- Working tree was clean at review start. Reviewed source must remain immutable; reports alone may be added here.
- CodeGraph directory absent; use existing repository navigation.

## Starting navigation map

This map is a starting point, not complete coverage or a conclusion. Each reviewer reads the full range and expands callers and consumers independently.

| Changed boundary | Owners and likely consumers | Relevant proof |
| --- | --- | --- |
| Public layout wire and bounded validation | `wyrd-spec/src/vala/{api,error}.rs`; redux `catalog/layout.rs`; Rust client table/query; server Bifrost service; Python `_client.py` and exports/stubs; generated TS types | V1 layout matrix, V2 create/describe/refusal, SDK contract checks |
| Composite identity and accepted revisions | redux `catalog/physical_partition.rs`, `bifrost_catalog.rs`; `scribe/{preprocess,ingress,seal_key,wal,replay,memtable,shards,write_recipe}.rs` | V3 identity/WAL, V4 atomic fanout, V6 accepted revision |
| Hot authority, staging and durable publication | `scribe/{assembly,claim_assembly,claim_merge,claim_publication,hot_source,hot_stage,member_stager,persistence,staging,staging_runtime,promotion,file_list_writer}.rs`; new vala-sql migration | V5 lifecycle/peers/restart; existing fences/recovery and promotion mismatch tests |
| Mixed-spec reads and pins | catalog pin/manifest resolution; `oracle/{exec,follower,codec}.rs`, `scribe/{memtable,tail_rpc}.rs`; pinned DataFusion fork | V7 mixed runtime specs plus catalog production pin test |
| Forge and physical writer truth | `forge/{publication,scribe_promotion}.rs`, `parquet/promoted_object.rs`, `scribe/parquet_writer.rs`, table defaults, pinned Iceberg/compaction forks | V8 mixed-spec promotion/compaction, V9 encoded sort/Bloom proof |
| Harness and consumers | `wyrd-testing/src/bifrost/revisions.rs`, scribe/forge/oracle journeys, redux integration promotion fixtures; docs generated errors and guide | Task evidence records exact selectors and results |

## Authorities and verification

Read root AGENTS.md, all touched nested rules (`docs`, Python SDK, TS SDK), `changes/spec-driven-development.md`, `agent_docs/index.md` and applicable Rust/errors/agent-surfaces/maintainer guides. Governing architecture: Bifrost table identity, storage/types, durability, Scribe, Oracle, Forge, resource/failure invariants; security peer identity and tenant/Postgres binding; applicable operations topology/recovery sections. Architecture owner itself changes in this range; reconcile with approved spec rather than accepting implementation as authority.

Available verification is recorded in the original task's Implementation Evidence table and Diagnoses sections. Treat those results as supplied evidence, not independently rerun checks. Inspect every changed assertion/test and diagnosis. Review is static unless a focused check is needed; coordinate Cargo commands through the orchestrator. External dependency fork source at pinned commits must be inspected where necessary; do not infer correctness merely from pins or green tests.

The complete active change packet was first tracked by the final candidate commit; other tasks in it are authority/context, not claims that their implementations belong to this task.
