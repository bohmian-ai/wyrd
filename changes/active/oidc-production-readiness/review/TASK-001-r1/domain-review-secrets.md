# Provider-secret and sealing-key domain review

## Immutable subject

- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `e126cdca7d4bf5bc467279df05cc3e199eb7fdf2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`

## Reviewed boundary

This review traced provider secrets from the typed administration request through validation, redacted authorization/audit handling, AES-256-GCM sealing, Postgres persistence, provider-test and login decryption, and removal. It also traced deployment sealing-key loading, versioned key identifiers and nonces, cross-tenant startup rewrap of tenant human connections, workload issuers, and the platform connection, operator rotation instructions, old-key retirement, and the secret-free `Public` client path.

## Authority and source coverage

| Boundary | Authority | Source and proof inspected | Result |
|---|---|---|---|
| Secret acceptance and redaction | `REQ-004`, `REQ-005`, `REQ-017`; task packet-local connection contract; `AGENTS.md` §4, §9; `architecture/wyrd-security-posture.md` secret rules | `wyrd-spec/src/auth/human_connection.rs`; `wyrd-server/src/components/admin/identity.rs`; `wyrd-auth/src/connections.rs`; `wyrd-auth/src/pg_resolvers.rs`; OpenAPI and identity journey assertions | PASS except SEC-003 |
| Encryption envelope and key selection | `REQ-005`; task rotation contract; security-posture cryptography rules | `wyrd-crypt/src/lib.rs`; `wyrd-auth/src/pg_resolvers.rs`; keyring unit tests | PASS |
| Durable ciphertext and deletion | `REQ-002`, `REQ-005`; tenant-isolation authority | human-connection migration and SQL query owner; RLS, secret/state check constraint, tombstone secret wipe, compare-and-swap rewrap | PASS |
| Startup configuration and failure semantics | `REQ-001`, `REQ-005`; task rotation contract; documented operator behavior | `wyrd-server/src/config.rs`; `wyrd-server/src/boot/mod.rs`; `wyrd-server/src/state.rs`; self-hosting authentication guide | FAIL: SEC-001 |
| Rotation ordering, verification, and retirement | `REQ-005`, `AC-007`; task requirement to retain the old key through rewrap and verification and retire it only after no ciphertext references it | `wyrd-auth/src/sealing.rs`; cross-tenant SQL rewrap slots; boot orchestration; rotation guide; `tenant_connection_rotation_journey` | FAIL: SEC-002 |
| Public client without a secret | `REQ-004`, task rule that `Public` requires no secret | `ConnectionInput::validate`; contract unit tests; DB constraint | FAIL: SEC-003 |
| Leakage through responses, errors, logs, audit, and schema | `REQ-005`, `REQ-017` | redacted `SecretBearer` debug; `skip_all` handler instrumentation; fixed audit resource/operations; redacted views; provider-test errors; keyring/debug tests; journey audit/response assertions | PASS |

## Material proposed findings

### SEC-001 — INCORRECT: a deployment can boot without a sealing key while stored provider secrets exist

- **Violated obligation:** `REQ-005` says the deployment sealing secret is required while provider secrets are stored. The shipped operator guide makes that behavior explicit: boot fails closed when a stored secret needs a sealing key and none is set.
- **Exact location:** `crates/wyrd/wyrd-server/src/boot/mod.rs:2157-2160` returns `Ok(None)` whenever no write key and no retained keys are configured; `crates/wyrd/wyrd-server/src/boot/mod.rs:1506-1509` then skips the only cross-store secret scan. `AppState::production_validate` has no stored-secret/keyring check. This contradicts `docs/src/content/docs/self-hosting/authentication.svx:72`.
- **Evidence and reachable path:** Configure and activate a secret-bearing tenant connection, stop the service, remove `WYRD_SEALING_KEY_FILE`, and restart. Keyring construction succeeds with `None`, rewrap returns immediately, and production validation does not inspect stored ciphertext, so the server becomes ready. The active connection later fails only when a login or provider test attempts decryption. Startup therefore accepts a deployment configuration in which a required secret authority is absent.
- **Observable consequence:** Operators receive a healthy boot for a deployment whose configured human, workload, or platform OIDC authentication is unusable. This also makes the documented fail-closed startup guarantee false.
- **Required testable correction:** At boot, use the existing cross-store sealed-secret inventory owner to distinguish a genuinely secret-free database from one containing ciphertext. When no keyring is configured, fail startup if any tenant or platform provider secret exists, while preserving successful keyless startup for a database with no stored secrets. Add a focused real-Postgres boot test for both halves.

### SEC-002 — INCORRECT: a zero rewrap report during a rolling key switch does not prove the old key is safe to retire

