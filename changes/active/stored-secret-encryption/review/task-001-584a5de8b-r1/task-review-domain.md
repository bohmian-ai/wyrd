# TASK-001 security boundary review

Maintainer amendment, 2026-11-19: DOMAIN-02 / FIND-TASK-001-2 is withdrawn.
Federation is authentication work outside this stored-secret encryption packet.
The proposal below preserves original reasoning, not a current requirement.

Subject: `/Users/stevenforrester/Documents/GitHub/wyrd`; base `a859dab1d20806e8ad29d9eb064585126419f5c2`; candidate `584a5de8b97f2d5bd2d258edbdd7ef2f95d8aa82`; approved `SPEC-stored-secret-encryption` revision 1; original `tasks/TASK-001-common-encryption-capability.md`. Reviewed the cumulative source diff directly. The authority update is the base commit itself and precedes the implementation. HEAD remained the candidate when checked. The dirty TASK-002 and TASK-004 files are excluded.

Assigned boundary: envelope cryptography and ownership authentication; configured key trust; local custody; remote authentication and credentials; provider refusal; key/plaintext lifetime; retained-key and renewal/rewrap security. No production owner cutover or SDK exposure is assigned to this task.

## Boundary coverage

| Boundary | Source and assertions inspected | Assessment |
| --- | --- | --- |
| Independent keys and AES-GCM | `wyrd-crypt/src/lib.rs:35-46,112-165`; `wyrd-kms/src/lib.rs:352-398`; local facade fresh-DEK/nonce and restart tests | OS RNG produces 32-byte keys and fresh nonces. Plaintext facade results and key owners zeroize. Remote transient copies remain a finding below. |
| Ownership/context binding | `wyrd-kms/src/lib.rs:117-138,352-398,456-484`; substituted-context test at `lib.rs:611`; remote protocol context assertions | Length-prefixed domain-separated encoding binds explicit scope, tenant, purpose, owner, authority count/parts and optional version. Both cryptographic layers receive it. Native provider mismatch refuses without fallback. |
| Envelope and reference trust | `envelope.rs:43-118`; `lib.rs:456-490`; config reference grammar | Format/algorithm checked before backend use; key is allowed before unwrap; opaque provider output remains opaque; DEK length checked before payload use. No stored endpoint/resource authority is added. |
| File and memory | `local.rs:38-62,78-115`; existing `wyrd-utils/src/secret.rs:49-77`; constructor/file tests | Only configured files loaded; file descriptor metadata enforces regular owner-only files, size bounded; no fallback; memory explicitly ephemeral and production-refused. Read/decode buffers zeroize in the new owner. |
| GCP authentication | `gcp.rs:46-58,133-150`; resolved `object_store` 0.14.2 GCP builder and credential implementations; fixture test at `remote_tests.rs:222` | Credential provider reused and fetched each call, with refresh delegated to its cache. Actual ADC constructor has a supported-use gap below; rotating fixture credentials bypass it. |
| AWS authentication | `aws.rs:48-61,135-166`; resolved `object_store` 0.14.2 `AwsAuthorizer::try_authorize` | Credentials are fetched each call; signer uses KMS service/ARN region and actual serialized payload hash. Explicit configured key and exact context included in Encrypt/Decrypt. Tests pin signature scope/header and changing credentials, rather than independently verifying the entire signature. No concrete signing defect found. |
| Transit authentication and retained versions | `transit.rs:37-105`; `wyrd-vault/src/lib.rs:178-407`; `tests/vault_transit.rs` | Token file reread through blocking strategy and restrictive file reader; token header marked sensitive; screening and redirect/proxy refusals preserved. Real dev test exercises associated-data refusal, native rotation/old readability, rewrap and bad token. |
| Refusal and recovery | `remote.rs:24-101`; `gcp.rs:150-159`; `aws.rs:167-183`; `transit.rs:108-116`; remote failure tests and renewal/rewrap failure test | Errors retain only static classes, no provider text. Provider failure yields no envelope; prior envelope remains usable after recovery. Unknown/disallowed envelope reaches no alternate source. HTTP requests are bounded; credential acquisition precedes the HTTP deadline (see limits). |
| Renewal/rewrap | `lib.rs:407-449`; lifecycle test at `lib.rs:821`; remote failure test at `remote_tests.rs:514` | Renewal opens/reseals with fresh key/nonce; routine rewrap unwraps/wraps the same DEK without payload decryption, preserving value ciphertext. `is_current` intentionally compares logical reference, not provider native version. |

