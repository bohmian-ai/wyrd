---
id: TASK-002-R4
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-007, INV-004, AC-007]
depends_on: [TASK-002-R3]
parent_task: TASK-002
remediates: [FIND-TASK-002-11, FIND-TASK-002-12]
---

# Enforce OIDC ID-token claims and serialize refresh-family containment

## Authority and immutable subject

- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Review verdict:
  `changes/active/oidc-production-readiness/review/TASK-002-r4/verdict.md`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Reviewed candidate: `63e6a545db156a095b670a5bb8bc36f6f36fba32`

This remediation closes `FIND-TASK-002-11` and `FIND-TASK-002-12` without
changing the approved specification, public auth contract, or ownership model.

## Issue diagnosis

### FIND-TASK-002-11 — required ID-token binding and time claims are optional

REQ-007 requires the tenant callback to verify ID-token issuer, audience, time,
and claims before creating identity or authority. The current shared verifier at
`crates/shared/wyrd-auth-verify/src/lib.rs:506-550` configures issuer and
audience on `jsonwebtoken 9.3.1`, but that dependency requires only `exp` by
default, ignores a missing configured `iss` or `aud`, and leaves `nbf`
validation disabled. Neither the generic verifier nor the tenant callback at
`crates/wyrd/wyrd-auth/src/callback.rs:201-220` requires numeric `iat` or rejects
a future `iat`. Nonce, `azp`, and later claim mapping do not close those gaps.

The path is reachable for every tenant callback and also serves platform OIDC.
A correctly signed token can therefore establish authority without the required
claim binding or issued-at contract. Current wrong-value tests pass because
their tokens include the claims; they do not prove absence or future-time
refusal. Applying mandatory OIDC `iat` rules directly to the generic verifier
would be incorrect because its third production path verifies workload
assertions rather than ID tokens.

### FIND-TASK-002-12 — replay containment can miss a concurrent successor

The task requires refresh rotation to lock the token family and Active tenant
connection in one transaction while preserving replay-family containment. In
`crates/wyrd/wyrd-auth/src/refresh.rs:82-226`, active rotation consumes the
presented row and inserts its successor, while stale replay separately reads the
old row and issues a family-wide revocation. The paths share no lock for the
stored tenant principal family.

PostgreSQL row locking safely serializes two callers presenting the same current
row, which is what existing race tests cover. It does not serialize replay of
ancestor `A` against rotation of current row `B`. The replay update can wait on
`B`, then recheck it after rotation commits, while successor `C` remains absent
from the update's original statement snapshot. The server can consequently
commit `RefreshError::Reused` and canonical containment audit while `C` remains
usable. Existing sequential replay proofs do not exercise this overlap.

## Intended correction outcome

- Tenant and platform OIDC accept only signed ID tokens containing the exact
  configured issuer/audience claims, valid expiry and optional-not-future
  `nbf`, and a numeric `iat` no later than the current time plus existing clock
  skew.
- Workload assertion verification keeps its existing non-ID-token contract
  while still receiving the shared issuer/audience and present-`nbf` hardening.
- Every refresh operation for one stored tenant principal family is serialized
  before active/stale classification and remains serialized through successor
  issuance or replay revocation, audit, and caller-owned commit.
- Replaying any ancestor while the current token rotates cannot leave a usable
  successor after containment commits.

## Decision-complete recommendation

### ID-token verification owner

Keep signature, JWKS, issuer, audience, expiry, and claim mapping in the existing
`ExternalVerifier`; do not add a parallel verifier or caller-local JWT parsing.
Make its generic verification require `exp`, `iss`, and `aud`, and enable the
installed validator's handling of a present `nbf`. Add one explicit ID-token
semantic entry on that same owner which reuses generic verification and then
requires numeric, non-future `iat` using the existing allowed clock skew. Route
the two real OIDC ID-token consumers—tenant callback and platform login—through
that semantic entry. Leave the workload assertion path on generic verification.
Preserve existing error mapping and update documentation only where the changed
contract requires it.

This is the smallest safe boundary: shared cryptographic verification remains
single-owner, OIDC-only semantics are expressed once for its two callers, and
the installed dependency plus the standard clock are sufficient.

### Refresh-family serialization owner

Use the repository's existing PostgreSQL transaction-scoped advisory-lock
pattern inside the current refresh-token SQL/service owners. Resolve the
presented row under the existing `TenantConn` and forced-RLS boundary only far
enough to obtain its stored tenant principal family; unknown hashes still fail
as `NotFound`. Serialize on tenant, principal kind, and principal id before
deciding whether the token is active or stale, then re-read and perform rotation
or containment while retaining the lock through audit and the route-owned
commit. Preserve the existing human-connection slot lock and use the fixed
order family first, connection second.

This closes the cross-row snapshot race without a new persistence model,
isolation-level change, lease, retry protocol, or process-local lock.

## Constraints and preserved behavior

- Keep `wyrd-spec` IO-free, async-free, and PyO3-free; no public wire or schema
  change is required.
- Preserve the shared verifier/JWKS/screened-provider owners, HMAC refusal,
  advertised-algorithm check, nonce and `azp` checks, stable Wyrd errors, and
  redacted diagnostics.
- Preserve workload assertion compatibility except for already-required
  issuer/audience presence and refusal of a present future `nbf`; do not impose
  the OIDC ID-token `iat` contract on workload assertions.
- Keep all tenant refresh work on `TenantConn` with forced RLS and caller-owned
  commit. A callee must not commit or roll back.
- Preserve connection provenance, current-role recomputation, replay-family
  revocation, canonical transactional audit, and the route's deliberate commit
  of `RefreshError::Reused`.
