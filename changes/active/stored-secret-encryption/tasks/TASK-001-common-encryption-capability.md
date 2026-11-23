---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-stored-secret-encryption
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005]
depends_on: []
---

# Build the common encryption capability

## Outcome and flow

Provide the shared capability that every secret owner will consume: verified
ownership context and secret bytes → `WyrdKms` → common envelope; expected
context and envelope → the same facade → protected plaintext for intended use.
File, explicit memory, GCP KMS, AWS KMS, and Vault Transit differ only in wrapping
and authentication. This task supplies the capability and common configuration;
Task 2 connects production resource owners and performs the runtime cutover.

Use the [approved spec](../spec.md) and finalized design sections
[One Wyrd KMS facade](../design/notes.md#one-wyrd-kms-facade) and
[What rotation versions mean and what Wyrd loads](../design/notes.md#what-rotation-versions-mean-and-what-wyrd-loads).
Do not create another secret registry or a public encryption/decryption API.

## Existing owner and scope

This task implements stored-secret encryption, not new authentication flows.
Reuse existing provider credential acquisition and refresh. Provider API
support does not require adding a vendor SDK or federation support; a new
authentication capability needs separately agreed scope. Auth-owned secrets
remain in scope as encryption consumers.

`crates/shared/wyrd-crypt/src/lib.rs` already provides AES-GCM/AAD, fresh-DEK
envelope helpers, canonical length-prefixed context, and zeroizing key types.
Its envelope helpers currently require a raw local KEK and cannot call remote
wrapping providers. Extend/reuse those primitives, with one server-tier facade
for wrapping clients and envelope lifecycle. Auth and Gateway must consume it
without depending on their consuming `wyrd-server`; cloud dependencies do not
belong in pure crypto, `wyrd-spec`, or SDKs. Private crate placement and provider
client choice remain implementation-owned under those boundaries.

The facade owns seal/open, fresh-DEK renewal, and routine rewrap. Backends supply
authenticated wrap/unwrap, configured-key resolution, and reusable authenticated
clients. Define one typed configuration and canonical projections, including
backend selection, active/retained logical references, provider authentication,
validation, and redaction. Task 2 replaces old server configuration with it.
Do not give tenant writers key-administration authority or generate fallback keys.

Before replacement behavior is implemented, make the spec-authorized authority
updates in `architecture/wyrd-security-posture.md` (stored credentials and tenant
isolation), `architecture/wyrd-design.md` (Operator connections), and
`architecture/operations/{deployment-and-release,reliability-and-recovery}.md`
(configuration and key retention/restore). Preserve unrelated storage/erasure
rules. These updates describe the approved contract; later tasks implement it.

## Ordered acceptance scenarios and evidence

Add actual assertion locations, focused commands, RED/GREEN results, and limits
to these entries during implementation. No new test names are prescribed.

1. **Seal and open through the facade** — REQ-001–002, REQ-004.
   Given two writes of the same bytes and trusted owner context, invoke the
   common file/memory-backed facade. Both open correctly, have independent DEKs
   and fresh payload nonces, and use the common envelope. Unit tests in the
   shared capability/crypto owner exercise its actual API; a caller-local cipher
   cannot satisfy them. RED is the missing facade/common-envelope behavior.
   - **Evidence.** Owner: new server-tier crate `crates/wyrd/wyrd-kms`
     (`WyrdKms` in `src/lib.rs`, envelope in `src/envelope.rs`, local keys in
     `src/local.rs`); `wyrd-crypt` gains `SecretKey::generate` and a public
     `canonical_aad`. Reuse: `wyrd-crypt`'s AES-GCM/AAD primitives are reused;
     its `seal`/`open` envelope needs a raw local KEK, so the remote-wrap
     facade is new. Assertions: `tests::seal_uses_a_fresh_dek_and_nonce_per_value_in_the_common_envelope`
     (`src/lib.rs:550`), `tests::file_keys_open_envelopes_after_restart` (`:587`).
     Focused command per test: `mise exec -- cargo nextest run --locked -p wyrd-kms --lib -E 'test(=<module>::<name>)'`.
     RED: both panicked `not yet implemented` in the stubbed `WyrdKms::new`.
     GREEN: both pass. **PASS.**
2. **Reject substituted ownership and ciphertext** — REQ-003–004.
   Starting from a valid envelope, change expected tenant/platform scope,
   purpose, owner, applicable provider/client authority or secret version; also
   tamper with payload/wrapped DEK, use a disallowed key reference, unknown
   format, or invalid unwrapped DEK length. Opening refuses without trying an
   unrelated source. Unit and adapter-boundary tests observe refusal and absence
   of unintended backend requests. RED is missing shared validation/binding.
   - **Evidence.** `tests::substituted_context_or_tampered_envelope_refuses`
     (`src/lib.rs:611`): tenant, platform scope, purpose, owner, authority
     (changed and extended), and version substitutions, plus a flipped byte
     in every envelope region → `Authentication`.
     `tests::disallowed_key_unknown_format_and_bad_dek_refuse` (`:676`):
     disallowed ref → `KeyNotAllowed`, unknown format, wrong-length DEK →
     `InvalidDek`. No backend request for a disallowed key:
     `remote_tests::provider_failures_refuse_redacted_and_disallowed_keys_send_nothing`
     (`src/remote_tests.rs:341`) asserts zero fixture requests.
     GREEN: pass. **TDD VIOLATED:** the binding shipped with scenario 1's
     seal/open, so no RED was observed; mutation check (dropping the version
     from the canonical context) made the substitution test fail, then the
     file was restored. **PASS (behavior).**
3. **Use the same envelope with each remote backend** — REQ-001, REQ-004–005.
   Given configured provider identity and allowed active/retained keys, wrap and
   unwrap facade-generated DEKs through GCP, AWS, and Transit adapters and open
   the payload through the facade. Integration tests exercise actual adapter
   HTTP/auth boundaries with local protocol fixtures and real dev Transit where
   applicable. Assert configured resource selection, exact context, opaque
   ciphertext round-trip, access-credential refresh, and redacted failures on
   timeout/denial. Native version changes remain provider-owned. RED is absent
   adapters/common provider behavior; local fixtures are not live-cloud proof.
   - **Evidence.** Adapters `src/gcp.rs` (Cloud KMS REST, bearer from
     `object_store`'s GCP credential chain), `src/aws.rs` (KMS JSON 1.1,
     SigV4 via `object_store::aws::AwsAuthorizer`), `src/transit.rs`
     (`wyrd_vault::VaultTransit`, new in `crates/shared/wyrd-vault`, sharing
     the screened bounded client with `VaultKv2`), common HTTP in
     `src/remote.rs`. Local protocol fixtures (`wiremock`) that seal the
     received context and refuse a mismatched decrypt:
     `remote_tests::gcp_wraps_through_the_configured_crypto_key_with_exact_context`
     (`:222`), `remote_tests::aws_wraps_through_the_configured_key_arn_with_exact_context`
     (`:286`), `remote_tests::transit_wraps_through_the_configured_key_and_rereads_its_token`
     (`:416`), and the failure test above (`:341`: 403 → `refused`, 503 →
     `transport`, delayed answer → `timeout`, failing chain → `credential`;
     error text holds no URL, key, token, or secret). Assert configured /
     retained resource selection, exact canonical context, opaque ciphertext
     round-trip, a new credential per call (refresh delegated to the
     provider chain), and redaction. Real service: `tests/vault_transit.rs`
     `transit_wraps_rotates_and_refuses_through_the_facade` against
     `vault server -dev` 2.1.1 (aes256-gcm96; substituted context refused;
     old envelope opens after native `rotate`; rewrap; bad token refused),
     lane `mise run test:kms:vault` (selected by CI for `wyrd-kms` and
     `wyrd-vault`; `gate:rust:journeys`). `wyrd_vault::tests::transit_wraps_and_unwraps_through_the_named_key`
     (`crates/shared/wyrd-vault/src/lib.rs:742`) pins the Transit wire shape.
     Regression: `test:gateway:vault:inner` (ExternalSecret over the
     refactored KV client) passes. GCP and AWS were proven only against
     local protocol fixtures, not live cloud. **TDD VIOLATED:** adapter and
     `VaultTransit` code compiled before their tests; no RED was observed.
     Mutation check: sending an empty AWS decrypt context failed the AWS
     test. Transit answers a substituted context as `refused`, not
     `Authentication`, because it does not distinguish it from denial.
     **PASS (behavior).**
4. **Prepare renewal and rewrap for consumers** — REQ-002, REQ-004.
   Given an old readable envelope and permitted replacement wrapping key, renew
   encryption and separately rewrap through the facade. Renewal preserves value
   with a fresh DEK/payload; rewrap preserves DEK/payload and opens under the new
   reference. Provider failure returns no successful replacement envelope.
   Facade unit/integration assertions drive the primitives consumed by Tasks 3–4.
   RED is missing shared renewal/rewrap, not a manufactured resource operation.
   - **Evidence.** `WyrdKms::renew`, `WyrdKms::rewrap`, `WyrdKms::is_current`.
     `tests::renew_uses_a_fresh_dek_and_rewrap_keeps_it_under_the_active_key`
     (`src/lib.rs:821`): renewal gives a new nonce and DEK under the active
     key and opens with the old key gone; rewrap keeps nonce, ciphertext, and
     DEK, names the active key, and opens with the old key gone; rewrap
     still authenticates the owner context.
     `remote_tests::renew_and_rewrap_return_nothing_when_the_provider_fails`
     (`src/remote_tests.rs:514`): 503 → error and no envelope; the old one
     still opens and rewraps through the active CryptoKey after recovery.
     RED: both panicked `not yet implemented` at the stubs. GREEN: pass.
     **PASS.**
5. **Validate backend setup without fallback** — REQ-005.
   Exercise typed configuration with valid selections and malformed/missing or
   incoherent settings. Assert redacted diagnostics and refusal rather than
   substitute file/memory keys. Explicit memory mode loses old readability when
   its key is lost; file access retains only configured active/retained material.
   Configuration/file integration tests cover the actual facade constructor.
   RED is missing common configuration validation; preserve existing safeguards.
   - **Evidence.** `KmsConfig` / `KmsBackendKind` in `src/config.rs` match
     `architecture/operations/deployment-and-release.md#stored-secret-encryption`.
     `config::tests::valid_selections_resolve` (`:451`),
     `config::tests::malformed_missing_or_incoherent_settings_refuse` (`:484`),
     `config::tests::debug_and_diagnostics_are_redacted` (`:639`),
     `config::tests::environment_projects_onto_settings` (`:665`); facade
     constructor: `tests::file_constructor_refuses_bad_keys_and_holds_only_configured_ones`
     (`src/lib.rs:743`, missing or loose key file refuses; only configured
     keys are held); memory loses readability in
     `tests::disallowed_key_unknown_format_and_bad_dek_refuse`. RED: the
     refusal test failed `configuration was accepted` and the redaction test
     panicked before validation existed. GREEN: pass. No fallback key is
     generated on any path. **PASS.**

## Verification and handoff

Use the owning library's unit runner: `mise exec -- cargo nextest run --locked
-p <actual-owning-crate> --lib`, recording each added test's exact focused
selector before RED. Existing primitive owner: `wyrd-crypt`. Any new server-tier
owner uses the same runner after its actual package is registered; do not invent
an existing lane. Reuse installed `wiremock`/HTTP infrastructure and the dev Vault
tool/lifecycle pattern in `mise.toml` rather than requiring production secrets.
Disclose which provider checks used local protocols versus a real service.

Run affected owner tests, `mise run check:deps`, `mise run fmt`, and
`mise run lints`; run `mise run codegen:check` if error/contract outputs change.
No runtime consumer or hot path is switched in this task. Explain the common
configuration and provider setup in its owning reference documentation; Task 2
connects the deployment guides to the actual server setup.

**Verification record.** `mise exec -- cargo nextest run --locked -p wyrd-kms
-p wyrd-vault -p wyrd-crypt --lib` (25 passed); `mise run test:kms:vault` (1
passed); `test:gateway:vault:inner` under `scripts/postgres/with-test-postgres.sh`
(pass); `mise run check:ci-selection`, `mise run check:deps`, `mise run fmt`,
`mise run lints`, `cargo hakari generate --diff` / `manage-deps --dry-run`,
`git diff --check`, and `mise run docs:check` pass. `codegen:check` not run:
no `WyrdError`, schema, or wire contract changed.

**Owner lane `test:wyrd`.** The first run failed 4 of 2849, all in
`wyrd-server` and all outside this task's code paths. Each was diagnosed
and fixed at the test seam; production code is unchanged:

- `scribe_outbox::tests::cancelled_encoding_retains_delivery_capacity`.
  Symptom: `attempted to use a condition variable with two mutexes`, then
  `PoisonError` (`scribe_outbox.rs:1185`). Cause: the `#[cfg(test)]`
  `EncodingHold` waited on one `Condvar` with two mutexes, which macOS
  pthreads rejects. Fix: one mutex guards both counters (`9482bad48`).
- `pg_durable_verifier_work::directory_snapshot_is_complete_or_rejected`.
  Symptom: `directory: File exists` (`:746`). Cause: on a case-insensitive
  filesystem, `kinds/A` names the file `kinds/a`. The test already adds
  collision cases only when both entries exist. Fix: tolerate
  `AlreadyExists` there (`a750f6931`). Process deviation: fixed before a
  diagnostician was consulted.
- `components::gateway::pg_invocation_tests::captured_speech_stays_charged_until_its_capture_settles`.
  Symptom: `the FIFO writes: Broken pipe` (`:4703`). Cause, confirmed by a
  read-only diagnostician: `object_store` 0.14.2 sizes a local read by
  fstat at open (0 for a FIFO). The capture parks only in `open()`; once a
  writer opens, the read returns empty and closes, and the capture fails
  closed. Fix: assert the charge before releasing `open()`, then release it
  without writing (`6e954aec5`), matching the sibling deadline test.
- `pg_deferred_evidence::a_parked_execution_releases_compute_before_its_park_is_acknowledged`.
  Symptom: charged worker `{pid}` and idle `{}` (`:640`). Cause, confirmed
  by a read-only diagnostician: the stalled write is the step-begin
  revision (`state.rs:1034-1046`), staged while Python still runs, not the
  park. Fix: the sibling test's bounded wait for released compute; the park
  still queues behind the stalled revision (`0ea1dc8f8`).

Re-run: `mise run test:wyrd` passes, 2849 of 2849; `mise run test:shared`
passes, 898 of 898.
 Reference:
`docs/src/content/docs/reference/stored-secret-encryption.md`, `draft: true`
until Task 2 makes the server read `[kms]`. Authority updated before
replacement behavior (commit `a859dab1d`). No runtime consumer is
switched and no SQL or migration changed. Correction (TASK-001-R1): the
shared Vault read the Gateway ExternalSecret path awaits
(`VaultKv2::read_field` → `Endpoint::send`) did change, so a Gateway hot
path is touched; its wait inventory and benchmark qualification are recorded
in the TASK-001-R1 remediation.

Output: one tested shared capability/configuration and updated authority for
Task 2, with no requirement for SQL, migrations, cloud key provisioning per
tenant, private SaaS code, or legacy readers.
