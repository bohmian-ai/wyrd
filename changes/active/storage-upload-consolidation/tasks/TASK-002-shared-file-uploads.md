---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-storage-upload-consolidation
spec_revision: 2
requirements: [REQ-001, REQ-003, REQ-007, REQ-010, REQ-013, REQ-015, REQ-016, REQ-017, INV-002, INV-003, INV-004, AC-001, AC-003, AC-004, AC-006, AC-007, AC-008]
depends_on: [TASK-001]
---

# File inputs use shared uploads

## Outcome and Value

A verification File uses the same initialization, transfer planning, dispatcher,
and completion as a Card file, including configured multipart and Azure plans.
Its completed upload is recorded in the shared inventory. Calling
`verify(File(...))` still feels like the same SDK operation; the server's
completion guarantee is the one established by TASK-001.

Before, File preparation obtains only local/single-PUT plans and skips completion.
After, it uses the full storage lifecycle without creating a Card or requiring
Card-write permission.

## What Changes and What Must Stay True

Own File preparation, its shared storage transport authorization, Rust-client
orchestration, HTTP/MCP projections, Rust/Python/TypeScript SDK projections and
CLI callers affected by these contracts. Preserve Artifact authoring interfaces,
File/ArtifactRef declaration shapes and exact invocation authorization.

Reuse is allowed only for a completed tenant File-upload record matching declared
digest and size. A matching Card file or envelope cannot supply File ownership.
Object length alone does not establish completion. Missing inventory during
verification remains bounded retryable readiness. This task must connect the
existing File byte reader to the recorded inventory location immediately:
a shared upload cannot be delivered while its verifier still guesses a path.
TASK-003 later owns broader reuse of resolved metadata and canonical readback.

Existing file/aggregate limits, local body digest validation, encryption and size
checks remain. Completion does not add independent remote digest verification.
Upload admission retains operation-level `verifier:run`; storage follow-up
operations use server-validated ownership. File callers need neither
Card-write nor general Bifrost-query privilege. A write scope on MCP supplements
domain authorization and cannot replace it. No caller chooses another tenant.

No compatibility route aliases, alternate planner, provider-specific SDK upload
loop, new completion mode, hashing pass, retention policy, migration, or physical
sharing between Card and File lifetimes.

## Approach

1. Extend existing storage initialization and session ownership to admit File uploads through the existing permission-aware evidence adapter.
2. Replace object-length reuse with tenant File-only inventory lookup and shared plan/completion.
3. Route both path and in-memory File sources through the shared Rust client dispatcher, preserving streamed transfer and existing Artifact authoring.
4. Resolve File byte reads through recorded locations so the upload-to-verification journey works at this task boundary.
5. Update HTTP/MCP, SDK bindings/exports/stubs/generated contracts, CLI callers and examples; remove obsolete local evidence route/contracts.
6. Exercise backend variants and permission/validation failures, and document the selected completion guarantee consistently.

## Ordered Implementation Scenarios

### Scenario 1 — File transfer and completion

**Behavior.** Upload path-backed and in-memory Files through local and supported configured remote protocols. A new File has a shared upload session, completes through the shared owner and becomes an inventory record; its Verifier reads the recorded location and returns the expected judgment. A File above the multipart threshold follows the same planner policy as an Artifact.

