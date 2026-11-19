# R2 staged-tree execution evidence

Candidate tree `abd896728870d606f3f79bf18b7be8b747208982` against HEAD
`52f69ed2da3846733be0d314c6627c95d308e8dd`. The user explicitly supplied staged
changes for review. No commit was created and no index entries were changed.
Tracked working source matches the frozen index; supplied untracked task/evidence
hashes are recorded in subject.md and evidence-identities.json. All Cargo-backed
commands run sequentially. Only this new review directory is written.

## Exact focused tests

```bash
mise exec -- cargo nextest run --locked -p wyrd-utils -p wyrd-kms --lib -E 'test(=secret::tests::google_service_account_json_prefers_the_decoded_base64_variable) | test(=secret::tests::google_service_account_json_does_not_fall_back_past_a_malformed_base64_variable) | test(=remote_tests::gcp_kms_signs_with_either_inline_google_credential) | test(=remote_tests::gcp_kms_keeps_google_credential_precedence) | test(=remote_tests::gcp_kms_redacts_an_unusable_inline_google_credential)'
mise exec -- cargo nextest run --locked -p wyrd-storage --features emulator --lib -E 'test(=factory::tests::gcs_signer_selects_the_configured_google_identity) | test(=factory::tests::gcs_signer_redacts_an_unusable_inline_google_credential)'
```

| Proof | Result | Nextest run |
| --- | --- | --- |
| Exact shared resolver and production KMS constructor/facade selectors | PASS 5/5; 29 unrelated tests excluded | `846fee67-6b77-46f2-9d01-16f7f6178032` |
| Exact production storage signer selectors | PASS 2/2; 57 unrelated tests excluded | `a1a56e0c-8399-46a3-8a0c-e57eaa3cf9a5` |

`git diff --check 52f69ed2d abd896728870d606f3f79bf18b7be8b747208982`: PASS.
`mise run check:deps`: PASS (including its shipped-source boundary check).
`mise run check:workspace-hack`: PASS; no generated manifest changes required.

## Limits

No fresh full owner suite, format, lint or docs aggregate is claimed. Their
implementation results are in the hashed R2 task evidence. GREEN reruns do not
recreate RED history. Storage's no-inline credential-file path is assessed using
unchanged production source and the KMS file-input proof; the local storage
emulator intentionally omits authentication without inline credentials.
No live Cloud KMS account or Linux startup image was run; accepted CI limits remain.
No benchmark was requested or added. No implementation/staging/commit/push occurred.
