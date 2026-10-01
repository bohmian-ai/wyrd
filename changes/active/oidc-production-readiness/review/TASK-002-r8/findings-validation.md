# TASK-002 R8 Structured Ponytail Findings Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `ced8acaabce7dbe1682bef46d65d073b07e0dbd9`
- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation authority: TASK-002 R1 through R7, their prior verdicts and
  validations, and stable findings `FIND-TASK-002-1` through
  `FIND-TASK-002-15`

`HEAD` equaled the candidate before and after source inspection. The untracked
R8 review directory is outside the immutable committed subject. Lead-directed
reuse and test commits recorded in the task evidence were treated as authorized
rather than scope drift and were still inspected for regression.

## Review-input completeness and method

The approved specification, original task, R1-R7 remediation and finding
history, repository rules, applicable security and OpenID Connect authority,
cumulative changed-file inventory and diff, current source and callers, and all
four R8 Wave 1 reports were inspected. The repository has no `.codegraph/`
directory, so caller tracing used Git, `rg`, and direct full-body inspection.

The Wave 1 union contains `REPO-R8-1` and `SEC-R8-001`. Validation traced the
only production caller of `print_tokens`; both production callers and every
test call of `ExternalVerifier::verify_id_token_against`; the generic
`verify_external_against` and `map_claims` owners; and the complete tenant
callback and platform-login bodies that consume the returned subject.

## Wave 1 disposition

| Wave 1 report | Proposed ledger | Validation disposition |
|---|---|---|
| `task-review.md` | Empty | **EMPTY LEDGER OVERTURNED.** Its task matrix correctly validates the other obligations, but its full-claims PASS misses the reachable ID-token subject-format defect retained as `FIND-TASK-002-17`. |
| `standards-review.md` | `REPO-R8-1` | **CONFIRMED** as `FIND-TASK-002-16`. |
| `domain-review-security.md` | `SEC-R8-001` | **REVISED** as `FIND-TASK-002-17`: the OIDC syntax defect and owner are correct, but duplicate provider subjects—not invalid syntax itself—cause account collapse, and a second callback test is unnecessary closure proof. |
| `domain-review-tenancy-data.md` | Empty | **EMPTY LEDGER VALIDATED.** No separate tenancy, persistence, concurrency, durability, or migration finding remains; the R7 family-lock correction closes `FIND-TASK-002-15`. |

## Validation of `REPO-R8-1`

### Reachability and rule

`print_tokens` has one production caller, `refresh::dispatch`. After the live
refresh exchange returns, `dispatch` passes the issued response to this helper,
which prints the access token, optional refresh token, and expiry. The full
helper body performs no argument parsing. Its first rustdoc sentence,
“Argument parsing for `wyrd auth refresh`,” was moved with neither the parser
tests nor their subject and now falsely describes the helper.

`AGENTS.md` section 16 requires accurate, substantive rustdoc for every
materially modified Rust item and makes placeholder documentation a hard
blocker. The following token-output sentences already document the helper's
intent, workflow role, and side effect, so adding another explanation would be
redundant.

### Ponytail correction boundary

Delete only the stale first rustdoc sentence. Reuse the accurate rustdoc that
already follows it. Do not rename, move, expose, split, or behaviorally test
the private helper. Direct source inspection plus format, lint, and diff-hygiene
checks are sufficient for this documentation-only correction.

## Validation of `SEC-R8-001`

### Reachability and complete caller trace

`ExternalVerifier::verify_id_token_against` first calls
`verify_external_against`. That shared generic owner verifies the signature,
key, issuer, audience, expiry, and present `nbf`, then calls `map_claims`.
`map_claims` accepts any JSON string at the configured subject path, including
an empty, non-ASCII, or over-255-byte value. The ID-token-specific method then
checks only that `iat` is a non-negative integer no later than the allowed
clock bound and returns the claims unchanged.

There are two production callers:

1. `AuthorizationCodeExchange::finish_id_token_exchange` verifies the token,
   nonce, authorized party, and bound connection, then opens a tenant
   transaction and passes `verified.subject` to `ensure_user_identity`. That
   function uses `(issuer, subject)` as the durable lookup/upsert key before
   roles, audit, access/refresh issuance, sealed completion, and commit.
2. `PlatformLogin::complete` consumes platform login state, exchanges the
   code, calls the same ID-token verifier, verifies nonce, and passes
   `claims.subject` to the federated platform-session owner as the pinned
   external identity.

The remaining calls are in the focused
`oidc_id_token_requires_binding_and_time_claims` test. Workload verification
does not call the ID-token method: it deliberately remains on
`verify_external_against`, whose configurable subject mapping is a separate
contract. Moving the restriction into `map_claims` or the generic verifier
would therefore widen this remediation into workload identity semantics.

