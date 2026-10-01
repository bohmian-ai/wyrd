# Provider-secret and sealing-key domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `bb4895d8e630ee8fb2ba075d6c3eeaa348e49414`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-001-r1/TASK-001-R1-production-readiness-gaps.md`

The candidate remained at the stated commit throughout this review.

## Reviewed boundary

This review traced provider-secret input and redaction from the typed HTTP
contract through candidate staging, sealing, SQL persistence, provider probes,
login-time opening, errors, traces, canonical audit, served schemas, and the
real-server journey. It also traced the shared sealing-key envelope and all
three persistent provider-secret stores (tenant human connections, workload
trusted issuers, and the platform OIDC connection) through boot inventory,
compare-and-swap rewrap, keyless refusal, documented rotation, and retirement
proof.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Secret-bearing input | Spec REQ-004/005; task packet contract; remediation `FIND-TASK-001-13` | `wyrd-spec/src/auth/human_connection.rs`, `secret_bearer.rs`, admin handler and OpenAPI contract test | PASS |
| Plaintext-to-ciphertext boundary | Spec REQ-005; task scenario 3 | `HumanConnections::stage`, shared `seal_secret`/`client_auth_from_row`, `SealingKeyring`, SQL row/write shapes and migration constraints | PASS |
| Read/error/log/audit redaction | Spec REQ-005/017; task secret and recovery-key rules | redacted views, `skip_all` handler instrumentation, connection error mapping, audit resource/events, admin journey redaction assertions | PASS |
| Stored-key inventory and keyless boot | Spec REQ-005; remediation `FIND-TASK-001-11` | `SealedSecretRewrap`, both tenant table slots, platform slot, `rewrap_sealed_secrets`, production and harness callers, focused Postgres boot test | PASS |
| Key identity, rewrap, and retirement | Spec REQ-005; task versioned ciphertext/key-ID rule; remediation `FIND-TASK-001-12` | `SealingKeyring`, CAS SQL slots, sealing module contract, runbook, rotation journey | FAIL |

## Prior-finding closure

- `FIND-TASK-001-11` is closed. Keyless boot now runs the existing cross-store
  inventory pass and refuses readiness when any provider ciphertext exists.
  The focused Postgres test passed during this review.
- `FIND-TASK-001-13` is closed. `ConnectionInput::validate` distinguishes
  presence from content: every supplied value is invalid for `Public`, while
  both secret methods require a nonempty value. The focused contract test
  passed during this review.
- `FIND-TASK-001-12` is only partially closed. The module contract and operator
  runbook correctly require a verification pass that starts after every writer
  uses K2, but the required journey does not execute that ordering. The retained
  finding below is the missing closure proof, not a request for a new rotation
  mechanism.

## Proposed findings

### SECRET-R2-001 — INCORRECT: the claimed post-roll test runs while a K1 writer is still serving

- **Violated obligation:** REQ-005 requires a tested rotation procedure; the
  original task requires retaining the old key through rewrap and verification
  and retiring it only when no ciphertext references it. Remediation
  `FIND-TASK-001-12` specifically requires a late K1 write, **all writers then
  moving to K2**, a final pass, and K2-only serving before retirement.
- **Exact location:**
  `crates/wyrd/wyrd-server/tests/identity_e2e.rs:2146-2195` and continued use of
  `replica_a` at `:2216-2228`.
- **Evidence:** Replica A is constructed with K1 as its write key, deliberately
  stages the late K1 candidate, and is never shut down or replaced before
  replica C is called the post-roll pass and the K2-only replica starts. The
  test continues using A afterward. This directly violates the runbook and
  module prerequisite that every serving writer use K2 before the final pass;
  A remains able to create another K1 ciphertext immediately after the asserted
  proof. The current assertions therefore prove that one late row can be
  resealed, but not that the post-roll retirement sequence is safe.
- **Observable consequence:** The required regression proof can pass without
  exercising the ordering that makes `remaining = 0` authoritative. A future
  test or lifecycle regression could again authorize K1 retirement while an
  old writer remains live, stranding a provider secret written after the scan.
- **Testable correction:** In the existing rotation journey, after the late K1
  write is observed, remove every K1-write-capable serving replica from service
  before starting the final K2-write/K1-retained pass. Run the K2-only boot and
  login only after that pass, and route the remainder of the journey through a
  K2 writer. Preserve the existing CAS rewrap engine and runbook; add no lease,
  coordinator, or new harness.
- **Focused closure proof:** The same journey must make it impossible for any
  live replica to seal with K1 before the final pass begins, then show the late
  ciphertext is current under K2 and a K2-only replica serves login. The test
  must fail if the old K1 writer remains serving through the purported
  post-roll proof.

## Verification performed

- `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=auth::human_connection::tests::secret_presence_follows_the_method)'` — PASS (1/1).
- `mise exec -- cargo nextest run --locked -p wyrd-crypt --lib -E 'test(=tests::keyring_rotation_rewraps_and_retires_the_old_key)'` — PASS (1/1).
- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=boot::sealing_boot_pg_tests::keyless_boot_refuses_only_when_ciphertext_is_stored)'"` — PASS (1/1).
- `git diff --check a5c8041a348a66bfb56fbac492383e8c688b0590..bb4895d8e630ee8fb2ba075d6c3eeaa348e49414` — PASS.

## Verification limits

- The Keycloak/Dex-backed ignored rotation journey was inspected but not rerun;
  it requires the repository identity environment. Its source is sufficient to
  establish `SECRET-R2-001` because the live K1 writer's construction, late
  write, absence of shutdown, and later reuse are explicit in one function.
- Broader repository lanes reported in the candidate task evidence were not
  rerun by this domain reviewer.
- No secret value was introduced into commands or output during review.

## Overall result

**FAIL** — provider-secret validation, redaction, sealing, and keyless boot
refusal are implemented, but the required post-roll retirement proof remains
incorrectly staged. `SECRET-R2-001` is bounded to the existing journey.
