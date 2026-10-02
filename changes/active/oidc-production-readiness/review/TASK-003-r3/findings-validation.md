# TASK-003 R3 structured finding validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `8289fa298ed33d21f2568558bc0a02905fd0b218`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Prior validation and remediation: `changes/active/oidc-production-readiness/review/TASK-003-r1/` and `TASK-003-r2/`
- Human directions: `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and `TASK-003-r2/human-direction-connection-test.md`

The candidate resolved to the stated object before validation. The repository
has no `.codegraph/` directory, so source navigation used the immutable Git
range and repository-native search. Per assignment, this pass ran no build,
test, provider, browser, or Postgres command.

## Inputs and coverage

This validation read the complete cumulative diff, the latest remediation
locator diff, the applicable repository and architecture authorities, the
original task, R2 remediation, both human directions, prior R1/R2 validation
and verdicts, and every required R3 discovery and follow-up report.

The runtime trace covered these complete owners and reachable paths:

- `BrowserSessions::{read,authority,current,renew,logout}` and the private BFF
  read/authority handlers and `ServerSessions` consumers;
- `RefreshTokens::execute`, its family lock, active-row consume, family
  revocation, canonical audit append, and the ordinary refresh HTTP route;
- `ExchangeApiKey::execute`, fixed-cost verification, `last_used_at` write, and
  `TenantTokenIssuer` issuance/audit outcomes;
- browser-row lock, rotation, revocation, and exact-expiry SQL; and
- the changed shared API-key test module, every newly crate-visible helper, and
  its browser-session test consumer.

## Proposal validation

### BEH-R3-001 — **REVISED**

The reported replay path is reachable and task-scoped. A browser session keeps
the refresh JWT issued at login or the last successful browser renewal. If the
same JWT is first rotated through the ordinary `/auth/token` path, the browser
later presents a stored-but-stale row. `RefreshTokens::execute` locks the
principal family, revokes every active successor, appends the canonical denied
containment audit, and returns `RefreshError::Reused`
(`refresh.rs:121-173`). The ordinary refresh route commits that precise error
before returning its refusal (`routes.rs:224-263`).

`BrowserSessions::renew` instead reduces `Reused` to `Renewal::Refused`
(`browser_sessions.rs:555-571`). During the proactive window,
`BrowserSessions::current` rolls the transaction back and serves the old
access token (`browser_sessions.rs:468-508`), erasing the family revocation and
audit and leaving the attacker-held successor renewable. Both production
consumers, `read` and `authority`, reach `current`; the private handlers add no
other containment.

The proposal is revised only at its correction boundary. Replay containment
must commit, but the browser row must remain live while its already-issued
access token is unexpired. Revision-7 REQ-016, the task's packet-local contract,
and R2-AC-04 preserve that snapshot authority until exact expiry. Immediate
browser-row revocation would wipe the only BFF-held copy and reintroduce the
early-cutoff behavior corrected by prior `FIND-TASK-003-10`.

### PC-R3-001 — **REVISED**

The report correctly identifies the same lossy result classification and the
additional internal-failure branch, but its proposed immediate browser-row
revocation is rejected for the exact-expiry reason above. Its statement that
all audit failures can commit unaudited tentative writes is also too broad: a
Postgres error from the canonical audit append aborts that transaction, so the
later browser revocation or commit cannot succeed.

The retained portion is source-proven. Refresh issuance can fail after
`consume_active_refresh`; API-key issuance can fail after
`touch_api_key_last_used`. `BrowserSessions::renew` classifies only direct
database variants as `Renewal::Failed` and maps every other issuance, store,
signing, role-decode, envelope, and verification-task failure to
`Renewal::Refused` (`browser_sessions.rs:547-585`). Before access expiry this
rolls back but incorrectly serves authority after an infrastructure failure.
After expiry, failures that leave a valid transaction—such as signing,
role/corrupt-state, or verification-task failures—can enter the terminal
refusal branch, revoke the browser row, and commit preceding refresh/key-use
state despite no successful issuance. This violates R2's explicit requirement
that infrastructure failures remain fail closed.

### SYSTEM-R3-001 — **REVISED**

The deployed consequence and shared cause are confirmed, with the same
transaction-abort narrowing. A transient non-semantic failure must fail the
request and leave the browser session and recoverable credential retryable; it
must not be converted into either successful pre-expiry authority or a durable
post-expiry credential decision. A failed SQL audit append already prevents a
destructive commit, but it is still misclassified before expiry: the code rolls
back and then serves the old token. Non-aborting internal failures retain the
destructive post-expiry path.

This is not a separate finding from BEH-R3-001/PC-R3-001. All three symptoms
come from the same information loss in `BrowserSessions::renew` and require one
correction in the existing `BrowserSessions` orchestration owner.

### STD-R3-001 — **CONFIRMED**

`exchange_api_key::pg_tests` became `pub(crate)` so the sibling
`browser_sessions::pg_tests` module can reuse its fixtures, but the materially
modified module has no module rustdoc (`exchange_api_key.rs:540-541`). This is
an explicit hard blocker under `AGENTS.md` section 16 and
`architecture/agent-rules.md`, which include modules and test helpers
regardless of visibility.

`insert_live_api_key` also became crate-visible, but its attached rustdoc is
the stale contract for a machine credential-exchange test. It claims fixture
startup and assertion panic conditions, then appends the helper's actual
description after that `# Panics` text (`exchange_api_key.rs:866-904`). The
function actually generates, hashes, and inserts one API key and can panic on
hashing or insertion. The existing prose therefore does not accurately
document the shared helper's intent or panic boundary.

