---
id: TASK-007
kind: implementation
status: proposed
spec: SPEC-bifrost-variant-main-integration
spec_revision: 5
requirements: [REQ-026, REQ-027]
primary_requirements: [REQ-026]
acceptance: [AC-026, AC-027]
depends_on: []
blocks: [TASK-002]
---

# Consolidate Wyrd storage on one object store

## Outcome and Value

`wyrd-storage` and Bifrost share one storage client, `object_store` 0.14.2.
`wyrd-storage` builds one backend store, and `StorageHandle` (data plane, signer,
multipart control) and `BifrostStorage` both use it. OpenDAL, the AWS SDK, the
gcloud crates and the `azure_*` crates leave the workspace. GCS large uploads
move from the resumable protocol to GCS XML multipart (`S3MultipartV1`). One GCS
emulator (`tustvold/fake-gcs-server`) then serves every lane, which closes
gate Cause D in TASK-002. Governs spec revision 5. TASK-002's gate closeout
waits on this task.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Backend store construction | `vala-bifrost-redux` `storage::cloud::{s3,gcs,azure}` (object_store builders) | `BifrostStorage::backend_store`; Bifrost journeys | `wyrd-storage` builds a second client (OpenDAL `factory::build_operator`, SDK `factory::*::build_signer`) | Move the builders to `wyrd_storage::factory`, which takes client and retry options; Bifrost passes its policy | None — the code moves to one owner |
| Data plane get/put/list/delete/len | `StorageHandle` over `opendal::Operator` | service.rs, fitter.rs, eval.rs, storage_e2e, pg_verification_runtime, handle_crud | — | `StorageHandle` holds `Arc<dyn ObjectStore>`; callers use handle methods, not `operator()` | None |
| Single PUT / GET presign | `S3Signer`/`GcsSigner`/`AzureSigner` over three SDKs | service.rs plan + download paths | — | `Signer::signed_url` on the shared store (Azure: SAS from account key or a user-delegation key) | None |
| Multipart control | S3 SDK create/complete/abort; GCS resumable session; Azure SDK block list/delete | service.rs, sweeper.rs | GCS has no abort and no server completion | `MultipartStore::{create_multipart, complete_multipart, abort_multipart}`; part URLs from `signed_url_opts` with `partNumber`/`uploadId`; Azure completion = `complete_multipart` with the six-digit block ids, abort = `delete` | None |
| Verification HEAD | SDK `head` per backend → `HeadInfo{size, content_type, sse_marker}` | service.rs completion, `require_encryption` | `ObjectStore::head` exposes neither content type nor SSE | HEAD on an object_store-signed URL through the crate's `reqwest`; read the backend's SSE header | One small private function; object_store has no header-returning head |
| S3 lifecycle warning | `cloud::check_s3_lifecycle` (aws SDK) | `preflight::run` | — | Signed `GET ?lifecycle` through object_store's S3 signer | None |
| Telemetry | OpenDAL `StorageTelemetryLayer` | handle data plane | — | The same metrics recorded by the handle's own operations | None — the layer is deleted |
| GCS wire protocol | `WireProtocol::GcsResumableV1`, `UploadPlan::GcsResumable`, `UploadCompleteRequest::GcsResumable`, wyrd-client `gcs_resumable.rs` | schemas, client dispatch, server card service, SQL check, sweeper | — | Delete; GCS emits `S3Multipart` and the client's existing S3 multipart path uploads it | None — deletion |

**Owners:** `crates/wyrd/wyrd-storage`, `crates/wyrd-spec/src/storage`,
`crates/shared/wyrd-client/src/storage`, `crates/vala/vala-bifrost-redux/src/storage`,
`crates/wyrd/wyrd-server` (card service, verification storage reads, storage
tests), `crates/wyrd/wyrd-sql/migrations/20260601000002_storage.sql`, the
storage docker-compose and `mise.toml` emulator tasks.

**Prohibited:**
- No second store, signer, or client per backend.
- No compatibility alias for `GcsResumableV1`.
- No retries added behind storage writes.
- No change to Bifrost's storage policy, the `LimitStore` wrap, or the metadata cache.
- No hand-edited generated schemas or stubs.
- No committed private key.

## Approach