Root reports an independent rerun of the combined KMS/Vault/crypt library tests passing 25/25 (nextest run `8f1d6641-a8e8-40bc-a288-6fe3ddd87edc`). This reviewer performed source/contract analysis and did not run Cargo concurrently. The root subsequently recorded all 15 individual KMS focused tests and the real Vault focused test passing, real dev Vault lane 1/1, and CI selection 64+7 passing in `verification.md`; see that report for exact commands. Execution evidence remains for the primary reviewer/validator to reconcile.

## Proposed findings for independent validation

### DOMAIN-01 — VIOLATION: remote adapters retain unprotected plaintext DEK copies

**Authority:** approved spec REQ-008 explicitly requires transient key/plaintext buffers to use shared redaction and zeroization facilities; architecture `wyrd-security-posture.md:969-970` requires secret-bearing Rust types to use redacted wrappers. This is an applicable safety invariant for the task's new encryption owner, although TASK-001's frontmatter maps its principal behavior to REQ-001–005. Finalized design's “One credential write and read,” step 5, also expressly carries this constraint.

**Exact locations:** `crates/wyrd/wyrd-kms/src/gcp.rs:92-95,139-149`; `crates/wyrd/wyrd-kms/src/aws.rs:95-99,150-165`; `crates/wyrd/wyrd-kms/src/remote.rs:73-79,87-101`; `crates/shared/wyrd-vault/src/lib.rs:230-237,268-278,397-407`.

**Starting conditions and trigger:** any supported remote-backed seal/open/renew/rewrap, including successful ordinary use, serializes or receives a plaintext DEK. GCP and Transit wrap create ordinary `serde_json::Value` strings containing base64 plaintext key material; AWS additionally serializes and clones that material into ordinary `Vec<u8>` request bodies. Decrypt responses are parsed into ordinary owned JSON strings containing the base64 DEK.

**Expected:** transient owned key representations have redacted access and zeroizing ownership through normal completion, errors and cancellation. Base64 is an encoding of the plaintext key, not encrypted key material.

**Actual and evidence:** `remote::send` correctly zeroizes its raw response accumulator, but `serde_json::from_slice` allocates a new ordinary `Value`. `base64_field` borrows its string, returning a separately zeroizing decoded vector; dropping that vector or the raw accumulator does not clear the parsed string. Transit repeats the same structure. `Value`/`String`/ordinary serialized vectors have no zeroizing owner, and their normal Debug exposes their contents. Consequently these copies survive deallocation in allocator memory despite the facade's correctly zeroizing DEK type. No observed log leak is alleged.

**Consequence/materiality:** every remote secret operation violates an explicit key-handling boundary and leaves additional recoverable plaintext-key representations outside the intended transient secret ownership. This is not an imagined malformed-input branch.

**Smallest correction boundary:** fix the new remote serialization/response handling in `wyrd-kms` and the shared Transit owner in `wyrd-vault`; reuse the installed zeroization/redaction facilities. Keep key-bearing parsed fields and owned wire buffers under secret ownership, avoid the unnecessary AWS plaintext-body duplicate, and preserve response size/time limits, exact provider protocols, refusal classes and existing KV behavior. Do not add a second cipher path or rewrite resource consumers. Validate cleanup/redacted Debug at the controllable owner boundary on success, provider error and cancellation, plus existing adapter round trips; do not attempt unsafe reads of freed allocations or promise zeroization of arbitrary TLS/kernel internals.

### DOMAIN-02 — INCORRECT: advertised GCP application-default workload credentials reject federated ADC