[OpenID Connect Core 1.0 section 2](https://openid.net/specs/openid-connect-core-1_0-final.html#IDToken)
defines `sub` as a locally unique, never-reassigned identifier and requires it
not to exceed 255 ASCII characters. REQ-007 requires verified ID-token claims,
the security posture requires invalid claims to fail authentication, and both
OIDC callers rely on the returned subject as a durable identity key. The
current path instead admits an ID token whose `sub` is not a valid OIDC Subject
Identifier.

Wave 1's account-collapse consequence is overstated. A trusted issuer that
reuses any subject value, including a syntactically valid one, violates the
issuer-side uniqueness contract and can collapse identities; Wyrd cannot infer
which provider accounts a signed subject represents. The independently proven
consequence here is narrower: Wyrd accepts and persists or pins a
non-conforming OIDC identity instead of failing closed at its ID-token trust
boundary.

### Ponytail correction boundary

Deletion does not preserve full ID-token verification. No new type, helper,
dependency, persistence object, public contract, or callback-specific guard is
needed. Reuse the existing `verify_id_token_against` owner and Rust string
operations: require raw `sub` to be a nonempty string containing only ASCII and
at most 255 bytes before returning the verified identity. Keep the generic
workload path unchanged, update this method's rustdoc/error contract, and
preserve all downstream nonce, authorized-party, connection, transaction,
audit, and issuance behavior.

The smallest credible closure proof extends the existing focused shared
verifier test to reject missing, non-string, empty, non-ASCII, and 256-byte
`sub` values while accepting a normal subject. A callback/Postgres test would
only repeat the same branch: both production callers return immediately on the
shared verifier error, before either downstream identity/session owner is
invoked. The existing broader identity journey remains the regression proof
for the normal callback workflow.

## Prior-finding closure

| Prior finding | Independent current-candidate result |
|---|---|
| `FIND-TASK-002-1` — exact human `sub` identity | **CLOSED.** Human connection authoring and stored decode require the literal `sub` path; the new finding concerns validation of that claim's value. |
| `FIND-TASK-002-2` — OIDC authorized party | **CLOSED.** Tenant callback enforces configured-client `azp`, including the multi-audience case. |
| `FIND-TASK-002-3` — provider role-change audit | **CLOSED.** Changed mapped roles append canonical evidence transactionally; unchanged roles do not. |
| `FIND-TASK-002-4` — provider-advertised algorithm | **CLOSED.** Fresh advertised membership precedes shared asymmetric signature and key verification. |
| `FIND-TASK-002-5` — duplicate tenant selection | **CLOSED.** Login-state transitions rely on forced RLS. |
| `FIND-TASK-002-6` — raw-pool state lookup | **CLOSED.** The least-disclosure lookup remains an inherent `WyrdPostgres` capability. |
| `FIND-TASK-002-7` — printable PKCE verifier | **CLOSED.** Durable PKCE state remains secret-backed and redacted. |
| `FIND-TASK-002-8` — incomplete Rust documentation | **CLOSED for its R2 inventory.** `FIND-TASK-002-16` is a later, distinct stale sentence introduced when the CLI token printer moved. |
| `FIND-TASK-002-9` — hidden imports | **CLOSED.** The cited imports remain module-scoped. |
| `FIND-TASK-002-10` — false algorithm-helper contract | **CLOSED.** The helper accurately documents advertised-set membership and the shared verifier's enforcement role. |
| `FIND-TASK-002-11` — optional OIDC binding/time claims | **CLOSED for its diagnosed claims.** OIDC callers require issuer, audience, expiry, and numeric non-future issuance time; `FIND-TASK-002-17` is the distinct Subject Identifier syntax gap. |
| `FIND-TASK-002-12` — replay/rotation race | **CLOSED.** Family locking precedes lifecycle classification and remains held through commit. |
| `FIND-TASK-002-13` — stale refresh lookup rustdoc | **CLOSED.** Its current documentation matches lookup, lock, classification, and test-observation behavior. |
| `FIND-TASK-002-14` — revocation versus refresh successor | **CLOSED.** Administrative User revocation shares the family lock with rotation and contains a committed successor. |
| `FIND-TASK-002-15` — revocation versus first issuance | **CLOSED.** The sole human-session insertion owner takes the family lock before connection and principal-status checks; deterministic proof covers both orderings. |

## Final deduplicated finding ledger

### FIND-TASK-002-16 — Refresh token-output helper carries false argument-parsing rustdoc

- **Wave 1 source ID:** `REPO-R8-1`
- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Violated obligation:** Every materially modified Rust item must have
  accurate rustdoc explaining its intent, workflow role, and operation.
- **Exact location:**
  `crates/wyrd/wyrd-cli/src/auth/refresh.rs:54-59`; sole caller at
  `crates/wyrd/wyrd-cli/src/auth/refresh.rs:34-51`.
- **Evidence:** `print_tokens` only prints a returned token pair and expiry,
  while its first rustdoc sentence says it parses `wyrd auth refresh`
  arguments. The accurate token-output documentation already follows it.
- **Observable consequence:** The candidate violates the repository's hard
  documentation contract and misstates where CLI argument parsing occurs.
- **Decision-complete correction:** Delete the stale “Argument parsing for
  `wyrd auth refresh`” sentence and preserve the existing accurate rustdoc and
  function body. Add no helper, move, visibility change, or behavioral test.
- **Focused closure proof:** Inspect the corrected item, then run
  `mise run fmt`, `mise run lints`, and
  `git diff --check 3fc085acf5b3a710d5dc80892bd2e664b3db6174..<new-candidate>`.
- **Specification decision:** No revision is required.

### FIND-TASK-002-17 — OIDC ID-token verification accepts an invalid Subject Identifier

- **Wave 1 source ID:** `SEC-R8-001`
- **Status:** REVISED
- **Classification:** INCORRECT
- **Violated obligation:** REQ-007 requires full ID-token claim verification;
  REQ-008 and INV-002 use verified `(issuer, subject)` as the stable human
  identity; the security posture requires invalid claims to fail
  authentication.
- **Exact location:**
  `crates/shared/wyrd-auth-verify/src/lib.rs:558-601`; permissive extraction at
  `crates/shared/wyrd-auth-oidc/src/claims.rs:38-62`; tenant consumer at
  `crates/wyrd/wyrd-auth/src/callback.rs:201-267`; platform consumer at
  `crates/wyrd/wyrd-auth/src/platform_login.rs:242-312`.
- **Evidence:** The generic mapper accepts any string at the subject path, and
  the ID-token method adds only `iat` validation. Both production callers use
  the returned value as the external identity. Missing and non-string subjects
  already fail through mapping, but empty, non-ASCII, and over-255-byte `sub`
  values pass despite the OpenID Connect Subject Identifier contract.
- **Observable consequence:** A signed ID token with a non-conforming `sub`
  can establish a durable tenant User identity or pin a platform identity
  instead of failing closed.
- **Decision-complete correction:** In the existing
  `ExternalVerifier::verify_id_token_against` owner, validate the raw `sub` as
  a nonempty ASCII string of at most 255 bytes before returning claims. Keep
  `verify_external_against`, `map_claims`, and workload assertion semantics
  unchanged; update the method's rustdoc/error contract. Add no new type,
  helper, dependency, persistence rule, or downstream guard.
- **Focused closure proof:** Extend
  `oidc_id_token_requires_binding_and_time_claims` to reject missing,
  non-string, empty, non-ASCII, and 256-byte `sub` values and accept a normal
  subject. Run its exact `mise exec -- cargo nextest run --locked -p
  wyrd-auth-verify --lib -E
  'test(=tests::oidc_id_token_requires_binding_and_time_claims)'` selector,
  then `mise run test:principals:unit`, `mise run test:identity:journey`,
  `mise run fmt`, `mise run lints`, and diff hygiene.
- **Specification decision:** No revision is required. The approved full-claim
  and fail-closed behavior plus the existing OIDC-specific verifier determine
  the correction.

No other finding is retained.

## Verification limits

- Static validation covered the two proposed findings, their complete
  correction-owner bodies and callers, the cumulative changed-file inventory,
  R1-R7 finding history, and the task, repository, security, and OpenID Connect
  authority needed to resolve them.
- `git diff --check
  3fc085acf5b3a710d5dc80892bd2e664b3db6174..ced8acaabce7dbe1682bef46d65d073b07e0dbd9`
  passed, and `HEAD` remained the immutable candidate.
- This bounded Wave 2 review did not rerun Cargo, Postgres, Docker,
  Keycloak/Dex, migrations, codegen, docs, lints, or broad identity lanes.
  Recorded candidate evidence covers the focused R7 concurrency proof,
  principals unit/integration, SQL, all 27 identity journeys, tenant isolation,
  format, lints, codegen, docs, and diff hygiene.
- `cargo nextest list` confirmed the exact focused verifier selector without
  executing the test.
- No current test exercises ID-token Subject Identifier syntax. Existing tests
  prove claim presence/path selection and other binding/time claims, not the
  admitted empty, non-ASCII, or overlong values.
- TASK-003 BFF redemption, TASK-004 CLI persistence, and live Okta/Entra
  qualification remain intentional downstream or change-level work.

## Overall validation recommendation

**VALIDATED WITH FINDINGS — FIX_REQUIRED.** Retain
`FIND-TASK-002-16` and `FIND-TASK-002-17`. Both corrections are bounded to
existing owners: delete one false rustdoc sentence and add one OIDC-specific
stdlib validation at the shared verifier with one focused test. No
specification revision is required.