**RED.** Add planned File upload-to-verification journey [V1](#v1) and mirrored SDK journeys [V4](#v4)–[V6](#v6). Current File transfer skips completion and rejects multipart/Azure plans. Confirm actual new selectors before RED.

**GREEN.** Extend existing initialization, ownership and Rust upload orchestration. Preserve the shared dispatcher and validate File bytes through the existing evidence reader after inventory resolution. Rerun the focused journey after each protocol extension.

**REFACTOR.** Remove evidence-only upload orchestration while preserving source streaming, transfer retries and common completion.

### Scenario 2 — Only completed File uploads are reusable

**Behavior.** Repeat the same tenant File digest/size and reuse its completed File record. A storage object with matching length but no completion, a foreign-tenant match, an identical Card file/envelope, a known-deleted record, or a size mismatch never qualifies. Delayed/failed inventory queries produce the approved bounded readiness behavior rather than a guessed path.

**RED.** Add [V2](#v2) with explicit reuse and refusal/readiness cases. It must fail while the preparation path trusts storage HEAD or ignores inventory origin.

**GREEN.** Use the inventory owner from TASK-001 for preparation and File reads, preserving declaration and byte-validation checks. Rerun V1–V2.

**REFACTOR.** Reuse one lookup policy instead of creating a digest cache or another persisted evidence map.

### Scenario 3 — Ownership controls every upload operation

**Behavior.** A caller with only an exact Verifier run grant can stage File evidence and use its authorized init/local/part/completion/abort operations, but cannot write a Card or invoke another Verifier. Forged ownership, foreign upload IDs, missing permission, oversized aggregate/files, invalid local digest and invalid completion all fail with their exact catalog codes. Each permission check keeps its audit boundary.

**RED.** Add [V3](#v3) and MCP journey [V7](#v7). Current shared routes demand Card-write, and current evidence contracts lack shared upload operations. Assert existing negative checks too; do not relax validation to pass a protocol case.

**GREEN.** Select permissions from validated persisted upload ownership and domain admission, not arbitrary request paths. Preserve explicit MCP write scopes and read discoverability. Rerun V1–V3 and V7.

**REFACTOR.** Keep adapters thin and remove obsolete evidence-local endpoints once every caller uses the shared route.

### Scenario 4 — Public projections agree

**Behavior.** Rust, Python, TypeScript, HTTP, MCP and affected CLI paths report the same chosen completion guarantee. Artifact authoring and file-list output survive. Runtime schemas/catalogs omit the removed local evidence route; authoring does not require new low-level upload plumbing.

**RED.** Extend V4–V7 and add the planned CLI regression [V8](#v8). Compare served OpenAPI and MCP contracts against actual mounted routes. A mismatched generated type, stale route or different completion claim must fail its owning proof.

**GREEN.** Update source contracts and thin bindings, regenerate generated outputs, author Python stubs/exports and update owning examples/docs. Rerun changed focused journeys.

**REFACTOR.** Remove obsolete contracts and import/export paths; do not preserve aliases or expose server internals.


## Acceptance Criteria

| Obligations | Required proof |
|---|---|
| REQ-001, REQ-003, REQ-016; INV-004; AC-001 | V1 and backend regressions: both upload origins use existing planner/transfer/completion; byte/aggregate limits and local digest checks survive. |
| REQ-010 (planning and File byte location), REQ-013; AC-004, AC-006 | V2 plus V4–V6: completed same-tenant File reuse only, no HEAD-based completion or path guess, recorded location feeds verification. |
| REQ-015; INV-002; AC-007 | V3 and V7: exact Verifier grant admits File staging, foreign/forged ownership is refused and audited, invocation remains exact. |
| REQ-007, REQ-016–017; INV-003; AC-003, AC-008 (File portion) | V4–V8 and served contracts: one completion guarantee, existing Artifact ergonomics, obsolete route removed, fresh File upload records in the sole inventory. |

## Implementation Reference

- `wyrd-storage/src/service.rs::{upload_init, upload_part_url, upload_local_blob,
  upload_complete, upload_abort}` is the full lifecycle. The separate
  `evidence_upload_plan` only presigns single PUT or the evidence-local route;
  `upload_local_evidence` validates local digest but skips coordination/completion.
  Extend full lifecycle ownership and remove that duplicated path.
- `wyrd-server/src/components/verification/service.rs::VerificationControl::evidence`
  enforces file/aggregate limits and uses `object_len` to skip existing content;
  `upload_local_evidence` supplies current local permission/digest behavior.
  Keep preparation as admission into the shared lifecycle, replace HEAD reuse.
- `wyrd-server/src/components/storage/routes.rs::{init,...}` currently authorizes
  Card writes before storage calls. Session continuation must derive Card/File
  permission from validated ownership while retaining tenant checks.
- `wyrd-client/src/storage/mod.rs::WyrdStorageClient::{upload_artifact,
  upload_artifact_with_progress,upload_evidence}` owns orchestration.
  `upload_evidence` rejects multipart/Azure and never completes.
  `storage/upload/mod.rs::{dispatch,FileSource,ArtifactSource}` already streams
  all supported protocols. Reuse the complete workflow, not a second transfer loop.
- `wyrd-client/src/verify/mod.rs::{File::pin,Projected}` and the
  `EvidencePlanRequest` call currently hash/pin File declarations, obtain plans,
  and call `upload_evidence`. Retain authoring, stream hashing and slot validation
  while carrying shared upload identity/completion.
- `wyrd-server/src/verification/hosted/evidence.rs::TenantEvidence::read`
  currently calls `tenant_path::evidence` for Files. Replace its location source
  now and preserve `stored` length/digest checks and format sniffing.
- `wyrd-spec/src/storage/{upload,artifact}.rs` and
  `verification/execution` own wire data. Follow their generators, not edited
  generated outputs. Server contracts also reach MCP catalog, CLI, the Python
  `src/`, public package/exports/authored stubs, and TypeScript
  `native/src/verification.rs` and generated napi declarations.
- Inspected test owners: server `storage_e2e.rs` (requires
  `storage-emulator`, real server and backend lanes), `pg_verification_routes.rs`
  (local real-server negative flows); SDK story
  `verifier_building_blocks` in Rust/Python/TypeScript; MCP
  `tests/bifrost/mcp/verification.rs::pg_tests`; CLI
  `tests/card_lifecycle.rs` under registered `cli` target.
  Existing emulator tasks supply backend-specific setup. Keep provider
  credentials/emulators in their established lanes.
- Reconcile Verifier upload route/lifecycle prose, protocol examples and affected
  machine-readable documentation. Respect SDK and docs directory rules.

## Verification and Evidence

All test names below are **planned**. Confirm actual exact selectors after
adding each test and before RED; a zero-test selection is not evidence.
Targets/modules are checked from existing test roots and Cargo manifests.
Journeys use real `WyrdTestServer` and repository Postgres. Run Cargo commands
sequentially, with `WYRD_LOG=info,wyrd_server=debug`.

Before language journeys, use the owning build setup: `mise run build:server`,
`mise run py:setup`, `mise run ts:build`, and `mise run ts:build:testing`
as applicable. These mirror the dependencies of existing language journey tasks.

### V1

Planned local File lifecycle journey, including actual verification.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E 'test(=pg_verification_routes::file_upload_uses_shared_completion)'"
```

### V2

Planned inventory-only File reuse and readiness matrix.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E 'test(=pg_verification_routes::file_reuse_requires_completed_file_inventory)'"
```

### V3

Planned ownership, limits, local digest and follow-up authorization journey.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration --test-threads=1 -E 'test(=pg_verification_routes::file_upload_authority_and_validation_are_preserved)'"
```

### V4

Planned Rust SDK story regression.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::file_inputs_use_shared_uploads)'"
```

### V5

Planned matching Python SDK story.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q tests/integration/test_verifier_building_blocks.py::test_file_inputs_use_shared_uploads'
```

### V6

Planned matching TypeScript SDK story.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts --testNamePattern "^file inputs use shared uploads$"'
```

### V7

Planned MCP File planning/completion and exact-grant journey.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::file_inputs_use_shared_uploads)'"
```

### V8

Planned CLI Artifact regression, using its existing registered binary and module.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::pg_tests::artifact_upload_keeps_shared_completion)'"
```

### Protocol, contract and owner checks

Run `mise run test:storage:e2e` after extending its existing backend journeys
to cover File as well as Card transfers; it provides Postgres and the
RustFS/fake-GCS/Azurite configuration. Extend its current explicit selectors
when adding protocol tests; no test may be omitted accidentally. Real GCS
multipart retains the existing credentialed lane; report unavailable credentials
as missing proof, not a claimed pass.

Run `mise run codegen:regen`, `mise run codegen:check`,
`mise run ts:napi:check`, `mise run py:lints`,
`mise run py:typecheck`, `mise run ts:typecheck`,
`mise run check:deps`, `mise run check:tenant-isolation`,
`mise run check:storage:drift`, `mise run fmt`, `mise run lints`,
and `git diff --check` for the resulting write set. Use Python/TypeScript
format tasks when their sources change. Check actual mounted OpenAPI through
the existing `pg_openapi_contract` module and MCP catalog through V7;
codegen alone does not prove runtime contracts. Static outputs need no
manufactured RED. Run focused affected Rust-client upload unit regressions
rather than full SDK suites.

## Material Stop Conditions

Revalidate TASK-001's inventory and selected completion contract before changing
File preparation. Stop if implementation requires a different declared File or
ArtifactRef shape, broader permissions, a second upload implementation, a
separate queue/inventory, cross-owner physical reuse, or relaxed local digest
validation. These require specification authority. Missing private plumbing or
equivalent test setup is an implementation adaptation, not a new design round.
The combined baseline and Artifact authoring workflow must survive.

## Authority Links

- [Approved specification, revision 2](../spec.md).
- [Task 1 dependency](TASK-001-object-inventory.md).
- [Repository rules](../../../../AGENTS.md).
- [Spec-driven development](../../../spec-driven-development.md).
- [Implementation execution](../../../implementation-execution.md).
- [Testing](../../../../TESTING.md).
- [Wyrd Verifier authority](../../../../architecture/wyrd-design.md#verifier).
- [Security posture](../../../../architecture/wyrd-security-posture.md).
- [Python rules](../../../../sdks/wyrd-sdk-python/AGENTS.md).
- [TypeScript rules](../../../../sdks/wyrd-sdk-ts/AGENTS.md).


## Implementation Evidence

Commits `fa0870703`..`4ab2bdf1d` on `shared-file-uploads`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001, REQ-003, REQ-016; INV-004; AC-001 | `wyrd-storage/src/service.rs` (Card-less `upload_init`, `upload_owner` with declared size); `components/verification/service.rs::evidence` plans through the shared planner with file/aggregate limits; `storage/routes.rs::local_blob` bounded by declared size, exempt from the 1 MiB body limit (`http/middleware/body_limit.rs`) | V1 `pg_verification_routes::file_upload_uses_shared_completion`; V3 SHA256/SIZE mismatch and per-file/aggregate 413; `storage_e2e::{local_client_server_round_trip, s3_multipart_e2e, gcs_single_put_e2e, azure_multipart_e2e}` now complete a File per backend (`mise run test:storage:e2e`) | PASS |
| REQ-010, REQ-013; AC-004, AC-006 | `components/storage/inventory.rs::file_object` (completed, undeleted, same-tenant `file_upload` fact matching digest and size); `verification/hosted/evidence.rs::file_object` reads the recorded location and re-checks length, digest, and limit | V2 `file_reuse_requires_completed_file_inventory` (uncompleted bytes, Card file/envelope facts, foreign tenant, size mismatch, deletion, inconclusive readiness); `pg_hosted_files::*` (5 tests); V4 Rust, V5 Python, V6 TypeScript `file_inputs_use_shared_uploads` | PASS |
| REQ-015; INV-002; AC-007 | `storage/routes.rs::authorize_upload` derives `cards:write` or any `verifier:run` from the persisted owner after a tenant-scoped lookup (foreign → 404) | V3 `file_upload_authority_and_validation_are_preserved` (exact-grant operations, Card-write denial, foreign 404, malformed 400, audited decisions); V7 `wyrd-mcp` `file_inputs_use_shared_uploads` (exact role; other Verifier refused) | PASS |
| REQ-007, REQ-016–017; INV-003; AC-003, AC-008 (File portion) | Local evidence route, `upload_evidence`, `upload_local_evidence`, `evidence_upload_plan` removed; `EvidenceUpload { sha256, upload_id, plan }`; MCP guide/description, `docs/.../mcp-tools.md`, generated `llms-full.txt`, `architecture/wyrd-design.md` Verifier evidence section | `pg_openapi_contract` (route absent); V8 `wyrd-cli card_lifecycle::pg_tests::artifact_upload_keeps_shared_completion` and `card_lifecycle_cli_journey`; `codegen:regen` no diff, `codegen:check`, `docs:check` | PASS |

Checks run on the final tree: `fmt`, `lints`, `py:format`, `py:lints`,
`py:typecheck`, `ts:typecheck`, `ts:napi:check`, `codegen:check`, `check:deps`,
`check:tenant-isolation`, `check:storage:drift`, `docs:check`,
`git diff --check`; `cargo nextest` for `wyrd-server` and `wyrd-mcp` `--lib`
(574 passed), `wyrd-storage` and `wyrd-client` `--lib` (320 passed), and the
`pg_verification_routes`, `pg_hosted_files`, and `pg_openapi_contract` modules
(44 passed).

Diagnosis (storage e2e):
- **Symptom:** `storage_e2e::local_client_server_round_trip` and
  `local_upload_capability_rejects_raw_paths_and_bad_bytes` failed at Card
  completion with `WriterUnavailable { table: "bifrost.storage.objects" }`.
- **Evidence:** the outbox logged "Scribe rejected a frame terminally", and the
  in-process Scribe returned `ingest card scope validation failed`.
- **Cause:** since TASK-001, completion records a card-scoped inventory fact.
  The journeys named a fabricated `FIXED_CARD_UID` that the tenant never
  registered.
- **Fix site:** the journey fixture now registers its Card (`register_card`).
  The refusal journey keeps the fixed UID because it is refused before the Card
  is resolved. No production caller completes for an unregistered Card.

Non-goals stayed excluded: no compatibility alias, alternate planner,
provider-specific SDK loop, new completion mode, extra hashing pass, retention
policy, migration, or Card/File physical sharing. File and ArtifactRef
declaration shapes and exact invocation authorization are unchanged. No
unrelated file changed.

Material limits:
- Real-GCS multipart was not run because no GCS credentials are available.
  The emulator lanes prove GCS single-put, S3 multipart, and Azure block blob.
- Deletion dominates: a re-upload of a deleted File digest is not reused, which
  matches Card fold semantics. No path deletes Files today.