**Authority:** REQ-005 requires provider-standard workload authentication and credential lifecycle; architecture `operations/deployment-and-release.md:118-121` specifies the provider's standard workload credentials/application default credentials. The task's new draft reference explicitly directs operators to `GOOGLE_APPLICATION_CREDENTIALS` or the gcloud default. Self-hosted installations selecting remote GCP are explicitly supported by REQ-005. This finding concerns recognized standard ADC configurations, not a demand for a new Wyrd authentication option.

**Exact location:** `crates/wyrd/wyrd-kms/src/gcp.rs:46-56` delegates actual authentication construction to `object_store::gcp::GoogleCloudStorageBuilder`. Locked dependency `object_store` 0.14.2, `src/gcp/credential.rs:544-585`, defines ADC as only the `service_account` and `authorized_user` enum variants. Its builder `src/gcp/builder.rs:577-586` reads that enum during construction and propagates errors. ADC with `type: external_account` (Workload Identity Federation) is therefore rejected before any KMS call; impersonated service account ADC is also absent from that enum.

**Starting conditions and trigger:** a self-hosted deployment on premises or another cloud selects GCP KMS and configures the provider-recommended workload identity federation ADC file through `GOOGLE_APPLICATION_CREDENTIALS`; `WyrdKms::new` constructs the GCP backend.

**Expected:** the documented provider-standard ADC workload identity can obtain and refresh its access credential and perform the same authenticated wrap/unwrap path.

**Actual/evidence:** Serde's internally tagged enum has no `external_account` variant, and `.build()` errors are converted into `Config { setting: "backend", reason: "could not load GCP workload credentials" }`. All KMS GCP protocol tests construct `GcpKms::with_credentials` using `Rotating<GcpCredential>`, so none exercises the actual ADC constructor. Official provider documentation identifies federated configuration files as ADC inputs and recommends them for on-premises/other-cloud workloads: [How ADC works](https://docs.cloud.google.com/docs/authentication/application-default-credentials), [Set up ADC on premises or another cloud](https://docs.cloud.google.com/docs/authentication/set-up-adc-on-premises). The failure follows from the locked source enum; no live-cloud access is needed to reproduce startup rejection.

**Consequence/materiality:** a supported remote-backend deployment using standard short-lived workload identity cannot start the common capability, despite having valid provider permissions. Requiring a long-lived service account private key is not an equivalent implementation of its selected authentication lifecycle.

**Smallest correction boundary:** replace/extend credential acquisition behind the existing `GcpKms` owner with a reusable ADC provider that accepts the advertised standard workload configuration and refreshes it; reuse a suitable existing provider if available. Keep the common envelope, authentication context, configured KMS resource resolution and consuming API unchanged. Add a constructor-path fixture using a valid federated ADC configuration and local token-exchange endpoint, proving acquisition, refreshed use and redacted failure; existing injected-credential tests remain useful protocol checks. No live production cloud account or new public Wyrd authentication option is required for closure. If the maintainer instead chooses a narrower GCP authentication contract, that change needs explicit authority rather than silently treating a documented ADC type as unsupported.

## Limits and non-findings

- GCP/AWS tests are local protocol fixtures, not live-cloud proof. That is the task's stated boundary and is not independently a blocking gap.
- Token lookup/refresh occurs before the KMS HTTP client's 10-second timeout; `object_store` credential HTTP defaults have their own 30-second timeout/retry policy, and Vault token-file reads precede its HTTP bound. Thus the draft reference's “one wrap or unwrap call ... bounded to 10 seconds” is stronger than the complete operation actually guarantees. The approved spec does not fix that exact duration; the primary/validator should reconcile the documented claim without inventing a new product timeout requirement.
- Transit returns refused for context mismatch. REQ-003/004 require refusal, not a mandatory identical authentication error taxonomy, so this disclosed provider difference is acceptable.
- File initialization, production server config cutover, resource authorization, CAS/persistence, cache invalidation and public-language journeys belong to subsequent tasks. Their absence is not a TASK-001 finding.
- No runtime hot path is switched by this task; no external wait is added to a live verification, gateway, Bifrost query/append path. A performance benchmark is not required for the isolated library addition.
- TDD violations are recorded separately by the task; this specialist does not treat historical RED omissions as defects requiring production churn.