- **Violated obligation:** `REQ-005` and the task require the old key to remain through rewrap **and verification**, and permit retirement only when no ciphertext references it. `AC-007` requires tested provider-secret rotation across two replicas.
- **Exact location:** `crates/wyrd/wyrd-auth/src/sealing.rs:61-111` performs one snapshot walk and reports `remaining = 0`; `crates/wyrd/wyrd-server/src/boot/mod.rs:1499-1513` runs that pass once during each new replica's boot. `docs/src/content/docs/self-hosting/authentication.svx:78-81` says one zero boot report is sufficient to remove the old key. The journey at `crates/wyrd/wyrd-server/tests/identity_e2e.rs:2063-2104` rewraps one human-connection ciphertext but deliberately leaves replica A writing K1 and never proves retirement with a K2-only serving replica.
- **Evidence and reachable path:** During step 2's rolling deployment, new replica B (K2 write, K1 retained) can finish its scan and report zero while old replica A still serves administration with K1 as its write key. A tenant administrator can then stage or rotate a secret through A, creating a fresh K1 ciphertext after B's snapshot. Following step 4 and retiring K1 makes that connection permanently undecryptable by K2-only replicas even though the operator observed the prescribed zero report.
- **Observable consequence:** The documented successful rotation procedure can strand a provider connection after old-key retirement. The committed two-replica journey cannot catch this failure because it performs the post-rewrap secret mutation on B and never removes K1.
- **Required testable correction:** Make retirement proof occur only after every writer has switched to K2. The smallest acceptable boundary is an explicit post-roll verification pass (using the existing rewrap owner) after no K1 writer remains, and documentation that only a zero result from that pass authorizes retirement. Extend the two-replica journey to create a late K1 ciphertext after the first K2 scan, complete the post-roll verification, then start and use a K2-only replica successfully; cover all three provider-secret stores or add focused integration proof for the two stores not exercised by the journey.

### SEC-003 — INCORRECT: `Public` accepts an explicitly supplied empty `client_secret`

- **Violated obligation:** `REQ-004` and the task contract require `Public` to carry no secret; only secret-authentication methods may accept one.
- **Exact location:** `crates/wyrd-spec/src/auth/human_connection.rs:237-250` defines secret presence as `Option` containing a non-empty value. Consequently `client_secret: ""` is treated as absent even though the field was supplied. The unit coverage tests a non-empty extra secret only.
- **Evidence and reachable path:** Submit an otherwise valid `ConnectionInput` with `client_auth: "Public"` and `client_secret: ""`. Deserialization yields `Some(SecretBearer(""))`, `has_secret` is false, validation succeeds, and staging silently drops the supplied value.
- **Observable consequence:** The public API accepts an input shape its contract says must be refused, hiding operator/client misconfiguration and weakening the exact secret boundary.
- **Required testable correction:** Validate presence separately from non-empty content: any `Some` is invalid for `Public`, while secret methods require `Some` with a non-empty value. Add the missing empty-secret cases to the existing contract unit test and preserve the database invariant that public rows store `NULL`.

## Positive controls

- Provider and recovery secrets use redacted wrapper types at the wire boundary; request handlers use `#[tracing::instrument(skip_all)]`, and connection audit events contain fixed operation/resource metadata rather than request bodies.
- `HumanConnectionView`, list/test responses, audit assertions, and generated read schemas contain no secret, ciphertext, nonce, or key identifier.
- AES-256-GCM uses OS randomness for 96-bit nonces. Versioned ciphertext carries only a domain-separated key fingerprint, and key/keyring `Debug` output omits key bytes.
- Secret-bearing methods fail closed on absent ciphertext, absent keyrings at use time, failed authentication, invalid UTF-8, and unrecognized client-auth discriminants.
- Stored tenant secrets are behind forced RLS; deployment-wide rewrap is isolated to the explicit `OperatorPool`, and updates use compare-and-swap against the exact ciphertext read.
- Removal clears ciphertext transactionally, and the database constraint prevents live `Public` rows from storing ciphertext.
- Rewrap errors and logs expose table/tenant/count metadata, not ciphertext or plaintext.

## Verification limits

- The candidate remained at `e126cdca7d4bf5bc467279df05cc3e199eb7fdf2` throughout this review.
- Available verification evidence was the committed implementation-evidence table and committed tests. This domain review did not rerun the environment-owning Postgres/Keycloak/Dex lanes.
- The committed rotation journey proves one human secret is rewrapped and readable with K2, but does not test missing-key startup, an old writer creating ciphertext after a zero scan, K1 retirement, a K2-only server, workload/platform secret retirement, or `Public` plus an empty secret. Those are the concrete proof gaps captured above.
- No raw log artifact was available to independently inspect runtime secret leakage. Static instrumentation/error tracing and the committed response/audit assertions were inspected instead.

## Overall result

**FAIL**

The candidate protects ordinary provider-secret storage and disclosure boundaries, but it does not enforce the required key at startup, its documented zero-report retirement rule is unsafe with an old writer during a rolling switch, and its `Public` input contract accepts an explicitly supplied empty secret.
