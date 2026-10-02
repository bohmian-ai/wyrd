# TASK-009 round-3 findings validation

## Immutable subject

- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `04597909203463820b2033c12956f5fe6fcfe1f4`
- Candidate tree: `0905c9a3bf16004c9c1d4d1405c2c11a95e6619a`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Current remediation: `changes/active/oidc-production-readiness/review/TASK-009-r2/TASK-009-R2-relying-party-corrections.md`

The immutable commits, applicable authorities, cumulative and remediation
diffs, both prior ledgers and verdicts, and every required round-three report
were available. `.codegraph/` is absent, so validation used commit-qualified
Git source and direct caller searches. The candidate object and tree remained
unchanged.

`FIND-TASK-009-5` and `FIND-TASK-009-14` are **WITHDRAWN — MUST NOT
REOPEN**. This validation does not reassign either defect, require an
equivalent mechanism, or include either in the ledger.

## Proposal validation

### `MNT-R3-001` — CONFIRMED

Producer-to-consumer trace:

- `PlatformLogin::new` constructs and owns the platform `RelyingParty` and its
  process cache (`crates/wyrd/wyrd-auth/src/platform_login.rs:94-128`).
- `PlatformLogin::{begin,complete}` use that private field directly
  (`platform_login.rs:171-181,261-318`).
- The remediation added the public `PlatformLogin::relying_party` accessor at
  `platform_login.rs:131-141`. Candidate-wide caller search finds exactly one
  caller, platform connection configuration at
  `crates/wyrd/wyrd-server/src/components/platform/identity.rs:235-244`, and
  that caller immediately invokes `RelyingParty::discover` only to obtain the
  advertised JWKS URL.
- The sibling tenant accessor is not equivalent public precedent:
  `HumanConnections::relying_party` is `pub(crate)` and serves the separate
  tenant login and callback owners inside `wyrd-auth`.

The accessor is reachable and was introduced by the round-two correction. It
exposes the entire dependency to satisfy one platform operation, contrary to
the required struct-centered style in `AGENTS.md` section 5 and
`architecture/agent-rules.md`. Deletion alone would regress
`FIND-TASK-009-13`; an owner method is the first Ponytail rung that preserves
the required fresh discovery and cache replacement.

**Validation:** `CONFIRMED` as `DRIFT`. Retain as `FIND-TASK-009-15`.

### `MNT-R3-002` — CONFIRMED

Producer-to-consumer trace:

- `configure_connection` now performs full `openidconnect` discovery through
  `RelyingParty::discover`, which fetches both provider metadata and the
  advertised JWKS before persistence
  (`components/platform/identity.rs:196-244` and
  `crates/shared/wyrd-auth-oidc/src/relying_party.rs:397-435`).
- `discovery_error` maps blocked-address refusals to a catalogued `400`, but
  maps resolution/client failure, unreachable or undecodable discovery/JWKS,
  and issuer mismatch to `WyrdError::DiscoveryUnavailable`
  (`components/admin/routes.rs:765-805`). The existing catalog fixes that
  error at HTTP `503` under `WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`.
- The changed route advertises only `200/400/401/403/500`, describes an
  unreachable issuer as `400`, and says only that discovery occurs
  (`components/platform/identity.rs:173-192,221-243`). The adjacent established
  trusted-issuer route declares the same catalogued discovery failure as
  `503` (`components/admin/routes.rs:247-297`).
- The added served failure loop reaches both unavailable and undecodable JWKS
  cases but asserts only non-success
  (`crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:1607-1648`). The
  existing served OpenAPI target validates route declarations but does not pin
  this operation's missing `503` response.

This is a reachable public declaration mismatch introduced at the round-two
full-discovery seam. The standard/native correction is to declare the existing
catalogued response accurately; no new error, document, generator, or check is
needed.

**Validation:** `CONFIRMED` as `INCORRECT`. Retain as
`FIND-TASK-009-16`.

### `PERSIST-TEN-R3-001` — REJECTED

