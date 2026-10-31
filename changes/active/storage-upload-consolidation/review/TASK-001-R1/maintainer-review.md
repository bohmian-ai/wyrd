# Maintainer review — TASK-001

Result: **PASS** within the assigned ownership and maintainability scope. No material findings proposed.

## Immutable subject and scope

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Base: `3899b0a35`; candidate: `826fcf9c742a36bcae5e8f68307e76685d88244e`.
Reviewed the first-round cumulative diff with `git diff 3899b0a35...826fcf9c7`, expanded against candidate source and existing callers. HEAD matched the recorded candidate. There is no `.codegraph/` directory. No previous findings or acceptance evidence carried forward; no other reviewer reports were read.

Assigned scope: inventory schema/projection/recording/readback ownership, shared completion and reconciliation, Card consumers, and the existing outbox extension. This is an ownership and maintenance assessment, not an independent whole-task behavioral or resilience verdict.

Authorities read: root repository instructions; `changes/spec-driven-development.md`; approved specification revision 2 and original TASK-001; `agent_docs/index.md`, `rust.md`, and `maintainer-style.md`; `architecture/README.md`; applicable storage/Card lifecycle, server-owned Bifrost tables/outbox, and scoped internal-read sections in the design/security authorities; `TESTING.md`; SQL directory rules while tracing the removed inventory and retained transfer coordination.

## Changed-surface coverage

| Surface and changed symbols | Callers, consumers, or surrounding owner inspected | Assessment |
|---|---|---|
| `vala-bifrost-redux/src/tables/storage/objects.rs`: `ObjectsTable`, `ObjectOrigin`, `ObjectState`; built-in registration and namespace enumeration | Existing `DomainTable`, `definition`, built-in list, attributed/daily layouts, schema/layout tests | One authored schema and normal catalog path; no competing catalog or persistence abstraction. |
| `tables/staged.rs`: `Cell::Int`, fit/weight/project branches and projection test | Full `Cell` and `StagedRow` owner | Extends the existing dense scalar-row mechanism for `size_bytes`; no one-row Arrow batch or parallel encoder. |
| `wyrd-server/src/components/storage/inventory.rs`: `ObjectFact`, `ObjectFact::row`, `ObjectFact::deleted`, `CardObjects`, all `ObjectInventory` methods | All production `ObjectInventory` callers; existing audit/result staged-row projection; `SystemReadAuthority::resolve`; `ScheduledQueryCaller::run_with`; scheduled Drift use of those owners | Cohesive recorder and Card inventory reader. Schema comes from `ObjectsTable`; SQL/Arrow projection remains at the storage boundary. The handle owns server dependencies without introducing another query executor, principal issuer, cache, or worker. |
| `wyrd-storage/src/service.rs`: `upload_complete`, `reconcile_upload`, `finish_upload`, `persist_completed_upload`, download changes | Full completion entry points; backend commit, verified HEAD, size/encryption verification, upload failure marking, payload validation, download path/signing; all production completion callers | Reconciliation extends the existing completion workflow. `finish_upload` owns shared backend/validation/transfer-state work; recording stays server-owned, avoiding a reverse server dependency. |
| `components/storage/routes.rs`: `complete`, `download_init` | Shared storage service and Card file reader; completion fact construction | Route retains authorization/transport responsibility, delegates completion and recording, and uses the Card owner for download inventory. No route-local planner or transfer implementation. |
| `components/cards/service.rs`: get variants, `list_card_artifacts`, `card_file`, `card_manifests`, `card_files`, hydration/integrity, `verify_card_manifests`, envelope/deletion recording | `complete_card_inner`, activation transaction, foreground deletion, finalization/cleanup reconciliation, manifest verification, abort/cleanup helpers, existing pre-change workflow | One inventory lookup/fold path serves Card consumers. Card lifecycle retains ownership of manifest binding, activation, envelopes, and cleanup; storage completion is delegated. |
| `verification/fitter.rs`: state dependency and inventory read | `BaselineFitter::resolve`, bounded artifact loading, Card kind/state checks; hosted evidence's existing Card-listing consumer | Fitter reuses `card_file`; it does not add another inventory SQL decoder or fallback. Shared server state supplies the actual query/storage dependencies. |
| `scribe_outbox.rs`: `ScribeWrite::Object`, destination enumeration, projection, lane/weight, `submit`, `tally`, `write` | Existing frame/chunk delivery and result-summary phases; loss accounting; unchanged queued input handoff; inventory notification test | Inventory extends the current batching/local/peer delivery owner. ACK notification is tied to existing stream ownership indices and successful submission, with no second writer or generic producer barrier. |
| `grpc/capture_peer.rs`: documented destination and rejection message | Existing `ScribeTable::from_fqn` allowlist and tenant rejection | Reuses the closed destination owner and existing RPC. No parallel transport. |
| Relevant proof | New Card inventory round-trip/replay/readiness journeys; `inventory_ack_is_isolated`; extended staged-row test; built-in registration/layout assertions; task-recorded verification | Tests exercise public results or existing transport seams rather than a newly invented test framework. Verification limitations are below. |

Paths in this table are relative to their owning crates beneath `crates/`.

## Existing-owner comparison

