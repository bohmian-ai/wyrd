# TASK-001 independent findings validation

Validated against base `a859dab1d20806e8ad29d9eb064585126419f5c2` and
candidate `584a5de8b97f2d5bd2d258edbdd7ef2f95d8aa82`, approved
SPEC-stored-secret-encryption revision 1, and the original TASK-001.
This is a separate validator context from both discovery reviewers.
The candidate and reviewed source remained unchanged. No implementation was
performed. Commands and execution results are in [verification.md](verification.md).

## Validation decisions

| Discovery proposal | Decision | Retained ID | Reason |
| --- | --- | --- | --- |
| DOMAIN-01 | CONFIRMED | FIND-TASK-001-1 | Ordinary owned JSON strings and serialized buffers contain plaintext DEKs; zeroizing decoded/raw buffers does not clear those independent allocations. Explicit secret-handling authority applies to this new owner. |
| DOMAIN-02 | WITHDRAWN after maintainer scope clarification, 2026-11-19 | FIND-TASK-001-2 (withdrawn) | Federation is authentication work outside this encryption packet. The review incorrectly inferred a new required authentication capability from general ADC wording. |
| BEHAVIOR-1 | WITHDRAWN by maintainer, 2026-11-19 | FIND-TASK-001-3 (withdrawn) | The maintainer removed the automatic benchmark requirement. No performance regression was demonstrated; this is no longer a blocker or remediation obligation. |

The authentication proposal was withdrawn after maintainer scope clarification;
the benchmark proposal was withdrawn by explicit maintainer direction. No new blocking finding originated
with this validator. The retained findings are recorded once below.

## FIND-TASK-001-1 — VIOLATION: remote DEK copies escape secret ownership

**Requirement/authority.** Spec REQ-008 requires transient key/plaintext buffers
to use shared redaction and zeroization. Architecture
`wyrd-security-posture.md:969-970` requires redacted wrappers for secret-bearing
Rust types. This invariant applies to TASK-001's new encryption capability
even though resource action/CAS work under REQ-008 is assigned later.

**Locations.** `crates/wyrd/wyrd-kms/src/gcp.rs:92-95,139-149`;
`crates/wyrd/wyrd-kms/src/aws.rs:95-99,150-165`;
`crates/wyrd/wyrd-kms/src/remote.rs:73-79,87-101`;
`crates/shared/wyrd-vault/src/lib.rs:230-237,268-278,397-407`.

**Starting condition and trigger.** A configured GCP, AWS, or Transit backend
seals/opens a secret, renews it, or rewraps its DEK. Ordinary successful
operations produce base64 plaintext-DEK request or response fields.

**Expected and actual.** Application-owned transient key representations must
remain under protected ownership through completion, error and cancellation.
Instead, `json!` creates ordinary key-bearing strings. AWS serializes that
value into an ordinary vector and clones it for signing. Decrypt answers
become ordinary owned `serde_json::Value` strings. `remote::send` and Vault's
`Endpoint::send` zeroize their raw accumulator, and decoded vectors zeroize,
but neither action clears the independently allocated parsed strings or
request bodies. Encoding a key as base64 does not encrypt it. These ordinary
values also have revealing Debug implementations. No actual log leak or
recovery of freed memory is alleged.

**Independent source validation.** Traced facade `seal/open/renew/rewrap` to
each adapter; inspected both request serialization and response extraction.
The returned `SecretKey`/decoded vector's drop protection does not own the
other representations. `serde_json::from_slice` returns an owned `Value`,
not a view into the protected raw buffer. Existing protocol tests assert
correct context and plaintext round trips, not ownership/cleanup of these
additional application-controlled copies.

**Consequence.** Every remote key operation leaves extra plaintext-key
representations outside the explicit secret-handling boundary. This is a
direct supported-use invariant violation, without requiring an attacker input
or an invented exploit prerequisite.

**Smallest correction.** Correct key-bearing serialization and decoding at
the existing remote adapters and Transit owner using the installed secret and
zeroization facilities. Avoid the unnecessary AWS body duplicate. Preserve
the envelope, provider protocols, response bounds, refusal behavior, screened
Vault transport and KV consumer. Demonstrate protected application-owned
representations and cleanup paths; do not promise control over arbitrary
TLS/kernel allocations or read freed memory in a test.

## Withdrawn authentication finding

FIND-TASK-001-2 is withdrawn following the maintainer scope clarification on
2026-11-19. This packet changes encryption of stored secrets; it does not
introduce federation or expand credential acquisition. The review incorrectly
promoted an interpretation of general ADC wording into mandatory auth work.
The original discovery report preserves that superseded reasoning. This
withdrawal does not choose whether to retain, relocate, or remove the added
authentication dependency; no revert is prescribed.

## Withdrawn benchmark finding

FIND-TASK-001-3 was withdrawn by explicit maintainer direction on 2026-11-19.
Touching the existing Gateway Vault path does not automatically require a
benchmark. The source-based external-wait inventory remains in `verdict.md`;
no benchmark work remains assigned by this review. The original discovery
report preserves the superseded reasoning for provenance.

## Coverage reconciliation

The primary report covers assigned REQ-001–005/scenarios 1–5, authority order,
new owner/reuse, crate boundaries, configuration, draft documentation,
renewal/rewrap, existing consumers, and the four test-only server fixes. The
security report supplies the separate expertise for key/AAD trust, local file
custody, provider authentication, lifetime and refusal. This selection is
sufficient; no additional SQL, SDK/public projection, distributed lifecycle,
or persistence boundary ships in TASK-001. Root did not validate its own
findings, and reviewers did not validate their own new proposals.

Independently sampled actual fresh-DEK/nonce, substitution, unknown-format,
retained-key, GCP/AWS context/refresh, and real-Transit assertions, plus the
four test diffs. The core facade/protocol evidence remains credible. GCP/AWS
fixtures establish protocol behavior. Federation constructor support is
outside this encryption review after the maintainer scope clarification. No server cutover/journey is required of this internal preparatory
task; those remain later-task acceptance work. The ten-second duration in the
draft describes the HTTP bound more broadly than credential acquisition;
do not infer an additional approved end-to-end timeout requirement from it.

TDD deviations remain historical process facts: scenario 2 and remote
adapter work lacked RED; mutation checks do not restore test-first history.
The directory fix's prior-diagnostician deviation remains recorded. Current
GREEN reruns cannot prove historical RED or missing consultation.
