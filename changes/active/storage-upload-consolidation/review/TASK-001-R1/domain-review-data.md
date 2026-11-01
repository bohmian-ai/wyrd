# Persistent data domain review

Result: **FAIL**. Two material gaps remain in making the new inventory the completed-object authority. Both can be corrected inside the existing Card read/cleanup owners without a new storage design.

## Subject and reviewed boundary

Repository `/Users/stevenforrester/Documents/GitHub/wyrd`; immutable first-review range `3899b0a35...826fcf9c742a36bcae5e8f68307e76685d88244e`. HEAD was checked and equals that candidate. Original TASK-001 and approved storage-upload-consolidation spec revision 2 govern this review. No prior findings or acceptance evidence carried forward. No other reviewer reports were read, and no implementation source was edited.

Scope: table identity/schema/layout, inventory producers and folding, upload completion/reconciliation, Card activation/deletion/compensation, replacement consumers, and fresh-install SQL removal. The separate security and system reviewers own authorization and ACK/failure-propagation depth; this report checks their effects on persistent facts and lifecycle transitions. File-upload adoption and canonical evidence consolidation assigned to later tasks are not missing behavior in TASK-001.

## Authority and source coverage

| Authority | Reviewed obligation and source coverage |
|---|---|
| Root AGENTS.md; SQL AGENTS.md; agent_docs/index.md, rust.md, errors.md, maintainer-style.md; TESTING.md | Existing owner reuse, tenant transaction boundaries, immutable migrations, meaningful focused proof and journey coverage. |
| changes/spec-driven-development.md and wyrd-task-review skill | Full immutable subject, independent discovery, material scenarios, no speculative hardening or design expansion. |
| Approved spec REQ-002–008, REQ-013, REQ-015, REQ-017–018; TASK-001 | Shared completion, sole inventory, retry identity, deletion domination, preserved validation/coordination, fresh-install removal. |
| architecture/wyrd-design.md: Card blob state and Heavy artifacts (2259–2297) | Blob pointer remains control-plane state; completed-object metadata resides in Bifrost; activation verifies uploads first. |
| architecture/bifrost-design.md: daily partitioning and server-managed inventories (1278, 1745–1780) | Daily recorded_at layout, managed attribution, no public writes, Card-bound deletion dominates completion, ACK does not guarantee immediate query visibility. |
| architecture/wyrd-security-posture.md: server capture destination (83 onward) | Existing authenticated capture path extended with one destination; no parallel writer or data authority. |

Source inspected: `tables/storage/objects.rs`, `tables/{mod,staged}.rs`, `namespaces.rs`, Gate reservation, Forge namespace constraint and row-type change; server `components/storage/{inventory,routes}.rs`, complete Card service lifecycle bodies and their SQL lifecycle/upload queries; `scribe_outbox.rs` projection/notification changes; `grpc/capture_peer.rs` destination change; baseline fitter and hosted `TenantEvidence::{inventory,artifact_file}` consumers; removed SQL module/exports/migration and SQL absence assertions; new V1–V3 journeys, existing reconciliation/deletion/failed-blob journeys, changed CLI and baseline inventory fixtures. Diff and reference search found no remaining serving caller of the removed artifact metadata queries.

## Existing owners, sibling writers, and consumers

- `ObjectsTable` extends the existing built-in definition list, namespace enum, daily attributed layout, and staged-row projection. Int64 is added to the existing `Cell`/`StagedRow` machinery, not another serializer.
- `ObjectInventory` uses the process Scribe outbox and existing scheduled/system table reader. Production completed facts are produced by the normal storage completion route, `verify_card_manifests` after `reconcile_upload`, and `write_card_blob`. Existing explicit deletion and `reconcile_cleanup_claim` call `record_card_deletion`. Tests seed through the same recorder.
- `upload_complete` and `reconcile_upload` converge on `finish_upload`, backend completion/head verification, and `persist_completed_upload`. Postgres retains transfer status and manifest verification; its former inventory insertion is gone. `mark_completed` accepts pending/completed under its row lock and retains first completion time. The existing activation lock and reconciliation lease lock remain final transition fences.
- `card_files` is the shared inventory lookup for get-by-UID, get-by-reference, latest, artifact listing, downloads, fitting, and hosted artifact selection. `hydrate_card` consumes its result but ignores the envelope portion. This is the read-side gap below.
- All physical Card cleanup routes converge on `cleanup_card_artifacts`, including explicit deletion, cleanup claims, declaration compensation (`abort_card`), and the abandoned-upload branch of `reconcile_card_claim`. Only the first two currently record deletion facts. This is the writer-side gap below.