The producer/consumer trace confirms that platform and tenant human
`jwks_ttl_secs` values do not control the candidate's fixed five-minute
`RelyingParty` cache. It also confirms that this was already the base behavior:
the base `ExternalVerifier` used one boot-built 300-second `JwksCache`, and its
human verification calls did not consume the stored per-connection TTL. The
base already accepted, persisted, and projected both human TTL surfaces.

The cumulative diff changes adjacent audience and relying-party code but does
not introduce the TTL setting, make it newly inert, or remove an effective TTL
behavior. Deleting the platform request/view field and the tenant view,
inheritance, rows, queries, and columns would expand TASK-009 into an unrelated
public-contract and persistent-schema removal. It would also contradict the
task's explicit outcome that tenant/operator-visible behavior does not change.
That correction is not needed to satisfy the approved task.

The proposal also does not satisfy the standing conventionality premise used
to classify it as drift: configurable JWKS key-cache lifetimes exist in a
widely used comparable implementation (Keycloak documents
[`public-key-cache-ttl` and unknown-key refresh](https://www.keycloak.org/docs/24.0.5/securing_apps/)
in its official adapter configuration). OIDC does not require the option, but
the option is not a Wyrd-only mechanism merely because `openidconnect` itself
supplies no cache.
Whether Wyrd should repair or remove its pre-existing inert setting is a
separate product/schema decision, not this acceptance audit.

**Validation:** `REJECTED`. Assign no `FIND-*` ID and require no remediation.

## Validated finding ledger

### `FIND-TASK-009-15` — public dependency escape hatch bypasses the platform login owner

- **Discovery IDs:** `MNT-R3-001`
- **Status:** `CONFIRMED`
- **Classification:** `DRIFT`
- **Violated obligation:** stateful workflows using an owner's dependency are
  inherent methods on that concrete owner; the task requires platform setup to
  reuse the boot-owned platform relying party, not expose that dependency.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/platform_login.rs:131-141` and
  `crates/wyrd/wyrd-server/src/components/platform/identity.rs:235-244`.
- **Evidence and reachability:** the public accessor has one production caller,
  which immediately calls fresh discovery; begin and complete already use the
  same private dependency through `PlatformLogin`. The accessor therefore
  broadens the public owner boundary without a second consumer or operation.
- **Observable consequence:** callers can bypass `PlatformLogin` and invoke
  lower-level relying-party operations, while the platform setup workflow is
  split across the owner and its server consumer.
- **Decision-complete correction:** delete `PlatformLogin::relying_party` and
  place the required fresh-discovery/JWKS-URI operation on `PlatformLogin`,
  delegating to its existing `RelyingParty::discover` and existing cache. Have
  the configure handler call that operation. Add no trait, wrapper, second
  cache, invalidation service, option, or new protocol behavior.
- **Focused closure proof:** repository search finds no public dependency
  accessor; the existing served platform configuration/login test still proves
  unavailable or undecodable JWKS preserves the row and same-issuer
  reconfiguration refreshes the provider used by begin/callback. Run the exact
  changed test and the narrowest `wyrd-auth`/server lanes covering the write
  set.

### `FIND-TASK-009-16` — platform configuration omits its reachable discovery `503` contract

- **Discovery IDs:** `MNT-R3-002`
- **Status:** `CONFIRMED`
- **Classification:** `INCORRECT`
- **Violated obligation:** materially changed public Rust documentation and
  generated HTTP declarations must match reachable behavior and the stable
  Wyrd error catalog.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/platform/identity.rs:173-192,221-243`
  and `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:1607-1648`.
- **Evidence and reachability:** full provider/JWKS discovery can reach
  `DiscoveryUnavailable`, whose existing status/code are
  `503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`; the route declares no `503`,
  calls an unreachable issuer `400`, and its new served failure proof accepts
  any non-success response.
- **Observable consequence:** the served OpenAPI contract and route rustdoc
  tell clients and operators that a real retryable provider/JWKS outage is
  absent or a caller validation failure while runtime returns `503`.
- **Decision-complete correction:** update the existing rustdoc/comment and
  `utoipa` response list to describe full metadata-plus-JWKS discovery and the
  existing `503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`; retain blocked-address
  and malformed-input cases at `400`. Add no error or documentation mechanism.
- **Focused closure proof:** tighten the existing unavailable/undecodable JWKS
  cases to assert the existing `503` status and stable code, and extend the
  existing served OpenAPI contract test to assert that the PUT operation
  advertises that response. Run those exact tests and their narrowest owning
  server/principals lane; no broad journey or every-language sweep is required
  at task review.

## Prior-finding closure

| Finding | Candidate evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | One boot-owned `PlatformLogin` remains shared by configure, begin, and callback. | CLOSED |
| `FIND-TASK-009-2` | Platform callback carries and verifies RFC 9207 `iss` before redemption. | CLOSED |
| `FIND-TASK-009-3` | Workload setup remains metadata-only; workload JWKS retrieval remains with `ExternalVerifier`. | CLOSED |
| `FIND-TASK-009-4` | Moka `try_get_with` coalesces overlapping misses and the unknown-key path re-enters it once. | CLOSED |
| `FIND-TASK-009-5` | Binding lead direction requires no correction. | **WITHDRAWN — MUST NOT REOPEN** |
| `FIND-TASK-009-6` | Human audience is derived from client ID and no independent public input remains. | CLOSED |
| `FIND-TASK-009-7` | The unused `RelyingParty::http` accessor remains absent. | CLOSED |
| `FIND-TASK-009-8` | Library state, nonce, and S256 PKCE generators remain in use. | CLOSED |
| `FIND-TASK-009-9` | The changed key-set helpers retain accurate adjacent rustdoc. | CLOSED |
| `FIND-TASK-009-10` | Exact zero-selection-safe commands are recorded for every named remediation test. | CLOSED |
| `FIND-TASK-009-11` | The trust-all additional-audience override is absent and refusal proof is recorded. | CLOSED |
| `FIND-TASK-009-12` | Human `ExternalVerifier` prose and dead callback fixture wiring are absent; the workload consumer remains. | CLOSED |
| `FIND-TASK-009-13` | Platform configuration performs full discovery through the process-owned cache and the served test proves refusal/replacement behavior. `FIND-TASK-009-15` and `-16` are independent owner/declaration defects left by that correction. | FUNCTIONALLY CLOSED |
| `FIND-TASK-009-14` | Binding lead direction requires the direct unreleased-column removal and no overlap mechanism. | **WITHDRAWN — MUST NOT REOPEN** |

## Verification assessment

The remediation record contains exact focused tests for the changed
relying-party, callback, workload discovery, platform configuration/cache, and
tenant refusal paths, plus the narrow owner, contract, formatting, lint, and
boundary lanes covering its write set. Under human direction `518026d54`, the
absence of a fresh full user-journey or every-language sweep is not a task
review gap; those run once at change review.

The two retained findings need only existing focused mechanisms: the served
platform test and served OpenAPI contract test, plus the narrowest owner lanes
for their small Rust write set. No new test harness, permanent check, broad
aggregate, option, compatibility layer, or nonstandard mechanism is required.

## Validation result

- **Validated ledger:** two new findings, `FIND-TASK-009-15` and
  `FIND-TASK-009-16`.
- **Rejected proposal:** `PERSIST-TEN-R3-001`; it is pre-existing,
  task-unrelated public/schema debt and its proposed deletion is not required
  for TASK-009 acceptance.
- **`SPEC_REVISION_REQUIRED`:** no. Both retained corrections use existing
  owners, errors, route declarations, and tests without a material new
  decision.
- **`BLOCKED`:** no. Every required report, authority, commit-qualified source,
  caller trace, and follow-up resolution was available.

The candidate requires bounded remediation for `FIND-TASK-009-15` and
`FIND-TASK-009-16`.
