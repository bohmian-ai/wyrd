# Provider-secret, cryptography, sealing, and file-loading domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `c2787b37d456cd2eeeee01e04a9f8bbfdf8866e5`
- Candidate tree: `e389f56318a664920b5ebead0aee45d20fda6b23`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Prior remediation tasks: `TASK-001-R1-production-readiness-gaps.md`,
  `TASK-001-R2-remaining-production-readiness-gaps.md`, and
  `TASK-001-R3-production-readiness-gaps.md` in their respective review
  directories.

The candidate commit and tree matched the supplied immutable identities before
and after this review.

## Reviewed boundary

This review traced tenant provider secrets and activation recovery credentials
from the typed HTTP boundary through authorization, validation, sealing, SQL
storage, provider qualification, login-time opening, responses, errors, traces,
logs, audit, removal, and generated schemas. It also traced the deployment
sealing-key lifecycle across the tenant human-connection, workload-issuer, and
platform-connection stores: versioned AES-256-GCM envelopes, key selection,
keyless-boot inventory, compare-and-swap rewrap, rolling writers, post-roll
verification, K2-only serving, and old-key retirement. The review additionally
covered active, retained, and signing-key file loading, including restrictive
permissions, regular-file checks, opened-handle reads, byte bounds, redacted
errors, and operator replacement guidance.

## Authority and source coverage

| Boundary | Governing obligation | Source and proof inspected | Result |
|---|---|---|---|
| Authorized secret intake and client-auth rules | Spec `REQ-004`, `REQ-005`, `REQ-017`; TASK-001 connection contract; security posture secret rules | `wyrd-server/src/components/admin/identity.rs:139-195,266-313`; `wyrd-spec/src/auth/human_connection.rs:172-258,278-289`; `wyrd-auth/src/connections.rs:253-350,437-512,925-989` | PASS |
| Plaintext redaction and durable ciphertext | `REQ-005`; `AGENTS.md` secret/error rules; security posture cryptography rules | `wyrd-crypt/src/lib.rs:12-43,96-305`; `wyrd-auth/src/pg_resolvers.rs:377-453`; `wyrd-sql` human-connection migration and query owner; redacted views and `skip_all` handlers | PASS |
| Responses, errors, traces, logs, audit, and generated artifacts | `REQ-005`, `REQ-017`; task rule that provider and recovery secrets cross no evidence boundary | `HumanConnectionView`; `SecretBearer`/`SecretString`; stable connection errors; fixed audit fields; recovery decision attribution by non-secret credential id; admin journey and served OpenAPI assertions | PASS |
| Keyless boot and complete persistent inventory | `REQ-005`; prior `FIND-TASK-001-11` | `wyrd-auth/src/sealing.rs:58-154`; `wyrd-server/src/boot/mod.rs:1508-1550,2185-2237`; tenant and platform query slots; focused keyless-boot test source | PASS |
| Rotation, retained keys, rewrap, and retirement | `REQ-005`, `AC-007`; prior `FIND-TASK-001-12` | `SealingKeyring`; cross-store exact-byte CAS rewrap; module contract; `identity_e2e.rs:2131-2247`; self-hosting rotation runbook | PASS |
| Restrictive bounded key-file reads | Security posture file-mounted-secret rule; prior `FIND-TASK-001-17`; R3 accepted signing-key reuse | `wyrd-gateway/src/credential.rs:212-255`; `wyrd-server/src/config.rs:3384-3489,5409-5543`; self-hosting authentication guide | PASS |
| Secret-bearing provider transport | `REQ-004`, `REQ-005`, `INV-004`; prior `FIND-TASK-001-18`, `FIND-TASK-001-19`, `FIND-TASK-001-20`, `FIND-TASK-001-23` | shared screened/pinned/proxy-free client; HTTPS production scheme rule; bounded DNS and decoded bodies; real and probe token request construction | PASS |

## Prior-finding closure

- `FIND-TASK-001-11` remains closed. Boot runs the shared cross-store inventory
  even with no keyring and refuses readiness when any tenant human, workload,
  or platform provider ciphertext exists.
- `FIND-TASK-001-12` remains closed. The rotation journey creates a late K1
  write, shuts down the final K1 writer, runs the final K2-write/K1-retained
  pass, verifies both live human-connection ciphertexts under K2, and serves
  through a K2-only replica before retirement.
- `FIND-TASK-001-13` remains closed. `Public` refuses every present secret,
  including an empty value, while secret methods require a present nonempty
  value; the database independently enforces the corresponding ciphertext
  invariant.
- `FIND-TASK-001-17` remains closed. Active and retained sealing-key files use
  the shared opened-handle, regular-file, owner-only, 64-KiB-bounded loader.
  The R3 signing-key change correctly reuses that same loader and static
  redacted errors instead of introducing a weaker file path.
- `FIND-TASK-001-18`, `FIND-TASK-001-19`, `FIND-TASK-001-20`, and
  `FIND-TASK-001-23` remain closed for this boundary. Production provider
  fetches and the live browser authorization destination reject cleartext;
  provider DNS and decoded response bodies are bounded; pinned requests ignore
  ambient proxies and redirects.

## Material proposed findings

None.

## Positive controls

- Provider and recovery secrets enter through redacted wire wrappers; handlers
  use `#[tracing::instrument(skip_all)]`, and audit stores typed identity,
  permission, outcome, and non-secret credential id rather than credentials or
  request bodies.
- AES-256-GCM uses operating-system randomness for each 96-bit nonce.
  Versioned envelopes expose only a domain-separated key fingerprint, and both
  key and keyring `Debug` output omit key bytes.
- Secrets are sealed before entering the durable write shape. Live secret-auth
  rows require ciphertext, public rows forbid it, and tombstoning clears it.
- Rewrap covers all three persistent provider-secret stores and replaces only
  the exact ciphertext read, preserving a concurrent configuration write for a
  later pass instead of overwriting it.
- Unknown, malformed, tampered, retired-key, missing-keyring, and invalid-UTF-8
  values fail closed through redacted errors.
- The key-file loader opens once, inspects metadata on that handle, reads at
  most 64 KiB plus one byte, and returns only static failure reasons; the
  runbook requires owner-only modes and atomic same-directory replacement.
- The retirement guide explicitly says an in-roll zero count proves nothing
  and authorizes old-key removal only after every old-key writer is gone and a
  later pass reports zero remaining values.

## Verification limits

- This was a static, review-only audit as instructed. No Cargo, `mise`,
  Postgres, Keycloak, or Dex lane was run by this reviewer.
- `git diff --check` passed for the immutable base-to-candidate range. The
  keyless-boot, rotation, restrictive-file, contract, OpenAPI, audit-failure,
  and bounded-provider tests were inspected in source but their committed task
  execution evidence was not independently reproduced.
- No runtime log capture was available. Leakage resistance is established by
  the redacted secret types, `skip_all` instrumentation, bounded public error
  mapping, fixed audit payloads, and inspected log call sites; runtime sink
  configuration remains outside this static proof.
- The journey proves the temporal rotation rule with the human-connection
  store. Workload and platform stores use the same enumerated rewrap owner and
  exact-byte CAS path, but were not separately exercised by a live retirement
  journey; this is a verification limit, not a distinct reachable defect.

## Overall result

**PASS** — the cumulative candidate satisfies the reviewed provider-secret,
cryptography, sealing, redaction, rotation, and restrictive key-file-loading
obligations. All prior findings in this domain remain closed, and no new
material exploitable risk or task-acceptance defect was found.
