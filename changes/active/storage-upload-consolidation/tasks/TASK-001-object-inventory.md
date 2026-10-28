---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-storage-upload-consolidation
spec_revision: 2
requirements: [REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-013, REQ-015, REQ-017, REQ-018, INV-001, INV-002, INV-003, INV-004, AC-002, AC-003, AC-006, AC-007, AC-008]
depends_on: []
---

# One completed-object inventory

## Outcome and Value

Card files and envelopes are recorded and resolved through
`bifrost.storage.objects`. A fresh installation has one completed-object
inventory. Upload retries, Card reconciliation, listing, hydration, downloads,
cleanup, and drift baseline fitting agree about the same object.

Before, completion writes Postgres artifact metadata and readers consume it.
After, the existing Scribe outbox records inventory facts and all those readers
use Bifrost, while Postgres still coordinates Cards, manifests, and transfers.

## What Changes and What Must Stay True

Own the inventory schema/layout, registration, server-managed append restrictions,
scoped internal reads, existing completion and envelope recording, deletion
recording, and replacement of every existing Postgres inventory consumer,
including verification's baseline fitter and its affected fixtures. Move the
Artifact lookup under hosted evidence with its Card-listing owner now; later
verification work may consolidate repeated lookups but must not keep SQL alive.

Completion prefers waiting only for its own Scribe acknowledgment. Prove that
isolation first; otherwise select the approved staged contract for the whole
shipped surface and document its pre-ACK loss window. Neither mode promises
immediate query visibility. Audit and all other producers retain their current
return and acknowledgment semantics. No second writer, repair scan, or queue.

The inventory records declared digests and existing metadata, not new checksum
certification. An absent query row is retryable readiness, never proof of
deletion. Replays yield one logical object, and deletion cannot be undone by
late completion. Card activation retains existing validation. Tenant ownership
and attribution remain server-derived; public append is refused.

No deployment upgrade or data migration is required. Remove obsolete inventory
SQL/query ownership from the fresh-install result, using the maintainer's explicit
unshipped scope; do not add a deployment migration or preserve a serving copy.
All contracts for expected manifests and upload sessions stay intact.

## Approach

1. Register the inventory through existing Bifrost built-in ownership and the existing Scribe destination path.
2. Establish and prove the upload acknowledgment contract without changing other producer semantics.
3. Record normal completion, reconstructed completion, reconciliation, and envelope writes through the same completed-object owner.
4. Replace all existing inventory readers and deletion writers, including fitting and test seeds; remove obsolete inventory SQL and queries.
5. Close focused journeys, schema/security proofs, and narrow registry, Bifrost, and security authority updates.

## Ordered Implementation Scenarios

### Scenario 1 — Fresh installation and Card round trip

**Behavior.** On a fresh repository Postgres and real server, upload a Card file, complete it, list/hydrate/download it, and record its envelope. Inventory separates the envelope from listed files and returns the recorded location, digest, and size. The removed SQL inventory is absent, while manifests and sessions still coordinate activation.