The smallest correction is documentation only: add concise module rustdoc for
the shared test-support role and replace the helper's stale block with one
cohesive contract plus accurate `# Panics`. No wrapper, fixture module,
abstraction, or runtime test is justified.

## Ponytail trace and correction selection

The runtime proposal cannot be deleted: proactive renewal is required by the
browser-session contract, and replay containment is an existing security
invariant. No new owner, persistent marker, audit path, refresh implementation,
dependency, public contract, or test harness is needed. Existing repository
behavior already supplies every necessary mechanism:

- `RefreshTokens::execute` owns replay classification, family revocation, and
  its audit;
- the ordinary refresh route establishes that `RefreshError::Reused` is the
  one error whose containment transaction commits;
- `BrowserSessions` owns the browser-row lock, exact-expiry verdict, and the
  existing rollback/relock loop; and
- the current `Renewal` result is the narrow private seam that lost the
  distinction.

The minimum safe correction is therefore:

1. Preserve a distinct replay-contained renewal result for
   `RefreshError::Reused`.
2. Treat only credential/lifecycle outcomes as ordinary refusal:
   `RefreshError::NotFound`; issuance `TenantNotAdmitting`,
   `PrincipalInactive`, and `ConnectionInactive`; and API-key malformed,
   missing, cross-tenant, wrong-secret, inactive-principal, or
   non-admitting-tenant outcomes.
3. Propagate database/store, audit, signing, role/corrupt-state, envelope-open,
   verification-task, and other internal outcomes as failures. Roll back and
   return the failure both before and after access expiry; do not serve the old
   token and do not revoke the browser row.
4. On refresh replay before access expiry, commit the existing family
   revocation and audit, reopen and lock the unchanged browser row, and serve
   only its current access token. At or after expiry, revoke the browser row in
   the same containment transaction and refuse. Do not duplicate family
   revocation or audit in `BrowserSessions`.
5. Keep the existing rollback/relock behavior for ordinary early refusal and
   the existing terminal browser-session behavior for ordinary refusal after
   expiry.

The schema has no non-renewable-live-session marker. Adding one solely to
deduplicate repeated pre-expiry presentations would introduce an unapproved
persistent-state decision and is unnecessary for closure. Repeated requests
remain bounded by the current access-token lifetime; the existing refresh
owner may record another zero-row containment event.

## Final deduplicated finding ledger

### FIND-TASK-003-14 — Browser renewal loses security outcome semantics

- **Discovery sources:** `BEH-R3-001`, `PC-R3-001`, `SYSTEM-R3-001`; resolved
  conflict evidence in `followup-review.md`
- **Status:** **REVISED**
- **Classification:** `REGRESSION`
- **Violated obligation:** REQ-007, REQ-009, REQ-016, REQ-017, AC-007; the
  security-posture refresh-reuse containment rule; R2-AC-04's exact-expiry
  preservation; and its explicit requirement that infrastructure failures
  remain fail closed.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/browser_sessions.rs:435-585`; producers at
  `crates/wyrd/wyrd-auth/src/refresh.rs:115-226`,
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:154-185`, and
  `crates/wyrd/wyrd-auth/src/issuance.rs:369-497`; ordinary replay precedent
  at `crates/wyrd/wyrd-server/src/components/auth/routes.rs:224-263`.
- **Evidence:** `renew` collapses replay containment, semantic credential
  refusal, and most internal failures into `Renewal::Refused`. `current` can
  then only roll back and serve, or revoke and commit. The former erases
  pre-expiry refresh-family containment and serves authority after internal
  failure; the latter can turn a valid-transaction internal failure into
  durable browser revocation and commit tentative credential writes.
- **Observable consequence:** detected refresh theft can leave the attacker's
  successor renewable with no durable containment audit until access expiry.
  A transient signing, corrupt-state, verification-task, or similar internal
  failure can either return authority during the proactive window or
  permanently end a browser session at expiry instead of remaining retryable.