## Acceptance and realistic scenario matrix

| Obligation/scenario | Implementation evidence | Verification evidence assessed | Result |
|---|---|---|---|
| REQ-004: one built-in schema, metadata/origin/state, daily partitions and lookup Blooms | ObjectsTable fields, attributed_layout(daily_layout, recorded_at, storage_path/sha256); managed subject_card_uid provides Card binding | Table vocabulary/layout tests and V1 source; execution evidence reports catalog tests green | PASS |
| REQ-006: reuse one delivery owner | Object variant projects through existing ScribeTable/Rows/Frame and local/peer delivery; no replacement queue/table | V4 source and unchanged sibling writer paths | PASS |
| REQ-002/003: normal/reconstructed completion preserve validation | Both storage entries use finish_upload, verified_object_head, verify_object_head and existing SQL completion owner; Card verify_manifest_storage remains | Existing storage tests and card_reconciler_recovers_after_storage_retry source; task reports 43 affected journeys green | PASS |
| REQ-004: replay produces one logical Card file | fold inserts by validated Card-relative path; producer storage paths are deterministic tenant/Card/path; repeated facts collapse | deleted_objects_survive_completion_replay records normal, reconstructed, repeated and stale facts | PASS |
| REQ-018: known explicit deletion dominates later completion | fold remembers any deletion before clearing files/envelope, independent of batch order; explicit delete and cleanup claim stage same tombstone | V2 asserts deleted flag, empty files, absent envelope after stale fact | PASS |
| REQ-018: failed-declaration compensation also reflects removed bytes | Two cleanup-success callers skip recorder and mark cleanup done | Existing V2 covers explicit delete only; source-proven failed/expired sibling scenario below is uncovered | FAIL: DATA-2 |
| REQ-005/013: hydration cannot serve an envelope absent from inventory | card_files only checks complete manifest files; hydrate_card directly reads row.card_blob_uri | V1 asserts envelope exists on healthy path; V3 alters a file manifest, not missing envelope | FAIL: DATA-1 |
| REQ-005/013: missing completed file is retryable; no SQL serving fallback | card_files tests complete manifests against Bifrost files; card_file and download/fitter/hosted use that owner | V3 asserts 503 QUERY_VISIBILITY_UNAVAILABLE and query stream error; no serving artifact_metadata calls remain | PASS |
| REQ-017: fresh install removes SQL inventory and preserves transfer/manifest control plane | New immutable DROP migration; old create migration retained; query module and exports removed; Forge namespace CHECK extended | V1 and pg_migration absence assertions; task reports 22 SQL tests green | PASS |
| No new dedup service, hashing, GC, immutable promotion or cross-owner physical sharing | Diff retains deterministic Card object paths and existing cleanup/validation owners | Static full-range inspection of assigned boundary | PASS |

## Material proposed findings

### DATA-1 — Hydration serves an envelope without its inventory record

Classification: **INCORRECT**. Violated obligations: REQ-004/005/013, TASK-001 sole inventory and missing-visibility behavior.

Location: `crates/wyrd/wyrd-server/src/components/cards/service.rs:371` (URI-based envelope read through 390), with the incomplete inventory readiness check at 341–349.

Producer-to-consumer proof: `write_card_blob` writes bytes and records an envelope fact; activation retains its URI in Postgres. `ObjectInventory::card_objects` queries completed/deletion facts and `fold` returns `envelope: None` when no envelope fact is in the returned cut. ACK explicitly does not promise unconditional read availability. All three hydrated getters call `card_files`, which checks only declared completed artifact paths. A metadata-only Card has no such paths; a heavy Card can have visible file facts while its later envelope fact is absent. Both cases pass. `hydrate_card` ignores `inventory.envelope`, validates the Postgres pointer, and fetches bytes from it. Its later integrity check only consumes `inventory.files`.

Observable consequence/materiality: a supported Card read succeeds from a retained pointer and bytes even when the inventory does not establish its envelope completion. This is exactly the alternate-authority fallback excluded by REQ-005, not a request for stronger digest validation or immediate visibility. The existing healthy V1 assertion cannot distinguish a required dependency from an unused envelope record.

