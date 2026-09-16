# Storage and cloud-CI domain review

**Result: PASS.** No material finding in this boundary.

## Subject and boundary

- Cumulative base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`; immutable candidate: `24b8e78d7420321eb8fe936b3045fcbd3bc1d26b`, tree `6a3ec967ab094a42256b44ce908abb080360e466`.
- Tested source: `d6de890d83f269eab834fa323f3ebe31f1adc573`, tree `d8a727b5846f24bac7b55bbc786bcb391c55cb03`. R4 changes only `oracle/forwarding.rs` and `state.rs`; its evidence-only successor changes the R4 packet, not storage or workflows.
- Reviewed cumulative storage configuration, signer/operator selection, storage test ownership, `.github/workflows/storage-integration-cloud.yml`, `.github/workflows/nightly.yml`, `mise.toml`, and self-hosting storage/configuration documentation against approved spec revision 9, TASK-003, and R1–R4 remediations. Authorities: `AGENTS.md` §§2–3, 9, 11–12, 14; `architecture/agent-rules.md` storage/security, journey, and gate rules; `architecture/wyrd-design.md`; `architecture/bifrost-design.md` storage/tenant boundaries; `architecture/references/languages/spec-driven-development.md` and `testing-workflows.md`.

## Source and obligation coverage

| Obligation | Source evidence | Result |
|---|---|---|
| One selected backend, no retired storage variables or silent fallback | `crates/wyrd/wyrd-storage/src/settings.rs::from_env` requires `WYRD_STORAGE_URL`; `BackendConfig::from_url` validates scheme, bucket/account/container, local root, credential-free URL and optional endpoint; source scan of `.github/workflows` and `.github/actions` finds only the new URL variable | PASS |
| Signer and data-plane operator use the same backend/endpoint | `StorageHandle::from_settings` builds both from the selected `BackendConfig`; `factory/{s3,gcs,azure}.rs` uses its matching config; production GCS/Azure signer refuses an endpoint override without `emulator`, avoiding a real-provider signer paired with an emulator operator | PASS |
| Cloud workflow is post-merge, three-provider, credentialed and aligned | `.github/workflows/storage-integration-cloud.yml` has only `push` on `main`, no weekly cron or manual dispatch, retains S3/GCS/Azure OIDC jobs and maps each provider's secret location to `WYRD_STORAGE_URL`; each invokes its corresponding `mise` cloud task | PASS |
| Local cloud proof is sufficient before merge | Revision-9 spec lines 887–891 and `testing-workflows.md` explicitly accept existing ignored `mise.local.toml` S3/GCS/Azure tasks; R4 packet records two passing tests per provider on tested source tree. Post-merge Actions results are not required now | PASS |
| Emulator, cloud and documentation agree | `mise.toml` storage lanes supply the URL, emulator endpoint and required feature; cloud lanes select handle CRUD and server multipart journey; `docs/src/content/docs/self-hosting/{storage,configuration}.svx` describes URL, credentials, emulator endpoint, local commands, and merge-to-main Actions | PASS |
| R4 did not disturb this boundary | `git diff b000af7ab..d6de890d8 --name-only` shows only two Oracle/server Rust files; cumulative `git diff --check` is clean | PASS |

## Verification limits

The candidate records same-tree `test:storage:matrix`, local `storage:{s3,gcs,azure}:dev` (two tests each), `codegen:check`, docs and CI checks, and a passing broad gate; I inspected source and recorded selections but did not rerun credentialed jobs or the broad aggregate. No post-merge Actions run can exist before merge, and approved revision 9 does not require it. The user also accepts same-tree gate child execution, so a single aggregate command is not a separate requirement for this review; this candidate records a completed aggregate in any event. Azure SAS expiry during later blocks and LocalFS buffering/route limits remain expressly excluded remediations, not findings here.

## Material findings

None.
