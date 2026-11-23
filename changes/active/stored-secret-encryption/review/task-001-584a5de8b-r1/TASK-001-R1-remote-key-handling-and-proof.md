---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-stored-secret-encryption
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-008]
depends_on: [TASK-001]
parent_task: TASK-001
remediates: [FIND-TASK-001-1]
---

# Complete remote key handling and acceptance proof

## Outcome and reviewed subject

Make the existing remote KMS capability preserve its secret-buffer invariant. Successful
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

FIND-TASK-001-2 is withdrawn following maintainer clarification on
2026-11-19: federation is authentication work outside this encryption packet.
No new authentication flow or vendor SDK is required by this task.

The maintainer withdrew FIND-TASK-001-3 and its benchmark requirement on
2026-11-19. No benchmark is assigned by this remediation task.

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
## Historical disposition of withdrawn authentication work

   **WITHDRAWN.** The reviewer withdrew FIND-TASK-001-2 from this task as
   scope expansion: no Wyrd authority requires Workload Identity Federation.
   Commit `9fd0858b4` (`google-cloud-auth` and its three Google crates) is
   reverted. `GcpKms::new` is back on the existing `object_store` credential
   chain (service-account key, gcloud user credential, metadata server), and
   the federation test and its reference-page wording are removed.

## Preserved external-wait audit

The recorded audit traces Gateway ExternalSecret
through `GatewayEngine::attempt` → `CredentialResolver::resolve` →
`VaultBackend::fetch` → `VaultKv2::read_field` → `Endpoint::send`. It retains
one Vault KV request and its response-chunk waits; no PostgreSQL, Scribe or
object-storage wait is added or moved. New KMS providers have no production
consumer yet. The maintainer stopped the attempted benchmark during its
release build; no benchmark result is claimed or required for closure.

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

Preserve independent DEKs, two-layer owner binding, active/retained reference
allowlisting, file/memory behavior, renewal versus rewrap semantics, provider
failure refusal, and the real dev Transit boundary. Do not add a cipher,
registry, SQL/migration, tenant KMS layer, cache policy, server cutover, public
resource action or broader journey suite. Historical TDD and diagnosis
deviations remain recorded; no retroactive test-first claim is closure work.

Record results against these scenarios and return the new immutable candidate
for maintainer-initiated remediation review. Do not launch another review
automatically.