1. Move store construction into `wyrd_storage::factory`. `BifrostStorage` and `StorageHandle` take their store from there.
2. Rebuild the signer over the shared store: presign, multipart control, HEAD, re-mint, and the S3 lifecycle warning. Delete the SDK signers.
3. Delete the GCS resumable contract end to end: spec types, schemas, client uploader, SQL check, service, and sweeper. GCS joins the S3 multipart path.
4. Replace OpenDAL in the handle and its server consumers. Move telemetry onto the handle's own operations.
5. Remove the dependencies, switch the GCS emulator, and give emulator lanes a per-run throwaway signing key.

## Ordered Implementation Scenarios

### Scenario 1 — GCS multipart round trip on the emulator

**Behavior.**
- A GCS upload above the threshold initializes as `S3Multipart` with a server upload id.
- Parts go to presigned part URLs, and completion uses their ETags.
- The download returns the same bytes.
- Abort and the sweeper cancel a GCS upload.

**RED.** `storage_e2e::gcs_multipart_e2e` fails today with
`Transport { operation: "upload" }` against the object_store emulator:
`mise run test:storage:e2e` (GCS leg).

**GREEN.** GCS signer over `MultipartStore` and `signed_url_opts`; GCS resumable removed.

**REFACTOR.** One multipart code path for S3 and GCS in the signer.

### Scenario 2 — Typed backend errors and HEAD metadata

**Behavior.**
- HEAD on a missing object is a typed not-found on every backend.
- HEAD on a present object reports size, content type and the SSE marker.
- Azure abort on a missing blob stays an error.

**RED.** `integration_gcs::gcs_head_on_missing_returns_not_found` fails under
the object_store emulator: `mise run test:storage:gcs-emu`.

**GREEN.** Signed-URL HEAD mapping 404 to the backend's not-found error and 429 to throttled.

**REFACTOR.** None beyond the shared HEAD.

### Scenario 3 — Handle data plane and Bifrost share one store

**Behavior.**
- Handle CRUD passes on local, RustFS, GCS and Azurite.
- Bifrost builds its store through the same factory.
- The media size test still refuses a body its metadata hides.
- Per-operation storage metrics are emitted.

**RED.** The handle and server consumers no longer compile once OpenDAL is removed.

**GREEN.** `StorageHandle` over `Arc<dyn ObjectStore>`; the eval test wraps the store to under-report size.

**REFACTOR.** Delete the telemetry layer, `operator()`, `build_operator` and `from_settings_with_operator`.

## Acceptance Criteria

- `cargo tree` for the workspace shows no `opendal`, `aws-sdk-*`, `aws-config`, `gcloud-*` or `azure_*` crate.
- No `GcsResumable`/`gcs_resumable` symbol, schema or SQL value remains.
- Codegen is clean.
- These lanes pass:
  - `test:storage:rustfs`, `test:storage:gcs-emu`, `test:storage:azurite`;
  - the handle CRUD emulator tests;
  - `test:storage:e2e` (all three multipart legs and the local legs);
  - wyrd-client storage tests;
  - the Bifrost storage unit tests.
- Azure presigning keeps the account-key and user-delegation modes. S3 and Azure behavior is otherwise unchanged.
- GCS abort succeeds. The sweeper aborts expired GCS uploads.

## Verification and Evidence

- `mise run fmt`, `mise run lints`, `mise run check:deps`, `mise run codegen:check`
- `mise run test:storage:rustfs`, `mise run test:storage:gcs-emu`, `mise run test:storage:azurite`, `mise run test:storage:e2e`
- `mise exec -- cargo nextest run --locked -p wyrd-storage --all-features --test integration -E 'test(=handle_crud::local_handle_crud) | test(=handle_crud::s3_handle_crud) | test(=handle_crud::gcs_handle_crud) | test(=handle_crud::azure_handle_crud)'`, with each backend's emulator variables
- `mise exec -- cargo nextest run --locked -p wyrd-client --test integration -E 'test(/^storage_dispatch::/)'`
- `mise exec -- cargo nextest run --locked -p wyrd-storage --all-features --lib`
- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(/^storage::/)'`
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib --features test-support -E 'test(=verification::eval::tests::media_refuses_a_body_past_the_limit_its_metadata_hides)'`

## Material Stop Conditions

- The object_store emulator rejects signed XML multipart part URLs and no object_store-supported emulator accepts them.
- A preserved behavior has no object_store or signed-request equivalent.
- Azure user-delegation signing cannot be produced from token credentials.

## Authority Links

- [Spec revision 5](../spec.md#revision-5--one-object-store-for-all-wyrd-storage)
- [TASK-002](TASK-002-close-the-integration.md)
- [AGENTS.md](../../../../AGENTS.md)

## Evidence