**Catalog and projection.** The existing alternative is `DomainTable` registration plus `StagedRow`, already used by audit and verification writes. The missing behavior is the storage table declaration and an integer scalar cell. The candidate extends both. A separate storage schema registry, record-batch builder, or serializer is unnecessary and was not introduced. `ObjectFact::row` maps field names from the authored schema, while the outbox still performs batched projection.

**Read orchestration.** The existing alternative is `SystemReadAuthority` plus `ScheduledQueryCaller`, already composed by `DriftEngine`. Those owners provide table-UID-scoped authority, dispatch, stream validation and query deadlines. They do not provide storage-specific digest conversion, Card/path binding, envelope separation or deletion-dominant folding. `ObjectInventory::card_objects` adds those rules while reusing the existing query/authorization workflow. Extending the generic query caller with Card semantics would move storage policy into the wrong owner; deleting the inventory adapter would leave that missing behavior in each consumer.

**Completion.** The existing alternative is the full storage completion pipeline, not merely its signer calls. The previous Card activation path independently verified objects and marked transfer rows during activation. `reconcile_upload` now reaches the same `finish_upload` as normal completion, preserving protocol restrictions and using the same backend/HEAD/size/encryption/transfer-state operations. Card-specific manifest/upload identity checks remain with the Card lifecycle. Both server entry points stage through `ObjectInventory::record`; envelope writes use that recorder without a fabricated client session. The remaining activation transaction is still the owner of Card activation and manifest lifecycle, rather than a competing object inventory.

The retained `verify_manifest_storage` performs a HEAD before shared reconciliation performs its HEAD. This is visible extra IO, and the candidate acknowledges it. I did not elevate it to a blocker: the existing function also owns Card/manifest binding and established Card validation/error behavior; the source establishes neither a supported-use failure nor a measured performance obligation violated by the extra call. A mere preference to merge these functions is insufficient for a maintenance finding. This observation does not authorize weakening either validation boundary.

**Consumers.** The existing Card service is sufficient for Card-specific listing, hydration, download admission and fitting. The candidate adds `card_file` and `card_files` there, and the latter makes completed-manifest visibility a shared rule. The manifest read remains declaration/coordination input; it is not used to fabricate digest, size or storage metadata when inventory is absent. The public file entry remains the existing `StoredArtifactEntry`; no SDK contract or generated declaration is added by this task.

**Delivery.** The existing alternative is `ScribeSink`'s frame ACK handling and item-ownership positions. The missing behavior is notifying the upload caller for its inventory write. The candidate adds a closed write variant and oneshot notification at `tally`'s successful stream branch. It retains the same `submit`/`deliver`/local-or-peer route and the same queued-submission handoff. Notification sends synchronously without awaiting its receiver; a failed or abandoned receiver does not create a producer lifecycle owner. Existing generic settlement is correctly not reused as evidence of ACK.

## Scoped acceptance matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| REQ-002: shared storage completion and completed-object recorder | Both storage entries reach `finish_upload`; route, Card finalization, envelopes and deletions use `ObjectInventory::record` | Card completion and replay journeys; task-recorded storage/Card regression results | PASS for owner reuse |
| REQ-004–005 / INV-001: one inventory and common consumers | `ObjectsTable`, `ObjectInventory`, `card_files`, `card_file`, hydrated integrity check, fitter; removed metadata query module | Fresh-install Card journey and absent-table assertions; caller search | PASS for assigned surfaces |
| REQ-006: existing delivery path | `ScribeTable::Objects`, `ScribeWrite::Object`, existing sink projection/submission and peer destination owner | Outbox test source and task-recorded existing outbox regressions | PASS |
| REQ-007–008 / INV-003: notification does not add another producer wait owner | Only Object writes hold sender; `tally` sends only on successful submission; existing handoff remains in `write` | `inventory_ack_is_isolated` exercises held/refused/replayed/rejected/unreachable inventory writes and other lanes; queued isolation also inspected structurally | PASS for ownership/structure |
| REQ-013 / REQ-015: common bounded Card inventory lookup, scoped internal authority | `card_files`, `card_objects`, existing `SystemReadAuthority` and query deadline | Readiness/scoped-read journey source | PASS for owner reuse |
| REQ-018: deletion recording stays with existing lifecycle | `record_card_deletion` reached by explicit deletion and cleanup reconciliation; `fold` gives deletion dominance | Deletion/replayed-completion journey source | PASS for changed recording ownership; full lifecycle behavior belongs to behavioral/data review |
| Maintainer guide: discoverable capabilities, dependency ownership and minimum new machinery | Cohesive storage inventory module; existing Card, storage and Scribe owners extended | Above caller/owner comparison | PASS |

## Findings and limits

Proposed findings: **none**. No speculative extension or alternative implementation is requested.

This reviewer performed static inspection and did not run Cargo, Postgres or backend tests. The orchestrator's `verification.md`, read after source assessment, independently records passing review-time reruns of the three exact inventory journeys and the isolated ACK test at the same candidate. Broader task-recorded green results were not independently reproduced here. The execution record reports focused Card/storage, outbox, migration, catalog, fitter and CLI checks plus format/lints/boundary checks. Test bodies and production callers were inspected to assess ownership and proof relevance; runtime ACK-isolation, resilience, tenant safety and lifecycle correctness remain independently reviewed by the selected behavioral/domain/system reviewers.

Source was kept immutable. Only this report was written; no commit, push, dependency, fixture or source changes were made.
