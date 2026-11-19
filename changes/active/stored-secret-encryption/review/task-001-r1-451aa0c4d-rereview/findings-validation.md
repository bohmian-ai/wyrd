# TASK-001-R1 closure validation

## Subject and authority

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Prior reviewed candidate: `584a5de8b97f2d5bd2d258edbdd7ef2f95d8aa82`.
Remediated candidate: `451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a`.
Reviewed `git diff 584a5de8b 451aa0c4d` directly; the five-commit sequence
starts with `17d0e3e6b`, whose parent is the prior candidate. Using that fix
commit as an exclusive diff base would omit the correction being reviewed.
The original cumulative base remains `a859dab1d20806e8ad29d9eb064585126419f5c2`.

Contract: SPEC-stored-secret-encryption revision 1, original TASK-001, and
TASK-001-R1 in the prior review directory. Applied the maintainer's current
scope clarifications: FIND-2 is withdrawn as authentication scope expansion;
FIND-3 and automatic benchmark requirements are withdrawn. These decisions
are not new implementation defects. The working-tree documentation edits
record those instructions; they are not additional production source in the
immutable candidate.

Inspected root AGENTS, task-review skill, scope discipline, TESTING, Rust/error/
maintainer guides, security posture Cryptography and secret handling/Stored
secrets, the original finding, remediation scenario, and prior acceptance
evidence. No CodeGraph index exists. Production source and candidate remained
unchanged during this review.

## Finding disposition

| Stable finding | Disposition | Source-backed reason |
| --- | --- | --- |
| FIND-TASK-001-1 | CLOSED | The changed remote paths keep application-owned key-bearing request, response and decoded buffers under protected ownership, redact Debug, and remove the AWS serialized-body duplicate. Details below. |
| FIND-TASK-001-2 | WITHDRAWN | The maintainer excluded federation/new authentication capability from this encryption packet. The Google authentication addition is reverted; it is not required for acceptance. |
| FIND-TASK-001-3 | WITHDRAWN | The maintainer removed benchmark qualification. The external-wait inventory remains; no benchmark is required or claimed. |

No new blocking finding was identified. No new remediation task is warranted.

## Independent source validation of FIND-1

- `wyrd-crypt/src/lib.rs:63-129`: `SecretBytes` owns a
  `Zeroizing<Vec<u8>>`, exposes borrowed slices, implements `Write`, and
  prints `SecretBytes([REDACTED])`. Growth allocates a new protected buffer
  while the old allocation still exists, copies into it, then replaces the
  old zeroizing owner. Appending cannot reallocate that old buffer first.
  The installed zeroization facility supplies drop cleanup; this does not
  promise access to or tests of freed allocator memory.
- `wyrd-kms/src/remote.rs:43-54,64-90,102-130`: serialization writes directly
  into the protected owner; response accumulation uses its protected growth;
  the parsed plaintext field borrows the response body instead of creating
  an independently owned JSON string. Decoded successful DEKs return as
  `SecretBytes`. Missing/escaped/invalid fields refuse. Size and timeout
  limits remain at the existing transport boundary.
- `gcp.rs:82-168` and `aws.rs:82-213`: encrypt request structs borrow the
  base64 key held by `Zeroizing<String>`. Serialized key bodies transfer
  ownership into `Bytes::from_owner`; an error or dropped request future
  releases that owner. AWS signs and sends clones of the same `Bytes`
  allocation (`aws.rs:153-168`), rather than cloning a plaintext vector.
  Decrypt answers remain protected until borrowed parsing and key decoding
  finish. Existing context fields, key selection, methods and refusal mapping
  are preserved and exercised by the protocol tests.
- `wyrd-vault/src/lib.rs:230-322,410-442`: Transit encrypt serializes borrowed
  key fields into `SecretBytes`; decrypt borrows `data.plaintext` from its
  protected response. Shared `Endpoint::send` preserves screening, status
  handling and response bounds while returning protected bytes. KV parses
  those bytes at its existing consuming owner and retains its prior missing,
  deleted and destroyed-value semantics. This remediation does not expand
  into a rewrite of pre-existing KV value handling.
- `wyrd-kms/src/lib.rs:238-252,456-476` and `transit.rs:94-105`: adapter results
  reach the existing facade, which still rejects a non-32-byte DEK before
  constructing `SecretKey`. Local unwrap now adapts its existing zeroizing
  result to the same protected return owner; no local cipher or format changes.

The new owner is earned: existing `Zeroizing<Vec<u8>>` alone exposes Debug and
allows vector growth to free a previous allocation without first clearing it.
The correction reuses zeroization and ordinary serialization rather than
introducing another cipher, registry, provider SDK or credential system.
Net manifest changes reuse installed `bytes`/`serde` and the existing crypto
owner. The lockfile contains none of the reverted Google SDK packages.

The protected ownership claim is bounded to controllable application-owned
representations. It does not claim control over HTTP/TLS/kernel internals or
all third-party parser/base64 implementation temporaries. Success, provider
error and cancellation retain ordinary Rust ownership/drop behavior; no unsafe
inspection of freed memory is necessary to establish that structure.

## Assertions and current execution

The three new tests inspect actual behavior: growth moves allocations while
retaining content and redacting output; every remote adapter returns the
correct DEK with redacted Debug; decrypt answers needing an owned unescape
copy refuse on GCP, AWS and Transit. Existing provider tests retain configured
resources, exact context, key rotation/renewal and refusal assertions.

Root ran these against the current immutable candidate, sequentially:
The complete execution record is [verification.md](verification.md).

| Command/proof | Result |
| --- | --- |
| `mise exec -- cargo nextest run --locked -p wyrd-kms -p wyrd-vault -p wyrd-crypt --lib` | 28/28 PASS; run `8ec8d5e1-5c52-478a-b16c-453d85cffa27` |
| `mise run test:kms:vault` | Real dev Transit 1/1 PASS; run `f1932e09-4d54-467e-988c-3f3cf7fd7b72` |
| `mise exec -- scripts/postgres/with-test-postgres.sh -- mise run test:gateway:vault:inner` | Real server/Postgres/Vault ExternalSecret journeys 2/2 PASS; run `eb5c0152-dc95-4495-b204-086d666dc466`; 17 other module tests correctly excluded |
| Individual exact nextest invocation for each of the three new tests | All PASS; commands below |
| `git diff 584a5de8b 451aa0c4d --check` | PASS |

Exact new-test invocations:

```bash
mise exec -- cargo nextest run --locked -p wyrd-crypt --lib -E 'test(=tests::secret_bytes_grow_by_moving_and_redact_debug)'
mise exec -- cargo nextest run --locked -p wyrd-kms --lib -E 'test(=remote_tests::remote_unwrapped_deks_have_redacted_debug)'
mise exec -- cargo nextest run --locked -p wyrd-kms --lib -E 'test(=remote_tests::remote_plaintext_answers_needing_a_copy_are_refused)'
```

The task reports post-revert Clippy, dependency and workspace-hack checks
passing; this review did not independently rerun those wider checks. Present
GREEN execution does not independently establish historical RED chronology.
The recorded two-test RED/GREEN is retained; the helper test was written after
its type, and earlier task TDD violations remain historical process limits.