Smallest testable correction: use the already-returned envelope fact at the hydrated Card read boundary; require its presence and exact binding to the Card/pointer before object IO, returning the existing typed retryable visibility error for an absent fact. Preserve existing pending-Card/not-found behavior and path/identity/integrity validation. No additional inventory query, metadata table, cache, hash, or upload session is needed. This guard belongs in hydration because file listing/download need only the selected file facts, whereas hydration consumes the envelope. A focused negative read test with a valid active pointer/bytes and missing envelope fact must return readiness and avoid payload IO; a matching recorded envelope must preserve the read.

Correction burden: small local use of the existing CardObjects.envelope projection plus focused negative coverage. Deleting envelope recording would violate the approved one-inventory requirement; keeping only file readiness cannot establish envelope completion.

### DATA-2 — Successful compensation clears cleanup before recording deletion

Classification: **MISSING**. Violated obligations: REQ-018 and TASK-001 replacement of existing deletion writers; governing architecture says every completed artifact, envelope and deletion is an inventory fact.

Locations: `crates/wyrd/wyrd-server/src/components/cards/service.rs:1952` and `:2295`; shared success clearing at `:2669–2672`.

Reachable scenario: declare a supported multi-file heavy Card. Complete file A through the normal storage endpoint, which ACKs its CardFile fact. Let file B fail or its pending upload expire. The production reconciler's `manifest_needs_cleanup` branch sees the failed/expired sibling; `cleanup_card_artifacts` calls `cleanup_manifest_artifact`, whose completed case physically deletes A. With successful backend cleanup, `commit_card_failure(..., true)` transitions the pending Card to failed and immediately calls `mark_card_reconciliation_succeeded`. No deletion fact was staged, and no cleanup retry remains. Declaration compensation through `abort_pending_declaration_cards` → `abort_card` has the same omission when already-created facts are cleaned after a later initialization/completion failure.

Observable consequence/materiality: the sole inventory permanently reports an available completed object whose bytes the authorized lifecycle has removed, and a delayed completion fact has no deletion fact to dominate it. Public Bifrost inventory reads are supported, so the inconsistency is observable even though normal Card getters reject a failed Card. Explicit Card delete and `reconcile_cleanup_claim` already call `record_card_deletion`, but neither runs after this successful failure cleanup clears reconciliation. This is an explicit lifecycle-recording requirement, not a new GC/retention guarantee.

Smallest testable correction: extend the existing cleanup-success boundary to record the same Card deletion fact through `record_card_deletion` for compensation and abandoned-upload cleanup too. Preserve the failed Card transition and leave cleanup retryable if inventory recording fails; do not mark cleanup succeeded until the required deletion fact meets the selected delivery contract. Reuse `cleanup_card_artifacts`/existing cleanup coordination and recorder rather than adding a writer, scan, or downstream deletion checks. Check all four sibling cleanup callers once. A focused multi-file failed/expired sibling test must establish A completed, run the real existing cleanup path, assert A bytes gone and Card failed, then assert inventory deleted/empty even after replaying A's old fact. Also exercise inventory recording failure at that success boundary to show cleanup is retried rather than forgotten.

Correction burden: bounded wiring of the existing recorder into two missing cleanup outcomes and focused failure/replay proof. No new data model or concurrency semantics are required. A reader-side filter on failed Cards would leave the canonical inventory fact wrong and would not satisfy REQ-018.

## Verification limits

I independently inspected test bodies and traced the required scenarios against candidate source. I did not run Cargo/Postgres checks during this specialist pass; shared Cargo serialization is coordinated by the orchestrator. The task records passing focused V1–V4 and owner lanes, but those results do not exercise either missing outcome above. No load generation, cloud backend checks, deployment upgrade/backfill, or later-task File upload/evidence behavior was added to scope.

No other material persistent-data findings were retained. In particular, append facts may repeat physically while the served Card list folds by identity; no uniqueness service is required. Record ordering does not defeat a tombstone that exists, so no timestamp ordering or new lock was prescribed. Fresh-install DROP through a new migration complies with the SQL authority requiring immutable historical migrations.

Overall result: **FAIL** pending independent validation of DATA-1 and DATA-2.