**RED.** Add the planned fresh-install Card journey in [V1](#v1). It must fail because completion/listing still use SQL and the Bifrost inventory does not exist.

**GREEN.** Implement the built-in and connect completion, envelopes, listing, hydration, and downloads through their current owners. Move the fitter and its direct SQL fixtures too, so removing SQL cannot break an existing consumer. Rerun V1.

**REFACTOR.** Remove superseded inventory queries and duplicated projections while V1 stays green; retain existing coordination and validation.

### Scenario 2 — Replay and deletion ordering

**Behavior.** Replay transfer completion and reconstructed completion, reconcile the same Card, delete it using the current lifecycle, then replay the old completion. The logical file list remains unique; deletion remains authoritative and bytes are deleted only by existing authorized lifecycle rules.

**RED.** Add [V2](#v2), asserting uniqueness and no resurrection rather than relying on batch-id fencing. It must fail until completion/deletion recording has logical replay semantics.

**GREEN.** Connect reconciliation and explicit deletion to the same inventory ownership and preserve logical identity across retries. Rerun V1 and V2.

**REFACTOR.** Delete parallel recording paths while preserving cleanup ordering, error handling, and replay assertions.

### Scenario 3 — Scoped inventory reads and missing visibility

**Behavior.** An authorized Card caller can list/download without a new general Bifrost permission. Foreign-tenant reads and public inventory appends fail. Missing or unavailable inventory produces a bounded typed retryable failure; it never falls back to SQL or authorizes cleanup.

**RED.** Add [V3](#v3) against the real server and its supported controlled-failure seam. Expect the missing inventory/readiness and internal scoped-read behavior to fail. Test failures must assert the exact catalog code selected from the existing catalog; add a derived code only if the approved readiness promise cannot be represented.

**GREEN.** Use existing domain authorization, table-scoped internal reader, and Bifrost query deadlines. Preserve independent public-query permissions and audited Card decisions. Rerun V1–V3.

**REFACTOR.** Reuse the same bounded lookup in consumers without broadening its principal or turning missing rows into terminal absence.

### Scenario 4 — An upload waits only for its inventory write

**Behavior.** Hold, acknowledge, reject, drop, or time out an inventory write while audit, capture, spans, direct results, and queued input handoffs proceed. ACK mode succeeds only on this write's real acknowledgment, never on settlement or loss. The request ends within its deadline; unknown outcomes are retryable. No other producer gains a wait. If isolation cannot be established, the selected staged mode returns only its documented staging guarantee and preserves existing losses.

**RED.** Add the focused controlled-outbox proof [V4](#v4). Current code lacks inventory staging and upload-specific notification. Add a real completion-API assertion to V1 for the selected mode; unit proof alone does not establish the public guarantee.

**GREEN.** Extend the existing outbox and completion boundary. Record structural and behavioral evidence for isolation, or select staged delivery consistently under the approved condition. Rerun V1–V4 and existing producer regressions.

**REFACTOR.** Remove any unnecessary generic notification machinery; never block the shared writer on an HTTP caller, drain, or unrelated producer.


## Acceptance Criteria

| Obligations | Required proof |
|---|---|
| REQ-002–006, REQ-017; INV-001; AC-002, AC-008 (Card portion) | V1: fresh installation serves files/envelopes only from Bifrost; all SQL inventory callers and seeds are removed, including fitter and CLI tests. |
| REQ-018; AC-008 (deletion) | V2: repeated completion produces no duplicate; deletion survives stale completion and existing cleanup still works. |
| REQ-003, REQ-013, REQ-015; INV-002, INV-004; AC-006–007 (inventory/Card portion) | V3: exact typed readiness/refusal, bounded failure, no fallback, no public append or tenant widening. |
| REQ-007–008; INV-003; AC-003 | V4 plus V1's real API: successful acknowledgment is genuine and isolated, or the documented staged contract is selected with the existing loss window. |

## Implementation Reference

Inspected against the current change checkout; paths guide ownership rather
than prescribe private modules or signatures.

- `vala-bifrost-redux/src/tables/verification/execution_inputs.rs::ExecutionInputsTable`,
  `tables/mod.rs::definition`, `namespaces.rs::BifrostNamespace`, and
  `gate/mod.rs` provide built-in registration, daily/attributed layouts,
  namespace enumeration, and managed-write protection. Extend this workflow for
  storage inventory; no parallel catalog.
- `wyrd-server/src/scribe_outbox.rs::{ScribeWrite, ScribeTable, ScribeSink}`
  already projects and batches local/peer writes and ACK-gates queued submission.
  Inventory and its isolated notification are missing. Extend this owner and its
  existing peer destination validation; do not reinterpret generic settlement as ACK.
- `wyrd-storage/src/service.rs::{upload_complete, persist_completed_upload,
  download_init, load_artifact_metadata}` currently inserts/reads
  `wyrd_sql::queries::storage::artifact_metadata`. Reuse full validation and
  completion, integrating server-owned recording without a reverse dependency
  from storage into the server.
- `wyrd-server/src/components/cards/service.rs::{get_card_by_uid, get_card_by_ref,
  get_latest_card, list_card_artifacts, reconcile_card_claim,
  reconcile_cleanup_claim, ensure_card_blob, write_card_blob, delete_card_by_ref}`
  own Card consumers, activation, envelopes, and deletion. Reconciliation currently
  has its own manifest/storage validation; it must reach the shared completion owner.
- `wyrd-server/src/verification/fitter.rs` reads
  `artifact_metadata::get` in baseline loading. Preserve its bounded size and
  Card-state checks while replacing the inventory lookup. Hosted
  `TenantEvidence::artifact_file` reaches the Card/listing owner.
- `wyrd-sql/src/queries/storage/artifact_metadata.rs::{insert,get,list_for_card,
  delete_for_card}`, `migrations/20260601000002_storage.sql`, query registration
  and schema assertions own the obsolete inventory. Its tests in
  `tests/integration/pg_migration.rs`, server
  `pg_verification_runtime.rs`, and CLI `tests/card_lifecycle.rs` directly
  seed or alter it. Replace those proofs at the new owner; do not weaken them.
- HTTP/Card test owner: `wyrd-server/tests/integration/pg_card_declaration_route.rs`
  uses real `Cards` and `WyrdTestServer`; its registered target is `integration`.
  Outbox unit support already provides controlled acknowledgment/refusal.
- Update only owning registry lifecycle, inventory/public-surface and scoped
  security/peer-destination authority slices. No operations cutover documentation.

## Verification and Evidence

All added selectors below are **planned**, not existing tests. Existing packages,
targets, module registration, runners, and setup were checked from Cargo manifests,
test roots, and mise tasks. After adding each test, confirm its actual exact
selector with the owning runner before RED; a zero-test selection is not proof.
Use the smallest suitable existing fixture; these selectors do not prescribe its
structure. Capture the expected behavioral RED, GREEN, regression results, and
final verification in the task's execution evidence. Run Cargo-backed checks
sequentially. Start with tracing enabled: `WYRD_LOG=info,wyrd_server=debug`.

### V1

Planned Card/inventory journey: fresh install, activation, envelope, download and chosen completion guarantee.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E 'test(=pg_card_declaration_route::completed_objects_use_one_inventory)'"
```

### V2

Planned completion/reconciliation/deletion replay journey.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E 'test(=pg_card_declaration_route::deleted_objects_survive_completion_replay)'"
```

### V3

Planned inventory readiness, authorization and append-rejection journey.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_card_declaration_route::inventory_reads_are_scoped_and_retryable)'"
```

### V4

Planned isolated acknowledgment test using existing in-process outbox test support.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::inventory_ack_is_isolated)'
```

### Owner checks and static proof

Run the affected existing Card and storage module regressions via focused
nextest module selectors, then `mise run check:storage:drift`,
`mise run check:tenant-isolation`, `mise run check:deps`,
`mise run fmt`, `mise run lints`, and `git diff --check`.
Run `mise run codegen:check` if contracts change. Prove fresh-install schema
absence with the SQL integration owner, not a new text-scanning gate.
Enumerate remaining references to the removed inventory as a one-time diff audit.
Retain coverage of all replaced direct-SQL fixtures, including drift fitting
and CLI lifecycle failures. These are static/regression obligations, not
manufactured RED tests.

SDK-facing inventory behavior remains behind the existing Card API. Carry its
Rust/Python/TypeScript and CLI regressions into integrated change review;
TASK-002 owns the changed upload projections, TASK-003 owns changed evidence
readback. Do not run every language suite per task.

## Material Stop Conditions

Revalidate against the combined PR 216 and rebased benchmark baseline before
implementation. Private renames and equivalent non-weaker commands are local
adaptations, recorded with evidence. A missing baseline or a materially different
workflow requires replanning before affected work. Stop for a specification
revision if correctness requires another serving inventory, a new durable queue,
new hashing or immutable promotion, changed tenant/permission semantics, changed
retention, deployment migration, or a new public/persisted decision beyond the
approved contract. Do not invent compatibility aliases or waive a red check.

## Authority Links

- [Approved specification, revision 2](../spec.md).
- [Repository rules](../../../../AGENTS.md).
- [Spec-driven development](../../../spec-driven-development.md).
- [Implementation execution](../../../implementation-execution.md).
- [Testing and exact selectors](../../../../TESTING.md).
- [Wyrd protocol](../../../../architecture/wyrd-design.md).
- [Bifrost authority](../../../../architecture/bifrost-design.md).
- [Security posture](../../../../architecture/wyrd-security-posture.md).

