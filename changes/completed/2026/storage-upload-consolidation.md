# Shared uploads and one Bifrost object inventory

- Change ID: `SPEC-storage-upload-consolidation`, approved revision 2.
- Completed: 2026-10-29.
- Integrated review target: `462ebee97c81772103054f7d090053ac7054bafa`, base `3899b0a35ec2eede7bf294db0cf66b27defa1498`; verdict FIX_REQUIRED.
- R1 remediation candidate: `493061d3c`; completion target `ecfb8cff0`, which adds the review records.
- Delivery reference: branch `canonical-evidence`, stacked on PR 221 (shared File uploads) → PR 218 (storage inventory) → `change/benchmark-rev19` / PR 217.
- Completion authority: explicit maintainer direction to close out after the R1 bounded re-review. No final-change-review PASS is claimed; R2 and R3 remain open (see below).

## Intent and shipped behavior

Artifact files and verification `File` inputs used separate upload and recording
workflows, and verification read File bytes from a digest-derived path with no
completion record. Both now share one upload lifecycle and one completed-object
inventory, so evidence resolves from recorded facts instead of storage guesses.

- Artifact files and File uploads use the same storage initialization, planner
  (local, single-PUT, multipart, Azure), client transfer dispatcher and
  completion owner. Card reconciliation and server-written envelopes record
  through the same completion recorder.
- `bifrost.storage.objects` is the sole completed-object inventory for Card
  files, File uploads and Card envelopes: tenant-scoped, server-managed,
  daily-partitioned, with Blooms on storage identity, declared digest and Card
  binding. `wyrd.storage_artifact_metadata` is dropped.
- Upload completion waits only for its own inventory record's Scribe ACK, within
  the request deadline; an unknown outcome is retryable, never a success.
- File planning reuses only same-tenant, File-origin, size-matched completed
  records. Verification resolves File and ArtifactRef evidence from the
  inventory once per execution; missing or unreadable records return a typed
  retryable readiness failure.
- The evidence-only local route and planner are removed with no aliases.

## Lasting invariants and constraints

- One completed-object authority; Postgres keeps Card metadata, expected
  manifests and upload sessions as intent and coordination only.
- Inventory records ride the existing process Scribe outbox and peer capture
  channel; the peer allowlist adds only the inventory destination. No other
  producer gains an ACK wait; audit stays non-blocking.
- Transport and completion retries fold to one logical object; recorded
  deletion dominates a delayed completion replay.
- A recorded digest is the declared digest. Existing size, encryption and
  read-time digest/format checks are preserved; nothing new certifies content.
- Shared endpoints select permission from the persisted upload owner: Card
  write for Card uploads, `verifier:run` for File uploads. Internal inventory
  reads use table-scoped server read authority; Gate refuses public appends.
- Verification inputs, outputs and results keep their separate benchmark
  owners; response assembly is never a persisted evidence copy.

## Decisions, revisions and deviations

- Upload-only ACK mode shipped, not the approved staged fallback: isolation was
  proven with a controlled delayed/refused-ACK test.
- Revision 2 removed migration, backfill and drain: nothing had shipped, so the
  change targets fresh installations only.
- Benchmark measurement remediation was excluded by the maintainer. Timing of
  upload completion and verification is unmeasured.
- **Open at completion.** Integrated review findings left unresolved, recorded
  in `review/change-462ebee97/TASK-CHANGE-R2-inherited-scribe-gates.md` and
  `TASK-CHANGE-R3-integrated-test-boundaries.md` at `ecfb8cff0`:
  - R2: FIND-CHANGE-002 (restart cache-reservation accounting), the Scribe
    panic-doc subset of 003, 005 (publication-budget bound) and 007
    (failed-replay snapshot boundary).
  - R3: the SDK fixture subset of 004, 006 (producer settlement before faults),
    008 (canonical output reader), 009 (group input count), 010 (settled
    source/read shape) and 011 (PTY echo observer).

  These are test-expectation, observation-boundary and documentation gates in
  inherited and integrated tests, not established production defects. Until
  they are fixed, the aggregate `mise run gate` stays red: Wyrd family,
  Bifrost/MCP journeys, Rust SDK, Python and TypeScript integration, and
  installed CLI journeys.

## Acceptance closure

| Criterion | Evidence | Result |
|---|---|---|
| AC-001 shared transfer | Local, S3 multipart, GCS single-PUT and Azure block-blob journeys; real-GCS multipart not run | PASS within backend proof |
| AC-002 inventory ownership | `completed_objects_use_one_inventory`, `deleted_objects_survive_completion_replay` | PASS |
| AC-003 ACK isolation | `inventory_ack_is_isolated`; queued delayed-ACK path is a structural proof | PASS with limit |
| AC-004 evidence consumption | `file_inputs_use_shared_uploads`, `evidence_metadata_is_resolved_once` | PASS |
| AC-005 canonical queries | Direct, submitted, group and scheduled readback on Rust, Python, TypeScript and MCP | PASS on source audit |
| AC-006 delayed visibility | `inventory_readiness_preserves_execution_bounds`; corruption cases | PASS |
| AC-007 isolation and permissions | Foreign-tenant, Card-path, append and exact-grant negatives; R1 scrubbed spans on `part_url`, `abort`, `local_blob`; MCP `staging_pressure_is_retryable` | PASS |
| AC-008 fresh install and deletion | Fresh-schema proof, compensation/replay, OpenAPI route absence | PASS |
| Repository gates | Aggregate gate red; R2/R3 open | FAIL, accepted by maintainer |

## Links

- Architecture: [Verifier](../../../architecture/wyrd-design.md#verifier),
  [registry lifecycle](../../../architecture/wyrd-design.md#registry-lifecycle),
  [Bifrost public surface](../../../architecture/bifrost-design.md#public-surface),
  [security posture](../../../architecture/wyrd-security-posture.md#principal-and-credential-lifecycle).
- Owners: [storage service](../../../crates/wyrd/wyrd-storage/src/service.rs),
  [storage routes](../../../crates/wyrd/wyrd-server/src/components/storage/routes.rs),
  [client storage](../../../crates/shared/wyrd-client/src/storage),
  [Cards service](../../../crates/wyrd/wyrd-server/src/components/cards/service.rs),
  [Scribe outbox](../../../crates/wyrd/wyrd-server/src/scribe_outbox.rs),
  [inventory table](../../../crates/vala/vala-bifrost-redux/src/tables/storage/objects.rs),
  [input resolver](../../../crates/wyrd/wyrd-server/src/verification/inputs.rs),
  [hosted evidence](../../../crates/wyrd/wyrd-server/src/verification/hosted/evidence.rs),
  [inventory drop migration](../../../crates/wyrd/wyrd-sql/migrations/20261028000000_drop_storage_artifact_metadata.sql),
  [File upload sessions migration](../../../crates/wyrd/wyrd-sql/migrations/20261029000000_file_upload_sessions.sql).
- Tests: [Card declaration route](../../../crates/wyrd/wyrd-server/tests/integration/pg_card_declaration_route.rs),
  [hosted artifacts](../../../crates/wyrd/wyrd-server/tests/integration/pg_hosted_artifacts.rs),
  [verification runtime](../../../crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs),
  [MCP verification](../../../crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs).