- Preserve one fixed lock order: refresh family before human connection.
- Do not weaken validation, skip audit, broaden tenant access, or convert a
  required refusal into best-effort behavior.

## Non-goals

- No new JWT library, verifier implementation, claim DTO, clock-skew setting,
  public API, route, grant, compatibility path, or provider behavior.
- No refresh-family table/id, repository abstraction, trait, lock service,
  lease, retry loop, serializable transaction, or process-local mutex.
- No BFF completion work, CLI handoff persistence, email linking, provider-token
  bearer authority, platform fallback, or new machine identity behavior.
- No unrelated refactor or broad test-harness change.

## Acceptance criteria

### AC-R4-01 — `FIND-TASK-002-11`

Tenant and platform OIDC ID-token paths reject signed tokens with missing
`iss`, missing `aud`, missing or non-numeric `iat`, future `iat` beyond the
existing skew, or present future `nbf`. Correctly bound tokens continue to pass.
The existing workload path remains on generic verification. The tenant refusal
path proves rejection happens before any User, roles, refresh row, completion,
or session is committed.

### AC-R4-02 — `FIND-TASK-002-12`

All refresh operations for the same stored tenant principal family serialize
before active/stale classification and retain that serialization through the
route transaction's terminal commit. When legitimate rotation of current token
`B` overlaps replay of ancestor `A`, the containment result is durable: any
successor `C` is revoked, cannot rotate from a fresh transaction, and exactly
one canonical containment audit identifies the replayed credential.

### AC-R4-03 — preserved boundaries

Same-row single-winner rotation, connection replacement cutoff, refresh
provenance, replay audit durability, workload federation, platform OIDC, tenant
RLS, stable errors, and the four TASK-002 identity journeys retain their
accepted behavior. No public contract or generated artifact changes.

## Focused proof and broader verification

Add and run these exact focused tests (the names are part of this task):

```bash
mise exec -- cargo nextest run --locked -p wyrd-auth-verify --lib \
  -E 'test(=tests::oidc_id_token_requires_binding_and_time_claims)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E 'test(=refresh::pg_tests::ancestor_replay_overlapping_rotation_revokes_successor)'"

mise exec -- env WYRD_IDENTITY_TARGET=server \
  WYRD_IDENTITY_FILTER=tenant_callback_refusal_journey \
  mise run test:identity:journey
```

The verifier proof covers missing `iss`/`aud`, missing/non-numeric/future `iat`,
future `nbf`, a valid ID token, and an unchanged valid workload assertion. The
Postgres proof controls the ancestor/current-row overlap and checks state from a
fresh transaction after both route-equivalent commits. The existing refusal
journey adds at least a validly signed missing-`iat` token and proves no login
effects are persisted.

Then run the owning lanes:

```bash
mise run test:principals:unit
mise run test:principals:integration
mise run test:sql
mise run check:tenant-isolation
mise run test:identity:journey
mise run fmt
mise run lints
git diff --check
```

If the implementation changes a generated contract despite the non-goal, stop:
that is scope drift rather than a reason to regenerate it.

## Implementation evidence

Commits: `aef4ef3e3` (ID-token claims), `117231745` (refresh-family
serialization).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-R4-01 | `crates/shared/wyrd-auth-verify/src/lib.rs`: `verify_external_against` requires `exp`/`iss`/`aud` and validates a present `nbf`; new `ExternalVerifier::verify_id_token_against` requires numeric `iat` ≤ now + skew. `callback.rs` and `platform_login.rs` route through it; `jwt_bearer.rs` stays on `verify_external`. | `tests::oidc_id_token_requires_binding_and_time_claims` (missing `iss`/`aud`/`iat`, string/future `iat`, future `nbf`, valid ID token, past `nbf`, workload without `iat` passes, workload future `nbf` refused); `tenant_callback_refusal_journey` adds a validly signed missing-`iat` token and asserts tenant C has no `auth_users`, `auth_user_identities`, `auth_user_roles`, or `auth_refresh_tokens` row, plus no completion | PASS |
| AC-R4-02 | `wyrd-sql` `lock_refresh_family` (transaction-scoped advisory lock on tenant + principal kind + id); `RefreshTokens::execute` resolves the row, locks the family, then consumes or contains under the lock; issuance then takes the connection slot lock (family → connection). | `refresh::pg_tests::ancestor_replay_overlapping_rotation_revokes_successor` holds rotation of B open until the replay of A is observed waiting in `pg_locks`, commits both, then from a fresh transaction asserts C is `reuse_detected`, C rotation is `Reused`, and exactly one containment audit names A. It failed before the fix (C unrevoked). | PASS |
| AC-R4-03 | No public contract, route, schema, or generated artifact changed; existing refresh, connection, and journey tests unchanged. | `test:principals:unit`, `test:principals:integration`, `test:sql`, `check:tenant-isolation`, `test:identity:journey` (27/27) | PASS |

Commands (all exit 0 in this session): the three focused commands above,
`mise run test:principals:unit`, `mise run test:principals:integration`,
`mise run test:sql`, `mise run check:tenant-isolation`,
`mise run test:identity:journey`, `mise run fmt`, `mise run lints`,
`git diff --check`.

Non-goals held: no new JWT library, verifier, claim DTO, skew setting, public
API, table, trait, lease, retry, isolation change, or process-local lock.

Limit: the refusal journey carries only the missing-`iat` claim case
end to end. Adding missing-`iss` and future-`iat` there pushed the journey past
the auth route's per-peer governor burst (20), and a later callback got
`429`. The verifier unit test covers those cases on the shared owner that both
OIDC paths use.
