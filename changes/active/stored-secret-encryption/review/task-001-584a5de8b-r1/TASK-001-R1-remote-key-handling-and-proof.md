---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-stored-secret-encryption
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-008]
depends_on: [TASK-001]
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3]
---

# Complete remote key handling and acceptance proof

## Outcome and reviewed subject

Make the existing remote KMS capability preserve its secret-buffer invariant,
accept the advertised federated GCP workload ADC configuration, and carry the
required performance proof for its shared Gateway Vault consumer. Successful
closure lets the next task consume the common facade without changing its
envelope, authority context or deployment key trust.

Approved contract: [spec revision 1](../../spec.md). Original task:
[TASK-001](../../tasks/TASK-001-common-encryption-capability.md).
Reviewed base `a859dab1d20806e8ad29d9eb064585126419f5c2`, candidate
`584a5de8b97f2d5bd2d258edbdd7ef2f95d8aa82`.
Validated diagnosis and evidence:
[findings-validation.md](findings-validation.md).

## Bounded corrections

**FIND-TASK-001-1 — REQ-008 and security secret-handling authority.** Normal
remote wrap/unwrap creates unprotected application-owned plaintext-DEK
representations in JSON values and serialized bodies. Correct their ownership
at `wyrd-kms` remote adapters/response handling and `wyrd-vault` Transit.
Reuse installed redaction/zeroization facilities; remove the unnecessary AWS
plaintext body duplicate. Preserve wire shapes, authentication context,
bounded transport, refusal classes, secret-free errors and existing KV reads.
Do not broaden this to arbitrary third-party transport/kernel allocations.

**FIND-TASK-001-2 — REQ-005 and deployment ADC authority.** A self-hosted
deployment supplying a Workload Identity Federation ADC file through
`GOOGLE_APPLICATION_CREDENTIALS` cannot construct `GcpKms`, because the current
storage client's ADC enum rejects `external_account`. Correct reusable
credential acquisition behind the existing `GcpKms` owner, including refresh.
Retain configured CryptoKey selection and the current common API. Test the
actual constructor path; the existing injected credentials bypass the gap.
No new public authentication option or live production cloud account is
needed. Narrowing the approved contract requires specification authority and
is not an implementation fix under this task.

**FIND-TASK-001-3 — mandatory Gateway qualification.** The shared Vault KV
extraction executes on Gateway ExternalSecret requests. Supply the full clean
canonical Gateway benchmark and comparison, and correct the original task's
no-hot-path assertion. This is an evidence correction; it requires no
production rewrite. Preserve the user's uncommitted files when obtaining a
clean checkout. Reuse existing benchmark history/readers/report format.

## Ordered closure scenarios

