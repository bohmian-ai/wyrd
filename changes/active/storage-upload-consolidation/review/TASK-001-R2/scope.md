# TASK-001 remediation review — round 2

Repository `/Users/stevenforrester/Documents/GitHub/wyrd`.
Original base `3899b0a35`; prior candidate `826fcf9c742a36bcae5e8f68307e76685d88244e`; new candidate `5e7dec69732ba6d4d05399aeffd6f4d9c683745d` (HEAD, initially clean).
Requested remediation range `e6a34a01092d44729456b17bb55553e08d63d322..5e7dec697`.
Review incremental `git diff 826fcf9c7 5e7dec697`; `826fcf9c7..e6a34a010` adds only the first review reports/task, so implementation patch correspondence is exact. Full cumulative `3899b0a35...5e7dec697` remains available. No rebase correspondence problem and no CodeGraph directory.

## Acceptance context

- Approved `changes/active/storage-upload-consolidation/spec.md` revision 2.
- Original `tasks/TASK-001-object-inventory.md`.
- Prior `review/TASK-001-R1/{verdict,findings-validation,scope,verification}.md` and all six discovery reports.
- Remediation `review/TASK-001-R1/TASK-001-R1-inventory-lifecycle.md`, including appended evidence, treated as claims to check.
- Root AGENTS, nested SQL AGENTS, spec-driven-development, maintainer/Rust/error guides, applicable TESTING and owning Card/Bifrost/security slices. These authorities are unchanged by the incremental diff.

This is the first remediation review, not a new full discovery pass. Carry forward source-backed first-review acceptance of schema/catalog/partitioning/Blooms, shared completion validation, existing query/authorization/tenant/public-write boundary, outbox/ACK isolation, removed SQL inventory, other consumers and bounded verification-runtime corrections. Recheck affected hydration and compensation consumers/callers and changed docs/tests/tracing/setup. Reopen an accepted area only if new evidence or remediation dependencies invalidate its proof; record why.

## Navigation map and review selection

| Changed surface | Symbols, callers and proof |
|---|---|
| Card hydration | `components/cards/service.rs::hydrate_card`; existing three getters and `card_files`; new `hydration_requires_recorded_envelope` and existing round trip/path/readiness proofs |
| Compensation completion | `commit_card_failure`, `abort_card`, `reconcile_card_claim`; existing `record_card_deletion`, `fail_card`, `mark_card_reconciliation_succeeded`, `reconcile_cleanup_claim`, explicit delete and reconciliation worker claim/finish; new `compensation_records_inventory_deletion` |
| Cleanup persistence and concurrency | `wyrd-sql/src/queries/cards/lifecycle.rs`, reconciliation SQL/worker callers; inspect failed/pending commit before ACK and possible overlap with claimed retries; no new queue/state/lease semantics are approved |
| Mandatory docs | Enumerated FIND-TASK-001-3 items in storage service, Card service, storage routes, scribe ObjectGate and ArtifactPrompt/helper docs; include newly changed test helpers/items |
| Handler tracing | `components/storage/routes.rs::{complete,download_init}`, safe argument scrubbing and existing route results |
| SQL setup proof | `pg_migration::pg_tests::fresh_install_has_no_postgres_object_inventory`, local DSN requirement and unchanged assertions/helper |

Paths in the map are under `crates/wyrd/wyrd-server/src/` unless qualified. This map is a starting point, not a conclusion; expand as the affected paths require.

Select one fresh domain-concurrency-rev for bounded discovery of the materially changed durable cleanup/ACK ordering and its overlap with lease-fenced retries. This scope includes the reported unleased-success overlap as a hypothesis, not a finding: establish actual consequences and approved obligations before treating duplicate idempotent facts as material. Do not recreate the first-round team.
Then select one fresh ponytail-rev for all five closures, incremental diff, affected paths, carried-forward coverage and independent validation of the domain report. That reviewer also owns mandatory docs/tracing/setup closure and the hydration proof. No separate standards/maintainer/system reviewer is warranted for this bounded patch absent a material unresolved question. If the validator discovers a new blocker, obtain separate focused validation before retaining it.

Exact report paths in this directory: `domain-review-concurrency.md`, then `findings-validation.md`; orchestrator writes `verdict.md` and only a bounded remediation task if required. No implementation edits or prior report overwrites. Cargo checks are serialized by root; reviewers should request a slot rather than contend.
