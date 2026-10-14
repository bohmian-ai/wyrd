# Maintainer review

## Subject and authority

- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Candidate: `202e5bab768cb998a01f04555c39f43b53c09929`.
- Specification: `changes/active/sealing-keyring/spec.md`, revision 1.
- There is no implementation task, by explicit human direction. Nothing has shipped, so compatibility with deployed Gateway envelopes is outside this review.
- Governing maintainer authority: `AGENTS.md`, especially §§3–6, 11, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/references/languages/spec-driven-development.md`; and the complete `architecture/references/languages/maintainer-style.md`.
- Reviewed the complete cumulative diff and expanded its owners, callers, and tests from source using CodeGraph first. No other reviewer's conclusions were read.

## Changed-surface coverage

| Changed surface | Symbols, callers, and evidence inspected | Maintainer assessment |
| --- | --- | --- |
| Gateway binding and encryption | `managed.rs`: `ManagedSecretBinding::{seal, open}`, `SealedPayload`, serialization/redaction helpers, and all changed tests; `credential.rs`: `CredentialResolver::{new, resolve}`, test resolver construction; `snapshot.rs`: `GatewayCredentialSource::ManagedSecret`; `lib.rs` exports | The binding owns real identity invariants. Encryption and envelope parsing remain in `wyrd_crypt::SealingKeyring`. Input/output types clearly distinguish plaintext and opaque bytes. No replacement crypto owner or speculative trait is introduced. One changed test helper lacks required error rustdoc, below. |
| Server administration and admission | `GatewayAdministration::{new, put_credential, seal, credential_snapshot}`, `StoredCredential`, `sealed`; SQL credential write and snapshot consumers; `pg_administration_tests.rs` changed helpers and managed-secret test | Plaintext protection stays at the server-owned write boundary. Snapshot decoding remains beside its consumer. Missing/invalid encoded envelopes become unusable bytes and fail at the credential resolver's existing trust boundary. The typed row and snapshot changes agree. |
| Server configuration and state | `GatewayConfig::{validate, assignment, credential_resolver}`, removal of the per-tenant key config/loader; `AppState` constructor and removed keyring field/builder; `boot::attach_config_fields`, its caller in boot, `rewrap_sealed_secrets` | Removes the second key authority and shares the existing `state.auth.sealing_key`. Resolver and administration receive the same retained handle. Config remains the owner of Vault backend composition. `attach_config_fields` lacks required error documentation, below. |
| Rewrap and persistent representation | `SealedSecretTable::{ALL, label, select_sql, swap_sql}`, `SealedSecretRow`, `sealed_tenant_secrets`, `swap_sealed_tenant_secret`, complete `SealedSecretRewrap::{run, settle}`; Gateway SQL `GatewayCredentialWrite`, upsert and snapshot SQL; migration | Extends the existing closed rewrap enumeration and its existing operator compare-and-swap workflow. No new rotation worker, query framework, or durable lifecycle is added. Existing SQL names for general sealed rows are somewhat auth-oriented but do not justify a refactor. |
| CLI key lifecycle | `DevArgs::run`, `DevServer::{new, command}`, `local_sealing_key`, `SEALING_KEY_FILE`, `ServerInstaller::root`, existing installation/root creation and activation behavior | Key provisioning remains a bounded invocation-owned helper; the retained path belongs on the existing `DevServer`. Existing `tempfile` persistence supplies no-clobber creation. A new owner or shared persistence abstraction would add machinery without serving the request. |
| CLI journey and fixtures | Changed `LocalDev::{command, stop}`, new `stdin_command`, `LOCAL_PROVIDER_KEY`, complete new restart journey and its `mise.toml` wiring | Reuses the compiled-CLI harness and real local-dev lifecycle. Assertions check upstream delivery, restart persistence, and captured-output leakage. Returning captured text from `stop` avoids a second teardown mechanism. |
| Server integration tests | Changed `sealing_keyring`, `stored_credential`, `managed_replica`; new `managed_envelope`, `replace_managed_envelope`, `assert_refused`; full changed managed-secret invocation test and surrounding dispatch/drain callers; remaining mechanical config/test-call updates | Helpers have concrete scenario purposes. The test names and assertions expose the tenant-copy, retained-key, retired-key, rewrap, keyless, and revocation cases. No optional test framework or broad fixture abstraction is added. |
| Shared server harness | Removed managed-key fixture generation/constants/temporary directory; changed `WyrdTestServerBuilder` state/resolver assembly and `test_gateway_config`; existing sealing-keyring selection and boot rewrap calls | Uses the harness's pre-existing deployment sealing keyring. Removes duplicate test-only key authority and lifecycle guards. Fixture behavior and production boot use the same sealing owner. |
| Contracts and generated schema | `ProviderCredentialWriteSource` rustdoc and corresponding JSON schema description | Public wire shape remains unchanged; the source and generated description agree on deployment sealing and binding. No Python/TypeScript declaration change is needed for this documentation-only schema change. |
| Dependencies and docs | CLI manifest, lockfile, security-posture change, quickstart/setup/deployment/troubleshooting docs, spec, and modified `mise.toml` expression | Uses already-installed workspace dependencies. User docs identify the persistent key, explicit override, and loss/recovery consequence. The integration expression includes the new exact test name. No legacy compatibility requirement is inferred. |

## Existing-owner comparison

The relevant alternatives are already present and the implementation extends them:

- `SealingKeyring::{seal, open, rewrap}` already owns the envelope, write key, retained keys, and re-encryption. The candidate removes Gateway's independent `ManagedSecretKeys`/`TenantKeyring` workflow and leaves only the necessary tenant/name/provider binding at the Gateway boundary.
- `SealedSecretRewrap` already walks operator-visible tables and swaps exact old ciphertext bytes. Adding the Gateway enum variant is sufficient; an independent Gateway sweep would duplicate this owner.
- `ServerInstaller` already owns the release-independent installation root and creates private installation directories. `tempfile::NamedTempFile::persist_noclobber` already supplies the required non-overwriting commit. The small CLI helper adds only key creation and child environment wiring.
- The existing `CredentialWriter` is an authenticated provider-credential HTTP mutation handle, not a deployment key-file writer. It is not an appropriate alternative to local key provisioning. Existing saved-login/config owners write their own structured client state and are not a reason to route raw deployment key files through that workflow.
- The existing `LocalDev` harness already owns installed-binary setup, foreground children, readiness, and termination. The new journey extends it instead of reproducing that lifecycle.

## Material finding

### MAINT-001 — Changed fallible items omit mandatory error rustdoc

- Classification: `VIOLATION`.
- Locations: `crates/wyrd/wyrd-server/src/boot/mod.rs:1326` (`attach_config_fields`) and `crates/wyrd/wyrd-gateway/src/managed.rs:163` (`tests::open`).
- Governing rule: `AGENTS.md` §16 requires every new or materially modified Rust item, including private/test helpers, to have substantive rustdoc and every fallible function to include a `# Errors` section naming its error conditions. The task-review skill explicitly preserves missing substantive rustdoc as a blocking finding.
- Evidence: `attach_config_fields` materially changes Gateway keyring assembly but its rustdoc ends with “Pure/sync (no I/O).” and has no `# Errors`. Its full body propagates the operator-key construction error and maps resolver/provider transport construction failures to `ServerBootError::GatewayTransport`. The resolver can read configured CA files through `GatewayConfig::credential_resolver`, so the no-I/O description also misstates the current composition boundary. `tests::open` materially changes its signature and delegation to `ManagedSecretBinding::open`; its two-line description explains exposing plaintext for assertions but never documents the propagated unavailable/decode/binding failure.
- Concrete maintenance cost: a maintainer cannot learn from the changed boot helper's contract which configuration or transport failures abort composition, and is incorrectly told it does not perform IO. The test helper likewise omits the negative-result contract that its callers assert. This is a bounded documentation gate violation, not a request for another abstraction or runtime change.
- Smallest correction: add accurate `# Errors` documentation to these two touched functions; describe operator-key and Gateway resolver/transport construction refusal for the boot helper and propagated `CredentialError::Unavailable` for the test helper. Correct the boot helper's no-I/O claim to acknowledge existing synchronous backend-CA loading. Preserve signatures and behavior. Nearby `GatewayConfig::credential_resolver` and `ManagedSecretBinding::open` already document the respective lower-level failure conditions and should be the source of truth.
- Focused closure proof: inspect the two final rustdoc blocks against their full bodies and the named callees; run the narrow relevant formatting check. No new runtime test or broad journey suite is warranted for these prose-only corrections.

## Non-blocking calibration

No additional style preferences are proposed as changes. In particular, the borrowed binding type, invocation-scoped file helper, closed rewrap enumeration, and existing test helper shape are appropriate; fewer or differently named private methods would not establish an observable improvement.

## Verification limits and result

This is a static maintainer review. No tests, codegen, formatting, lint, or integration lanes were executed in this role, and no prior execution results were supplied. Inspected source tests are not represented as passing executions. Source was not modified.

**FAIL** — `MAINT-001` is a bounded, explicit rustdoc gate violation. No additional maintainer finding was identified in the cumulative changed surfaces.