1. **Protected remote key representations** — FIND-TASK-001-1, REQ-008 and
   the existing security invariant. Drive ordinary seal/open and lifecycle
   operations through the existing GCP/AWS/Transit owner boundaries. Observe
   that key-bearing application-owned encoded/decoded/wire representations
   have protected ownership and redacted Debug, with cleanup on normal
   completion, provider error, and cancellation where the owner retains such
   material across awaits. Keep assertions at controllable ownership/drop
   boundaries; never inspect freed allocations or demand guarantees over
   third-party internals. Narrow unit/adapter tests in the owning modules,
   then existing protocol round trips, are the credible tier. Record exact
   focused selectors after adding the tests, before RED.

   **Evidence (commit `17d0e3e6b`). PASS.** Owner: `wyrd_crypt::SecretBytes`
   (`crates/shared/wyrd-crypt/src/lib.rs`), next to `SecretKey`. It is a
   `Zeroizing<Vec<u8>>` with redacted `Debug`, and it grows by moving into a
   fresh buffer and zeroizing the old one, so in-place reallocation never
   leaves a copy. `Write` lets `serde_json::to_writer` serialize straight
   into it. The existing `Zeroizing<Vec<u8>>` accumulator did not cover
   this: `Vec` growth freed unzeroized buffers, the parsed `Value` strings
   and `json!` bodies were independent allocations, and its `Debug`
   printed the bytes.
   - Requests: the GCP, AWS and Transit encrypt bodies are borrowed
     `Serialize` structs. Their base64 plaintext is a `Zeroizing<String>`
     serialized into `SecretBytes` and handed to reqwest as
     `Bytes::from_owner(SecretBytes)`. The body is therefore zeroized
     whenever reqwest drops it: on completion, on provider error, or when
     the call future is cancelled.
   - AWS now signs and sends one shared `Bytes`, and the
     `to_vec`/`clone` duplicate is gone.
   - Answers: `remote::send` and Vault `Endpoint::send` return the raw
     body as `SecretBytes`. Decrypt answers are parsed in place
     (`remote::plaintext`; `DecryptAnswer` in wyrd-vault) with borrowed
     `&str` fields. An answer that would need an unescaped copy is refused
     as `Transport` / `Unavailable`; provider base64 output never contains
     an escape.
   - Unwrapped DEKs return as `SecretBytes`.
   - Answers that carry no key (ciphertext, AWS `__type`) and the KV
     envelope still parse to `Value`. KV status, missing and size behavior
     is unchanged.
   - Not claimed: buffers inside reqwest, hyper, TLS or the kernel, or
     what base64 and serde_json hold on the stack.

   Tests:
   - RED, on the 584a5de8b code with the new tests:
     `remote_tests::remote_unwrapped_deks_have_redacted_debug` failed at
     the `!debug.contains("171")` assertion, because `Zeroizing` printed
     the DEK bytes.
     `remote_tests::remote_plaintext_answers_needing_a_copy_are_refused`
     failed with `left: Ok(())`, `right: Err(Unavailable { backend: GcpKms,
     failure: Transport })`.
   - GREEN, both: `mise exec -- cargo nextest run --locked -p wyrd-kms
     --lib -E 'test(=remote_tests::remote_unwrapped_deks_have_redacted_debug)
     | test(=remote_tests::remote_plaintext_answers_needing_a_copy_are_refused)'`.
   - `tests::secret_bytes_grow_by_moving_and_redact_debug` (wyrd-crypt)
     proves growth moves to a new buffer and keeps the content, and that
     `Write` and redacted `Debug` work. It was written after the type, so
     it is test-after for the helper itself.
   - Existing round trips keep the wire shapes, contexts and refusal
     classes: GCP, AWS, Transit, failure and renew/rewrap in
     `remote_tests`; `transit_wraps_and_unwraps_through_the_named_key`
     (exact `body_json`) and the KV tests in wyrd-vault; real Transit via
     `mise run test:kms:vault` (1/1); and the Gateway ExternalSecret
     journeys (`test:gateway:vault:inner` under
     `scripts/postgres/with-test-postgres.sh`, 2/2).
   - No test reads freed memory. That cleanup happens on error and
     cancellation follows from ownership, shown in the source: every
     key-bearing buffer is owned by `SecretBytes` or `Zeroizing` until it
     drops.
2. **Federated ADC constructor and refresh** — FIND-TASK-001-2, REQ-005.
   Supply a valid federated workload ADC fixture through the actual ambient
   credential constructor, with local subject-token/exchange services. Build
   the GCP backend, seal/open through the facade, expire/refresh its access
   credential and observe a subsequent authenticated request using the new
   credential. A refused exchange must return redacted failure and no
   replacement envelope. Use local protocol fixtures in the KMS owner; keep
   the existing injected-credential tests for KMS wire assertions. Do not
   substitute a direct injected provider for the constructor being fixed.

   **Evidence (commit `9fd0858b4`). PASS.** Owner: `GcpKms::new`
   (`crates/wyrd/wyrd-kms/src/gcp.rs`). It now builds Google's
   `google-cloud-auth` 1.19 application default credentials
   (`Builder::default().build_access_token_credentials()`), adapted to the
   existing `GcpCredentialProvider` by the private `Adc` provider.
   - The gap: the locked `object_store` 0.14.2 ADC enum has no
     `external_account` variant.
   - Why this crate: `google-cloud-auth` is Google's reference ADC
     implementation. It covers `service_account`, `authorized_user`,
     `external_account` (file, url, executable and aws sources) and
     impersonation, and its token cache refreshes before expiry.
   - It uses the workspace's `reqwest` 0.13 with default features off, and
     Rustls comes from `wyrd_tls`. It adds `google-cloud-gax`, `-rpc` and
     `-wkt`; `check:deps` and `check:workspace-hack` pass.
   - The cache refresh task needs a Tokio runtime, so the constructor
     refuses with a `backend` config error when none is current.
   - The CryptoKey selection, the facade, the injected-credential tests and
     the `Rotating` wire assertions are unchanged.

   Test: `remote_tests::gcp_federated_adc_builds_through_the_ambient_constructor_and_refreshes`.
   - Setup: `GOOGLE_APPLICATION_CREDENTIALS` names an `external_account`
     file with a file subject-token source and a local STS `token_url`.
   - Seal and open carry `Bearer fed-0`. After that token's one-second
     lifetime, the next seal carries `Bearer fed-1`.
   - A refused exchange (STS 400 for `revoked-jwt`) returns
     `Unavailable { GcpKms, Credential }` from both `renew` and `seal`, so
     no replacement envelope is produced. The error text has no subject
     token, provider text or URL.
   - RED, against the 584a5de8b constructor: `builds from federated ADC:
     Config { setting: "backend", reason: "could not load GCP workload
     credentials" }`.
   - GREEN, in about 10 s: `mise exec -- cargo nextest run --locked -p
     wyrd-kms --lib -E
     'test(=remote_tests::gcp_federated_adc_builds_through_the_ambient_constructor_and_refreshes)'`.
     The test's only wait is 1.1 s for the token to expire. The rest is
     the library's fixed 10 s short-refresh interval, during which the
     request waits for the new token.
   - Limits: no live GCP or STS. A permanent (4xx) exchange failure stops
     that credential's refresh task, so the backend reports `Credential`
     until restart; this is `google-cloud-auth` behavior. Credential
     acquisition is not bounded by the 10 s call timeout.
