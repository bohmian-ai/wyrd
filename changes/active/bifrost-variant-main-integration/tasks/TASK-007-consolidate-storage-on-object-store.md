---
id: TASK-007
kind: implementation
status: implemented
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
move from the resumable protocol to GCS XML multipart, and the multipart
protocol is renamed backend-neutral (`MultipartV1`, `UploadPlan::Multipart`,
`MultipartComplete`, `CompletedPart`). One GCS
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
| GCS wire protocol | `WireProtocol::GcsResumableV1`, `UploadPlan::GcsResumable`, `UploadCompleteRequest::GcsResumable`, wyrd-client `gcs_resumable.rs` | schemas, client dispatch, server card service, SQL check, sweeper | — | Delete; rename the S3 multipart contract to `Multipart*`; GCS emits it and the client's one multipart path uploads it | None — deletion |

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
- A GCS upload above the threshold initializes as `Multipart` with a server upload id.
- Parts go to presigned part URLs, and completion uses their ETags.
- The download returns the same bytes.
- Abort and the sweeper cancel a GCS upload.

**RED.** `storage_e2e::gcs_multipart_e2e` fails today with
`Transport { operation: "upload" }` against the object_store emulator:
`mise run test:storage:e2e` (GCS leg).

**Amended (user decision, 2026-10-12).** `fake-gcs-server`, including the
fork object_store's CI uses, does not implement GCS XML multipart
(fsouza/fake-gcs-server#852; object_store skips its multipart tests against
it). GCS multipart is therefore proven only against real GCS in
`test:storage:gcs:cloud`. The emulator keeps GCS single PUT, the verification
HEAD, and handle CRUD through `storage_e2e::gcs_single_put_e2e`.

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

- `cargo tree` for the workspace shows no `gcloud-*` or `azure_*` crate, no `aws-sdk-s3`, and no OpenDAL edge from a Wyrd crate. Amended: `opendal` 0.58, `aws-sdk-glue`, `aws-config` and its SSO/STS crates remain only through S's `vala-bifrost-redux → iceberg-compaction-core → iceberg-catalog-glue`, a non-optional dependency of S's fork, out of scope here.
- No `GcsResumable`/`gcs_resumable` or `S3Multipart*`/`S3CompletedPart`/`s3_multipart_v1` symbol, schema or SQL value remains.
- Codegen is clean.
- These lanes pass:
  - `test:storage:rustfs`, `test:storage:gcs-emu`, `test:storage:azurite`;
  - the handle CRUD emulator tests;
  - `test:storage:e2e` (local legs, GCS single PUT, S3 and Azure multipart);
  - `test:storage:gcs:cloud` against real GCS (GCS multipart journey, abort, re-mint, part signing);
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

Status: `IMPLEMENTED` on 2026-10-12.

**Diagnosis — GCS multipart against real GCS.**
- Symptom: `storage_e2e::gcs_multipart_e2e` returned `Internal "pending S3 upload is missing backend_upload_id"` on the first part URL.
- Cause: `service.rs` persisted the backend upload id only for `StorageBackendKind::S3`, so the GCS XML multipart id was dropped at init.
- Fix site: the one persistence point, now keyed on `WireProtocol::MultipartV1` (`persist_multipart_upload_id`). The id's readers (part signing, completion, abort, sweeper, re-mint) were unchanged.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One store per backend shared by handle, signer, Bifrost | `wyrd-storage/src/cloud.rs` `CloudStore`/`CloudSigner`; `factory::build_signer`; `StorageHandle` over `BackendSigner::objects()`; Bifrost `storage::backend_store` → `CloudStore::{s3,gcs,azure}` | redux `storage::` 2/2; handle CRUD on all emulators 4/4 | PASS |
| No `gcloud-*`/`azure_*`/`aws-sdk-s3`; no Wyrd OpenDAL (amended) | workspace and crate manifests; workspace-hack regenerated | `cargo tree -i opendal@0.58.2` and `-i aws-sdk-glue` show only `iceberg-catalog-glue` via S's `iceberg-compaction-core`; `cargo hakari generate --diff`/`manage-deps --dry-run`/`verify` clean | PASS |
| No GCS resumable / S3-named multipart symbols, schemas, SQL value | wyrd-spec `storage::{protocol,upload}`; `multipart_v1` SQL check; wyrd-client `upload/multipart.rs` | `mise run codegen:check`; wyrd-client `storage_dispatch::` 6/6 | PASS |
| GCS multipart round trip, abort, re-mint, part signing | signer `MultipartStore` path; `persist_multipart_upload_id` | real GCS via `mise run test:storage:gcs:real` (local `mise.local.toml`, credentials from pass) → `test:storage:gcs:cloud`: `integration_gcs::gcs_multipart_*` 3/3, `gcs_handle_crud`, `storage_e2e::gcs_multipart_e2e`, `gcs_single_put_e2e` | PASS |
| Typed HEAD not-found and HEAD metadata | `CloudSigner::head_for_verification` | `test:storage:gcs-emu` 1/1, `test:storage:rustfs` 4/4, `test:storage:azurite` 5/5 | PASS |
| Server journeys on emulators | `storage_e2e` | `mise run test:storage:e2e`: 3 local legs, `s3_multipart_e2e`, `gcs_single_put_e2e`, `azure_multipart_e2e` | PASS |
| Parts numbered 1..n (object_store numbers by position) | `service.rs` `build_complete_payload` | `multipart_complete_rejects_out_of_order_parts` within wyrd-storage lib 56/56 | PASS |
| Metrics on handle operations | `telemetry.rs` `ActiveOperation` | `guard_outcomes_are_exact_and_drain_the_gauge`, `data_plane_calls_emit_exact_operation_metrics` | PASS |
| Media size refusal still holds | eval test wraps the store to under-report size | `verification::eval::tests::media_refuses_a_body_past_the_limit_its_metadata_hides` 1/1 | PASS |
| Azure account-key and user-delegation signing kept | `MicrosoftAzureBuilder` SAS signing | `test:storage:azurite` 5/5 (account key); user delegation is object_store's token path, cloud lane only | PASS |
| Format, lints, boundaries | — | `mise run fmt`, `mise run lints`, `mise run check:deps`, `git diff --check` | PASS |

Limits:
- The OpenDAL idle-connection pg test was removed together with OpenDAL. The idle bound now lives in `client_options` and is covered by `pool_idle_timeout_matches_the_storage_server`.
- No non-goal changed. Bifrost's storage policy, the `LimitStore` wrap, and the metadata cache are untouched, and no retries were added behind storage writes. No private key is committed: emulator keys are generated per run by `scripts/storage/gcs-emulator-key.sh`.
