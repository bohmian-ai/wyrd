# Storage-domain review — TASK-003-R2

Subject: cumulative `861f8d86cc3f9d7e70fb59489e80f8be62afddbf..1b2db8634bf83c03ba210ebf55700983dd9091e6` (candidate tree `52b3fe3243df7f74ba793eaa5be6caa84fba78ad`). I independently inspected the full-range storage/CI/docs source and the R2 changes, against approved specification revision 9, the original TASK-002/TASK-004/TASK-003, the prior R1 verdict, and TASK-003-R2. This report covers storage test reachability, Azure dispatch, the retained URL configuration, and local/cloud workflow proof; other domains are outside this report.

## Authority and source coverage

| Boundary | Authority | Source and verification examined | Result |
|---|---|---|---|
| Test selection and retained assertions (`FIND-2`) | `AGENTS.md` §§11–12, `architecture/agent-rules.md` test-integrity rules, `architecture/references/languages/testing-workflows.md`, R2 finding and acceptance criterion | `mise.toml:623–740`, `wyrd-storage/Cargo.toml:65–76`, `tests/handle_crud.rs:119–164`, `tests/integration_azurite.rs:79–97`, original `backend_contracts.rs`, `BackendSigner::abort_multipart` → `CloudSigner::abort_multipart` → `AzureSigner::abort_multipart`; recorded exact tests and emulator matrix | PASS |
| Storage configuration and cloud proof (`FIND-7`) | Approved spec REQ-034, REQ-064, AC-008, AC-022; R2 owner decision; `AGENTS.md` §§11, 15; `architecture/wyrd-design.md` client/server boundary | `settings.rs::BackendConfig::from_url`/`from_env`, GCS/Azure factories, `mise.toml:661–700`, ignored `mise.local.toml` task names (URLs not retained in this report), R2 implementation evidence at commit `117f668f6`/tree `e97a1e0a` | PASS, subject to evidence limit below |
| GitHub Actions and Wyrd docs alignment | Approved spec REQ-033/034, AC-008; R2 storage follow-up; `architecture/references/languages/implementation-execution.md` | All `.github/workflows` and `.github/actions` storage-variable references, `.github/workflows/storage-integration-{cloud,emulator}.yml`, `docs/src/content/docs/self-hosting/{storage,configuration,index,local-development}.svx`, `mise.toml` storage task env and selectors | PASS |
| Storage journey and cleanup | `AGENTS.md` §11 user-journey priority; `architecture/bifrost-design.md` server/storage ownership | `wyrd-server/tests/storage_e2e.rs` configured backend assertion, multipart journeys, real `WyrdTestServer` + `WyrdStorageClient`, object cleanup; `mise.toml:644–700` and recorded cloud result | PASS |

## Boundary assessment

- `handle_crud` requires `emulator` in `Cargo.toml`, so an ordinary workspace test cannot run its local case. `test:storage:matrix` now depends on `test:storage:handle:emulators`; that owner runs the exact `local_handle_crud` selector after its three cloud-emulator CRUD selectors (`mise.toml:708–740`). The recorded focused run selected and passed one local test. This closes the unreachable-test path without a new test target or feature.
- The deleted `backend_contracts.rs` uniquely checked that Azure abort dispatched through `BackendSigner` and did not silently succeed. The retained Azurite test now constructs `BackendSigner::Cloud(CloudSigner::Azure(...))`, calls `abort_multipart` on a missing blob, requires an error, and rejects `BackendCapabilityMismatch` (`integration_azurite.rs:79–97`). The production match arms in `signer.rs` and `cloud.rs` reach `AzureSigner::abort_multipart`; the `test:storage:azurite` owner selects the test binary. The recorded exact test passed. No orphan target remains in the manifest or tracked tree.
- The chosen `WYRD_STORAGE_URL`/`WYRD_STORAGE_ENDPOINT_URL` contract remains in `settings.rs`; backend tests assert the selected kind rather than silently accepting a wrong backend. Emulator mise tasks set URL, endpoint, and necessary emulator credentials; cloud mise tasks select one handle CRUD and one server multipart journey each, while the developer-local tasks provide the real cloud URLs. The evidence table records both selected tests passing for each provider on `117f668f6`, the same source tree reviewed at `1b2db8634` (the latter commit changed only the evidence packet).
- `.github/workflows/storage-integration-cloud.yml:18–20` has only `push` on `main`, no schedule or manual dispatch. Its three OIDC jobs supply S3/GCS/Azure URLs and call the matching cloud mise tasks; the emulator workflow remains PR/main path-filtered. Wyrd self-hosting storage docs describe the URL/endpoint, provider credentials, local pre-merge cloud tasks, and post-merge `main` workflow. A scan of live workflows and self-hosting docs found no retired storage backend/bucket/container/local-root selection variables.

## Verification limits and findings

The review did not rerun credentialed cloud tests or the full gate: the available evidence is the committed R2 result table, which records each cloud task's two passing tests, the emulator matrix's nonzero selections, and the immutable commit/tree. The `1b2db8634` evidence-only commit has the same implementation source as `117f668f6`. The future post-merge GitHub Actions executions cannot be observed before merge and are not an acceptance gate under revision 9. No implementation file in this domain changed during this review.

Material proposed findings: none.

Overall result for the assigned storage domain: **PASS**.
