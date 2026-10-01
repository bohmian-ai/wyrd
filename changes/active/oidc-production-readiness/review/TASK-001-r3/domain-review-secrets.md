# Provider-secret, sealing, and redaction domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `eb4142cc95fa24b0525c71f66bfc978577cd4b7c`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Prior remediation tasks: `changes/active/oidc-production-readiness/review/TASK-001-r1/TASK-001-R1-production-readiness-gaps.md` and `changes/active/oidc-production-readiness/review/TASK-001-r2/TASK-001-R2-remaining-production-readiness-gaps.md`

The candidate remained at the stated commit throughout this review.

## Reviewed boundary

This review traced tenant provider secrets and activation recovery credentials
from the typed HTTP boundary through validation, sealing, SQL storage, provider
qualification, login-time opening, responses, errors, traces, logs, audit, and
served schemas. It also traced the deployment sealing-key lifecycle across the
tenant human-connection, workload-issuer, and platform-connection stores:
versioned envelopes and key identifiers, restrictive key-file loading,
keyless-boot inventory, compare-and-swap rewrap, rolling writers, the post-roll
verification pass, K2-only serving, and old-key retirement guidance.

## Authority and source coverage

| Boundary | Governing obligation | Source and proof inspected | Result |
|---|---|---|---|
| Secret-bearing request contract and client-auth rules | Spec REQ-004/005; TASK-001 packet-local contract; prior `FIND-TASK-001-13` | `wyrd-spec/src/auth/human_connection.rs:70-114,172-258,278-289`; `auth/secret_bearer.rs:9-95`; candidate PUT and activation handlers | PASS |
| Authorized acceptance and recovery-key handling | REQ-005/017; task authorization-before-provider-IO and recovery-key rules | `wyrd-server/src/components/admin/identity.rs:126-195,240-313`; `wyrd-auth/src/connections.rs:253-350,437-506,918-952`; all handler spans use `skip_all` | PASS |
| Ciphertext storage and opening | REQ-005; security-posture cryptography rules | `wyrd-crypt/src/lib.rs:96-305`; `wyrd-auth/src/pg_resolvers.rs:242-302,377-454`; `wyrd-auth/src/connections.rs:266-293`; migration secret/state constraints | PASS |
| Read, response, error, log, trace, audit, and artifact redaction | REQ-005/017; security posture secret and audit rules | redacted `HumanConnectionView`; `SecretBearer` redacted `Debug`; stable public error mapping; `skip_all` handlers; audit event path; admin journey secret/ciphertext/audit assertions; served OpenAPI schema assertion | PASS |
| Keyless boot and complete persistent inventory | REQ-005; prior `FIND-TASK-001-11` | `wyrd-auth/src/sealing.rs:58-154`; tenant-table and platform query slots; `wyrd-server/src/boot/mod.rs:1508-1550,2196-2237`; focused boot test source | PASS |
| Restrictive active and retained key files | Security posture cryptography rules; R2 `FIND-TASK-001-17` | shared opened-handle `read_secret_file`; both config loaders at `config.rs:3410-3482`; restrictive/permissive/non-regular/oversized test; runbook modes and atomic replacement | PASS |
| Rolling rotation and retirement proof | REQ-005; task scenario 3; R1/R2 `FIND-TASK-001-12` | `SealingKeyring`; cross-store CAS rewrap; module contract; runbook; `identity_e2e.rs:2104-2264` | PASS |
| Secret-bearing provider transport | REQ-004/005 and INV-004; R2 `FIND-TASK-001-18/19` | shared screened pinned client, production HTTPS-only scheme rule, bounded decoded body reader, shared real/probe token request construction | PASS |

## Prior-finding closure

- `FIND-TASK-001-11` remains closed. Boot runs the existing cross-store
  inventory and refuses keyless readiness when provider ciphertext exists.
  Real server composition requires the operator capability used by that pass;
  the `None` branch is a non-production/test seam and cannot admit a normally
  composed serving process.
- `FIND-TASK-001-12` is closed. The journey now makes the late K1 write, shuts
  down replica A (the final K1 writer), starts the final K2-write/K1-retained
  pass from replica B, proves both Active and Candidate ciphertext are current
  under K2, and only then starts and continues through a K2-only replica.
- `FIND-TASK-001-13` remains closed. `Public` rejects any present secret,
  including an empty one, while secret methods require a present nonempty
  value.
- `FIND-TASK-001-17` is closed. Both active and retained key files reuse the
  already-open, regular-file, owner-only, 64-KiB-bounded loader, and errors
  carry only a static reason plus the configured path.
- `FIND-TASK-001-18` and `FIND-TASK-001-19` are closed for the secret boundary.
  Production provider clients refuse cleartext endpoints before request
  dispatch, and discovery, JWKS, candidate probes, and real token exchange all
  use the shared 1-MiB decoded-response reader.

## Findings

No material secret, cryptography, sealing, rotation, client-authentication, or
redaction finding remains in the reviewed task boundary.

## Positive controls

- AES-256-GCM uses OS randomness for every nonce, and versioned envelopes carry
  only a non-secret domain-separated key fingerprint.
- Secret inputs use redacted wrappers, never appear in connection views, and
  are sealed before entering the durable write shape.
- The database reasserts that live secret methods have ciphertext, Public has
  none, and removed tombstones retain none.
- Rewrap covers all three persistent provider-secret stores and uses exact-byte
  compare-and-swap updates, so it cannot overwrite a concurrent connection
  mutation.
- Unknown, malformed, tampered, or retired-key ciphertext fails closed and is
  logged without ciphertext or plaintext.
- Rotation documentation explicitly rejects an in-roll zero count as
  retirement proof and requires a pass begun after every K1 writer is gone.

## Verification limits

- This was a static, review-only domain audit. Per instruction, no Cargo or
  `mise` Cargo lane was run.
- `git diff --check` for the immutable base-to-candidate range passed.
- The Keycloak/Postgres rotation journey, keyless-boot test, restrictive-file
  test, contract tests, OpenAPI test, and bounded-response tests were inspected
  in source but not executed by this reviewer. Their reported execution in the
  task packet is supporting evidence, not independently reproduced evidence.
- Runtime log capture is not asserted by a dedicated journey. Redaction is
  supported statically by redacted secret wrappers, `skip_all` handler
  instrumentation, path-only request tracing, bounded public errors, and log
  call-site inspection; this is a verification limit, not a reachable leak.

## Overall result

**PASS** — the cumulative candidate satisfies the reviewed provider-secret,
sealing-key, rotation, client-authentication, and redaction obligations. The
validated prior gaps are closed and this review found no new material defect.