- **Decision-complete correction:** apply the five-step correction above in
  `BrowserSessions`, reusing `RefreshTokens` containment/audit and the existing
  lock and expiry authority. Preserve current access until exact expiry after
  successful replay containment; propagate and roll back all internal
  failures; retain ordinary refusal behavior for genuine credential and
  lifecycle outcomes.
- **Focused closure proof:** in the existing Postgres-backed auth tests, create
  an OIDC browser session whose stored refresh was already rotated. During the
  proactive window, assert the same access token remains usable while the
  complete refresh family and at least the triggering canonical containment
  audit commit; after access expiry, assert the browser row ends. Separately
  force one refresh issuance/audit failure and one API-key internal failure and
  assert each request fails, no tentative credential or browser-row mutation
  commits, and retry succeeds after restoration. Retain the existing revoked
  API-key test as the ordinary-refusal control. No new harness is needed.

### FIND-TASK-003-15 — Newly shared API-key test support has invalid rustdoc

- **Discovery source:** `STD-R3-001`
- **Status:** **CONFIRMED**
- **Classification:** `VIOLATION`
- **Violated obligation:** `AGENTS.md` section 16 and
  `architecture/agent-rules.md` require substantive, accurate rustdoc for every
  materially modified Rust module and test helper, including accurate panic
  conditions.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:540-541,866-904`.
- **Evidence:** the newly crate-visible shared test module has no rustdoc, and
  `insert_live_api_key` carries another test's contract and panic description
  instead of its own.
- **Observable consequence:** the candidate violates a hard repository merge
  rule, and sibling maintainers cannot determine the shared fixture's actual
  ownership or panic boundary from its documentation.
- **Decision-complete correction:** document the existing module's shared
  API-key/browser-session fixture role and replace only
  `insert_live_api_key`'s stale rustdoc with its actual seed contract and hash
  or insert panic conditions. Add no wrapper or fixture layer.
- **Focused closure proof:** direct source inspection plus the existing format
  and lint lanes; no runtime test is required for a documentation-only change.

## Prior-finding closure

| Prior finding | Validation result and current source evidence |
|---|---|
| `FIND-TASK-003-1` | **CLOSED under the approved human replacement.** Typed optional callback `iss`, discovery advertisement projection, and pre-token exact comparison preserve the conditional RFC 9207 rule. |
| `FIND-TASK-003-2` | **CLOSED.** Browser API-key entry retains the shared fixed-cost verifier/dummy path. |
| `FIND-TASK-003-3` | **CLOSED.** Cookie suffixes are lookup hints only; chooser metadata comes from authenticated server reads. |
| `FIND-TASK-003-4` | **CLOSED.** Canonical inventory queries include every non-null browser envelope without an expiry predicate. |
| `FIND-TASK-003-5` | **CLOSED.** The production journey drives native fetch through the trusted TLS terminator while retaining literal-loopback HTTP and plaintext refusal. |
| `FIND-TASK-003-6` | **CLOSED.** The journey uses Keycloak and Dex and preserves genuine provider callback parameters while changing only victim state. |
| `FIND-TASK-003-7` | **CLOSED.** The originally cited Rust items remain substantively documented. |
| `FIND-TASK-003-8` | **CLOSED.** Server session projection carries the authoritative typed tenant id without browser exposure. |
| `FIND-TASK-003-9` | **CLOSED.** `SessionLifetime::Until` and its dead SQL capability remain absent. |
| `FIND-TASK-003-10` | **CLOSED for ordinary lifecycle refusal.** The rollback/relock path preserves issued access to exact expiry. Its lossy classification introduced the distinct shared regression retained as `FIND-TASK-003-14`; the prior ID is not reopened. |
| `FIND-TASK-003-11` | **CLOSED.** The RFC 9207 raw field and fallible discovery projection now have substantive rustdoc and `# Errors`. |
| `FIND-TASK-003-12` | **CLOSED.** Distinct cookie hints resolve sequentially through the existing `read` owner. |
| `FIND-TASK-003-13` | **CLOSED.** Only absent upstream configuration defaults; an explicit empty string reaches URL parsing and fails. |

## Human-directed additions

- The issuer-binding direction is satisfied by the conditional RFC 9207
  discovery/callback contract and receives no finding ID.
- The revision-7 real connection-test direction is satisfied by the common
  authorization-code exchange, exact candidate/tester binding, current
  permission recheck, transactional tested stamp/audit, and no-issuance return.
  It receives no finding ID.

These human directions remain distinct from independently validated reviewer
findings, as required.

## Validation result

**FIX_REQUIRED**

The final ledger contains `FIND-TASK-003-14` and `FIND-TASK-003-15`. Their
corrections stay within existing owners and approved behavior and require no
new product, public API, architecture, compatibility, concurrency,
resource-ownership, security, or persistent-data decision. No specification
revision is required.