3. **Shared consumer qualification** — FIND-TASK-001-3, existing benchmark
   acceptance. Inventory every added/moved PostgreSQL, Scribe, object-store
   and Vault wait for the remediated candidate. Run a full clean
   `mise run bench:l1:gateway` with VALID verdict and compare with the newest
   same-experiment/same-host record under `benchmarks/history/`. Use the
   canonical report and comparison readers and benchmark AGENTS reporting
   sections. Smoke, steps, profiled or dirty runs cannot close this scenario.

   **Evidence. NOT CLOSED: the benchmark was not run, at the maintainer's
   direction.**
   - Wait inventory for candidate `9fd0858b4`, on the Gateway ExternalSecret
     path `GatewayEngine::attempt` → `CredentialResolver::resolve` →
     `VaultBackend::fetch` → `VaultKv2::read_field` → `Endpoint::send`:
     - Vault: the same one KV HTTP request and the same response-chunk
       awaits. The body now builds in `SecretBytes`, which copies when it
       grows, and is then parsed to `Value`.
     - PostgreSQL, Scribe, object storage: none added or moved.
     - The KMS provider calls and the GCP `google-cloud-auth` token cache
       have no production caller yet.
   - The original task's no-hot-path claim is corrected in place.
   - A full clean `mise run bench:l1:gateway` was started in a clean
     worktree at `9fd0858b4` and stopped during its release build, when
     the maintainer said not to benchmark. No record and no comparison with
     `gateway/overhead/20261031T131441Z-92b62f9c0.json` exist, so
     FIND-TASK-001-3 stays open.

## Verification and preserved scope

Run newly added tests individually with the exact nextest selector through
`mise exec -- cargo nextest run --locked -p <owner> --lib -E 'test(=<name>)'`.
Then run the owning libraries:
`mise exec -- cargo nextest run --locked -p wyrd-kms -p wyrd-vault -p wyrd-crypt --lib`,
and real Transit through `mise run test:kms:vault`.
If shared Vault handling changes, retain the focused ExternalSecret Gateway
journey with the existing repository Postgres/Vault lifecycle wrapper
(`test:gateway:vault:inner` as recorded in the original task).
Run affected format/lint/dependency checks and `docs:check` if documentation
changes; `codegen:check` applies only if an actual generated contract changes.
The mandatory full benchmark is separate from these functional lanes.

Preserve independent DEKs, two-layer owner binding, active/retained reference
allowlisting, file/memory behavior, renewal versus rewrap semantics, provider
failure refusal, and the real dev Transit boundary. Do not add a cipher,
registry, SQL/migration, tenant KMS layer, cache policy, server cutover, public
resource action or broader journey suite. Historical TDD and diagnosis
deviations remain recorded; no retroactive test-first claim is closure work.

Record results against these scenarios and return the new immutable candidate
for maintainer-initiated remediation review. Do not launch another review
automatically.
