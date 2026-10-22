# TASK-001 cumulative remediation review subject

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/bifrost-source-consistency`.
- Base: `1c5edf72958af8f47c63eb8ce39eb316672e170a`.
- Candidate: `339c9b8d0484421b5fd67b71194eed8fcc60cadb`.
- Prior reviewed candidate: `0d3223696f091fb16bb2e0ec36040a249d39b61f`.
- Full range: `git diff 1c5edf72958af8f47c63eb8ce39eb316672e170a...339c9b8d0484421b5fd67b71194eed8fcc60cadb` (78 paths, including prior reports).
- Latest fix range: `git diff 0d3223696f091fb16bb2e0ec36040a249d39b61f...339c9b8d0484421b5fd67b71194eed8fcc60cadb`.
- Approved spec: `changes/active/bifrost-source-consistency/spec.md`, revision 2.
- Original task: `changes/active/bifrost-source-consistency/tasks/TASK-001-source-consistency.md`.
- Prior review: `changes/active/bifrost-source-consistency/review/task-001-0d3223696-20261021/{verdict,findings-validation}.md`.
- R1 task/evidence: that prior review directory's `TASK-001-R1-retained-capture-lifecycle.md`.
- Prior stable IDs: FIND-TASK-001-1 through FIND-TASK-001-5. Preserve these IDs for any unclosed shared cause; new independently validated findings begin at 6.

Source was clean at pinning. Read source and full cumulative diff independently; use latest fixes to locate closure but retain original acceptance audit. No conclusion is supplied for this candidate. Reports and recorded green checks are claims to assess, not source proof. Do not receive or consult other CURRENT discovery conclusions; prior review/remediation are required history.

## Common starting navigation map

This is a starting map, not exhaustive coverage or a conclusion. `.codegraph` exists; use CodeGraph before source navigation and expand through every relevant sibling writer/consumer.

| Surface | Owners, callers and tests |
| --- | --- |
| Shared Arrow charge lifetime (R1) | `scribe/memory.rs::RetainedArrowCharge`, `ScribeOwnership::detach_immutable`, existing `ScribeMemoryLease`; `memtable.rs` active/frozen/ReadableSource/LiveCapture/CapturedRead and `commit_retirement`; `shards.rs::release_committed_generation_memory`, replay/rollback sibling writers; `oracle/memtable_source.rs` source, streams, fetch/projection/repartition clones; `oracle/follower.rs`; persistence/staging/Parquet owners. New `scribe::shards::tests::a_retired_generation_stays_charged_until_its_last_holder_drops`. |
| Capture authority/expiry (R1) | `scribe/tail_rpc.rs::{RetainedCapture,CaptureTable,capture_active_streams,capture,retain,open_capture,release_capture}`; `ScribeImpl::tail_service`; local/private listing callers; tests for elapsed-before-installation and elapsed-open-before-cleanup. |
| Cleanup deadline (R1) | Redux `oracle/{tail_discovery,live,mod}.rs` original-deadline propagation and last-route Drop; server `oracle/tail_discovery.rs::{admit,discover,release}`; transport `TonicTailReadTransport`; local Gate/forwarding/scheduled consumers. New real mTLS stalled release test in server module. |
| Published authority and recovery (original) | SQL `20261021000000_bifrost_publication_revision.sql`, `OracleTableCuts`; catalog materialization/CutCache eviction; Scribe ClaimPublisher/file-list/reconciler/hot-source registry and startup restoration. |
| Query protocol (original + R1 consumers) | Oracle acquisition/classification/build/list_live_routes; retained routes/follower/exec/planner/profile; Scribe coherent rollover and staged leases; private proto/conversions, ScribeProviderCut and assignment digest. |
| Exact rows and negative proof | Existing published source-transition and invalid-capture journeys, distributed cut-cache/failure matrix, late group evidence; local rollover/staged/capture-authority/expiry proofs; SQL publication replay/rollback/tenant proof. |
| Documentation (R1) | Five prior rustdoc sites in server `grpc/scribe_tail.rs` and Redux `oracle/{follower,live}.rs`; all newly modified R1 items also governed by actual AGENTS §16; current architecture/product/diagram/generated LLM prose, dispatcher/query_stream guidance. |
| Remaining cumulative contents | Existing capacity workload/report target changes, current architecture changes, workflow skills/reference prose, disclosed draft specs, carried span-permit test correction, active packet and prior reports. |

## Authorities and evidence

Read `.agents/skills/wyrd-task-review/SKILL.md` role requirements, actual `AGENTS.md`, `architecture/agent-rules.md`, reference router, `architecture/references/languages/{spec-driven-development,maintainer-style}.md`, Wyrd design/doctrine and applicable Bifrost/security/operations/testing references. Architecture and approved spec govern tasks/tests/evidence.

R1's appended Implementation evidence records exact RED/GREEN tests, module/owner coverage, final fmt/lints/docs checks and diagnoses, including the modified freeze-clone test. Independently inspect these tests and diagnoses; the implementation agent's completion summary is not an input to discovery. No Cargo or benchmark runs are authorized in parallel reviewer work; use static source/evidence review and explicitly disclose execution limits.

Retained original capacity artifacts remain at `../bsc-rev2` and `../bsc-baseline` (commit/status/reports/raw data/logs/profiles). No newer capacity qualification is recorded in the R1 packet. Assess original REQ-006 / AC-006 independently; bounded remediation scope does not remove original task acceptance. Do not invent an optimization or alter targets based on a baseline. Full integrated SDK/agent gates remain change-review obligations per approved spec.

## Report protocol

Fresh discovery roles: behavior, invariant, repo standards, maintainer, system resilience, concurrency domain, durability/persistent-data domain, and security/tenancy domain. Each reads the full cumulative range and relevant surroundings, focuses closure using the fix range, writes only its exact assigned report, and expands the common map independently. Do not spawn more agents, edit source, run Cargo/benchmarks, or exchange discovery conclusions. You are not alone; preserve other report files.

Each report must give scope/authority/source coverage, prior-ID closure evidence, material proposed findings with local IDs and exact consequence/correction/proof, verification limits, and overall PASS/FAIL/BLOCKED. Behavior/invariant reviews include complete acceptance matrices. Distinguish original-task acceptance from R1 finding closure; assess every new/material item and immediate owner under stronger authority without optional scope growth. New cleanup/accounting machinery must justify reuse versus existing owners. Record no new finding for a mere wording/placement preference; mandatory changed-item rustdoc retains its explicit gate.
