# Recommended user journeys

Four journeys describe the user goals. Owner types, backends, API projections,
failures, and security invariants are coverage within these journeys, not
additional journeys. These are recommendations for the later specification;
the design remains finalized and no implementation is authorized.

## 1. Get started and use stored credentials

**Given** a new Wyrd deployment,
**when** the administrator completes setup, stores a credential, and uses the
capability that needs it,
**then** the operation succeeds without feature-specific encryption setup, and
its credential remains usable after restart.

The local path is `wyrd init` → Operator connection → failed Verifier → delivery.
Other existing secret owners exercise their actual use, not merely a redacted
GET. Durable deployments preserve wrapping-key access; explicit memory-backed
development is disposable and never claims to recover ciphertext after key loss.

## 2. Rotate a secret without changing clients

**Given** clients use a resource containing a stored secret,
**when** an authorized administrator invokes Rotate to renew encryption or
supply a replacement credential,
**then** Wyrd protects the resulting value with a fresh data key and applies it
under the same resource identity, without client reconfiguration.

Cover a working credential and an expired/leaked credential that needs a valid
replacement. Supplying a replacement must not require opening the old value.
Encryption-only renewal preserves sessions and the underlying credential value.
Actual auth value replacement retains its existing validation/session lifecycle.
A failed operation preserves committed state; a concurrent change is not lost.
Copied provider credentials require issuer revocation too. Replica refresh and
already-admitted work retain their existing owner semantics.

## 3. Revoke a secret and stop its use

**Given** a credential is in use, including on replicas with cached material,
**when** an authorized administrator invokes Revoke,
**then** Wyrd disables that resource and subsequent use observes revocation
through the existing owner checks and cache-refresh mechanism. Other credentials
remain usable.

Restart and concurrent rotation must not re-enable it. Preserve the existing
notification/reload propagation and in-flight-operation behavior; do not add an
instantaneous cross-replica completion guarantee. Revocation in Wyrd is not
upstream issuer revocation or erasure of stolen plaintext.

## 4. Rotate the deployment wrapping key safely

**Given** a deployment with stored credentials across owners and tenants,
**when** the deployment administrator starts wrapping-key rotation or compromise
recovery,
**then** new writes use the replacement, existing client references remain
unchanged, and Wyrd completes the affected stored-secret transition with truthful
progress and no lost concurrent changes.

Routine rotation may rewrap existing DEKs; compromise recovery generates fresh
DEKs and ciphertext. Interrupted work resumes. Required old-key and backup
access is preserved until retirement is safe. Deliberate wrapping-key revocation
is deployment-wide containment and can interrupt dependent operations. A source
outage or missing key is reported, never hidden by generating a substitute.

## Coverage, not more journeys

Apply these journeys across the secret owners and backends they support, and
exercise each new operation through every public projection that ships it.
Use focused integration/crypto checks for tenant and owner binding, permissions,
fresh independent data keys, tampering, redaction, and failure/concurrency
invariants. Source outages, expired provider access, and restart/restore are
variations of the affected journey, not standalone user goals.

One facade and no new warm hot-path waits additionally need architecture and
performance evidence. The specification records the new public and persisted contracts while retaining
the existing owner semantics described in [notes.md](notes.md) and
[admin-journey.md](admin-journey.md). These scenarios do not introduce new
cache, session, coordination, or recovery policies.
