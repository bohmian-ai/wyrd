# TASK-001 first review

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Base: `3899b0a35`; candidate: `826fcf9c742a36bcae5e8f68307e76685d88244e`.
Review `git diff 3899b0a35...826fcf9c7`; candidate is HEAD and source starts clean.
Approved authority: `changes/active/storage-upload-consolidation/spec.md`, revision 2.
Original task: `changes/active/storage-upload-consolidation/tasks/TASK-001-object-inventory.md`.
First review; no carried-forward acceptance or findings. Scope is the entire recorded range, including verification telemetry and fixture changes.
No `.codegraph/` directory exists. Use Git and repository navigation directly.

## Starting navigation map

| Surface | Owners and paths | Callers, consumers, proof |
|---|---|---|
| Inventory contract | `vala-bifrost-redux/src/tables/storage/objects.rs`, `tables/{mod,staged}.rs`, `namespaces.rs` | Built-in catalog/bootstrap, Forge namespace migration, managed columns and attribution |
| Write protection | `vala-bifrost-redux/src/gate/mod.rs` | Public write admission, `gate_reserves_server_capture_tables` |
| Recording/readback | `wyrd-server/src/components/storage/inventory.rs::ObjectInventory`, `ObjectFact`, `CardObjects` | `record`, `card_objects`, `fold`; existing `SystemReadAuthority` and `ScheduledQueryCaller` |
| Delivery | `wyrd-server/src/scribe_outbox.rs::{ScribeWrite,ScribeTable,ScribeSink}`, `grpc/capture_peer.rs` | Existing local/peer capture, row projection, batching, tally and ACK, submission handoffs, audit and capture producers; `inventory_ack_is_isolated` |
| Upload lifecycle | `wyrd-storage/src/service.rs`, server `components/storage/routes.rs` | `upload_complete`, `reconcile_upload`, completion metadata, download initialization; validation and transfer coordination |
| Card lifecycle | `wyrd-server/src/components/cards/service.rs` | get/list/hydrate, `card_file`, `card_files`, manifests, activation/reconciliation, envelopes, deletion and cleanup |
| Removed SQL | `wyrd-sql/src/queries/storage/artifact_metadata.rs`, exports and drop migration | Nested SQL AGENTS; original immutable migration and fresh-install absence assertions |
| Verification consumers | `wyrd-server/src/verification/{fitter,drift,mod,runner,telemetry}.rs` | Baseline fitting and hosted artifact consumers; queued attempt defer telemetry and runtime fixture changes |
| Journeys/regressions | server `tests/integration/pg_card_declaration_route.rs`, `pg_verification_runtime.rs`; CLI `tests/card_lifecycle.rs`; SQL `pg_migration.rs`; testing `tests/bifrost/server/verification_runtime.rs` | V1–V3 new inventory journeys; V4 outbox test; changed seeds and producer regression proof |

Paths above are relative to `crates/` unless explicitly repository-relative. This is a starting map, not complete coverage or a conclusion: expand against actual source and callers.

## Authority and verification inputs

Read root AGENTS, `changes/spec-driven-development.md`, `agent_docs/maintainer-style.md`, applicable Rust/error guides, `TESTING.md`, architecture authority map and owning storage/Card/Bifrost/security sections. Only nested AGENTS in the changed tree is `crates/wyrd/wyrd-sql/AGENTS.md`.
Use the task's appended execution evidence as verification claims to assess, not acceptance conclusions. Inspect its two diagnosed verification-runtime failures and the actual fixes. No implementation summary is supplied to reviewers. Cargo-backed checks must run sequentially: coordinate with orchestrator before running them; static inspection is available in parallel.

## Selected independent discovery

- behavior-rev: comprehensive task acceptance and realistic caller-to-result paths, including regressions across the whole range.
- repo-rev: cross-tier Rust, SQL, migrations, tests and normative documentation rules, including changed-item rustdoc and capabilities.
- maintainer-rev: new inventory ownership, completion/readback workflow reuse and outbox extension; compare existing owners before accepting new orchestration.
- system-rev: ACK, deadlines, cancellation, retries, deletion/activation failure propagation and unchanged sibling producers.
- domain-data-rev: persistent inventory, replay/deletion ordering, storage reconciliation and concurrency. Covers shared state invariants, so no separate invariant-rev.
- domain-security-rev: newly privileged internal reads, tenant/path binding, public write reservation and peer destination boundary.

All six are fresh, independent discovery roles. A fresh ponytail-rev will independently validate the report union and acceptance coverage after discovery. Follow-up is conditional on a material unresolved path or conflict. Reports only are written here; implementation source stays immutable.
