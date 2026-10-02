# TASK-003 R4 structured finding validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `6aedcda5166509001db0cc851a5bc74502b4b043`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority:
  `changes/active/oidc-production-readiness/review/TASK-003-r2/TASK-003-R2-production-ui-remediation.md`
  and
  `changes/active/oidc-production-readiness/review/TASK-003-r3/TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`
- Human directions:
  `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and
  `TASK-003-r2/human-direction-connection-test.md`

The candidate resolved to the stated object before validation. This repository
has no `.codegraph/` directory, so source navigation used the immutable Git
range, repository search, and direct source inspection. Per assignment, this
validation changed no reviewed source and ran no build, provider, browser, or
database command.

## Inputs and source coverage

This validation read the complete cumulative diff, the R3 locator diff, the
approved specification and task, both supplied remediation tasks, both human
directions, prior R1-R3 findings and verdicts, applicable repository,
security, Rust, testing, and maintainer authorities, and every required R4
discovery and follow-up report.

The producer-to-consumer trace covered:

- `IssuanceError`, `RefreshError`, and `ExchangeError`, every repository caller
  of their new `is_refusal` methods, and their public module exposure;
- `BrowserSessions::{current,renew}`, `Renewal`, `open_text`, and
  `open_credential` in full;
- both renewal modes, the existing classification unit test, and the four R3
  Postgres renewal tests;
- the private BFF `sessions/read` and `sessions/authority` consumers and their
  cookie-deletion behavior; and
- sealing-key construction, opening, inventory, and rewrap behavior relevant
  to a credential sealed under an unheld key.

## Explicitly flagged behavior

The implementer-highlighted runtime behavior is correct.

For both renewal modes, `BrowserSessions::renew` calls `open_credential`
(`browser_sessions.rs:585-597`). A missing envelope or one that `open_text`
cannot open becomes `Renewal::Failed(WyrdError::Internal)`
(`browser_sessions.rs:765-777`). `BrowserSessions::current` returns that error
without committing, revoking, or serving the stored access token
(`browser_sessions.rs:503-528`). Dropping the tenant transaction rolls back
any tentative work and leaves the browser row unchanged. The BFF deletes the
cookie only on `401` or a tenant mismatch; another error is propagated while
the cookie remains (`server-sessions.ts:192-205,317-332`). Restoring the held
key or repairing the envelope therefore allows a later request to retry.

This is the approved request-boundary failure: no authority is returned while
the credential cannot be opened, but security uncertainty is not converted
into a durable session-ending decision. It requires no specification revision.

## Proposal validation

### `STD-TASK-003-R4-1`, `MAINT-R4-001`, and `FOLLOWUP-R4-002` — **CONFIRMED and consolidated**

The three proposals report one cause. The R3 remediation added
`IssuanceError::is_refusal`, `RefreshError::is_refusal`, and
`ExchangeError::is_refusal` as public methods
(`issuance.rs:285-300`, `refresh.rs:55-71`,
`exchange_api_key.rs:88-108`). Their modules and owning error types are public,
so these methods are externally callable crate API.

Repository-wide caller tracing finds no external consumer. Production calls
are confined to `wyrd-auth`: `BrowserSessions::renew` calls the refresh and
exchange classifiers, and those classifiers call the issuance classifier.
The remaining calls are the same crate's tests. The error types themselves
remain legitimately public; only these browser-renewal classification methods
are wider than their callers require.

The public visibility cannot be justified as reuse or future flexibility.
`architecture/references/languages/rust-core.md` requires `pub(crate)` by
default, and the R3 task explicitly prohibited a new public contract. The
Ponytail ladder stops at narrowing the existing methods: no trait, common
policy type, re-export change, or new abstraction is warranted.

Disposition: retain one new finding, `FIND-TASK-003-16`.

### `INV-R4-001`, `MAINT-R4-002`, and `FOLLOWUP-R4-001` — **CONFIRMED and consolidated**

The runtime mapping is correct, but the materially affected local contract and
proof are not.

`open_text` says that a credential sealed under a retired key ends its session
(`browser_sessions.rs:753-757`). That helper only returns
`WyrdError::InvalidToken`; lifecycle policy belongs to its caller. Its access
token and CSRF callers do end or refuse their request, while the newly changed
renewal caller deliberately converts the same open failure into retryable
`Renewal::Failed`. The unconditional session-ending sentence is therefore
false for a reachable changed caller.

The new classification-test rustdoc also says an "unusable credential" is a
refusal "wherever" it surfaces (`browser_sessions.rs:840-845`), directly beside
`open_credential`'s opposite missing/unopenable-envelope contract. It needs to
name rejected credential/lifecycle outcomes rather than include stored
envelope-open failure.

The existing proof does not pin this producer. The classification unit test
does not call `open_credential`. The two R3 internal-failure Postgres tests
successfully open the stored credential and then inject a corrupt role. They
prove that an already-produced `Renewal::Failed` rolls back and remains
retryable, but they remain green if `open_credential` regresses to
`Renewal::Refused`. The behavior is new core logic, practical to test without
IO, and explicitly called out for this review; treating the missing direct
check as a verification limit would not satisfy `AGENTS.md` section 12 or the
active task's Red-Green requirement.

The smallest correction stays in `browser_sessions.rs`: make `open_text`
document its returned error and caller-owned lifecycle policy, narrow the
classification-test prose to actual credential/lifecycle refusals, and add one
focused unit test that passes both no envelope and ciphertext sealed under a
different held key to `open_credential` and observes
`Renewal::Failed(WyrdError::Internal)`. The existing Postgres tests remain the
transaction-level proof. No database fixture, browser journey, helper layer,
or new harness is justified.

Disposition: retain one new finding, `FIND-TASK-003-17`.

## Validation of empty discovery ledgers

The empty proposed ledgers from the behavior, system-resilience,
security/identity, and persistence/concurrency reviewers were checked against
source rather than accepted by agreement.

- The three-way renewal outcome is preserved: replay containment commits,
  ordinary refusal rolls back before exact expiry and ends the session at or
  after expiry, and internal failure returns without commit.
- A missing or unopenable stored renewal credential reaches the internal
  failure branch and is retryable; it does not reopen `FIND-TASK-003-14`.
- The BFF retains the cookie for that non-`401` failure and exposes no fallback
  authority.
- Sealing inventory and exact-byte rewrap ownership remain unchanged; no new
  persistent marker, audit owner, migration, retry loop, or compatibility path
  entered R3.
- The supplied R3 evidence credibly covers replay containment, ordinary
  refusal, and corrupt-role internal failures. Its only material omission is
  the direct envelope-open producer check retained in `FIND-TASK-003-17`.

No additional behavior, security, persistence, concurrency, or resilience
finding is supported.

## Ponytail correction selection

Both retained corrections stop at existing owners:

1. Narrow three methods from `pub` to `pub(crate)`. Their exhaustive matches,
   error enums, callers, and tests remain unchanged.
2. Correct two nearby rustdoc contracts and add one pure unit check for the
   changed `open_credential` mapping. Reuse the current `SealingKeyring`,
   `Renewal`, and test module; retain the existing Postgres rollback/retry
   tests for downstream consequence.

Deletion of the classifiers or `open_credential` would duplicate classification
at callers or restore the original lossy renewal path. A trait, shared policy
object, fixture abstraction, new integration test, or new dependency would add
complexity without closing another obligation.

## Final deduplicated finding ledger

### FIND-TASK-003-16 — Renewal classifiers unnecessarily widen the public Rust API

- **Discovery sources:** `STD-TASK-003-R4-1`, `MAINT-R4-001`,
  `FOLLOWUP-R4-002`
- **Status:** **CONFIRMED**
- **Classification:** `DRIFT`
- **Violated obligation:** the R3 remediation's explicit no-new-public-contract
  constraint and `architecture/references/languages/rust-core.md`'s
  `pub(crate)`-by-default rule require visibility no wider than real callers.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/issuance.rs:285-300`,
  `crates/wyrd/wyrd-auth/src/refresh.rs:55-71`, and
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:88-108`.
- **Evidence:** all repository callers of the three `is_refusal` methods are
  inside `wyrd-auth`; no external crate consumes them, while their public
  modules make the methods externally callable.
- **Observable consequence:** a private browser-renewal policy becomes a
  promised crate API with avoidable compatibility and documentation burden.
- **Decision-complete correction:** change only the three methods to
  `pub(crate) fn is_refusal`; retain their error types, exhaustive matches,
  callers, rustdoc, and existing tests. Add no trait, policy type, or re-export.
- **Focused closure proof:** repository-wide caller search confirms no external
  use; run the existing
  `browser_sessions::tests::only_lifecycle_refusals_end_a_renewing_session`
  exact selector, then the normal format, lint, and `test:wyrd` lanes.

### FIND-TASK-003-17 — Retryable renewal-envelope failure has contradictory rustdoc and no direct regression check

- **Discovery sources:** `INV-R4-001`, `MAINT-R4-002`,
  `FOLLOWUP-R4-001`
- **Status:** **CONFIRMED**
- **Classification:** `VIOLATION`
- **Violated obligation:** `AGENTS.md` sections 12 and 16 and
  `architecture/agent-rules.md` require accurate workflow and side-effect
  rustdoc for materially affected Rust items and practical Rust proof for new
  core behavior. The R3 task requires internal failures, including
  envelope-open failure, to remain retryable.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/browser_sessions.rs:753-777,830-881`; downstream
  transaction owner at `browser_sessions.rs:470-612`.
- **Evidence:** `open_credential` correctly maps absent/unopenable renewal
  state to `Renewal::Failed(Internal)`, but `open_text` says the same key
  failure ends the session and the classification-test rustdoc calls an
  unusable credential a refusal wherever it appears. No current test calls
  `open_credential`; the internal-failure Postgres tests inject only after a
  successful credential open and cannot detect regression of this producer.
- **Observable consequence:** adjacent contracts give opposite lifecycle
  instructions, and restoring the old terminal mapping can leave every R3
  runtime test green while permanently ending affected browser sessions.
- **Decision-complete correction:** make `open_text` describe only its
  `InvalidToken` return and caller-owned lifecycle consequence; narrow the
  classification-test prose to rejected credential/lifecycle outcomes; add
  one focused unit test in the existing module proving that both an absent
  renewal envelope and ciphertext sealed under an unheld key produce
  `Renewal::Failed(WyrdError::Internal)`. Keep `open_credential`,
  `BrowserSessions`, and all Postgres tests unchanged.
- **Focused closure proof:** run the new exact unit selector and retain the two
  existing refresh/API-key internal-failure Postgres selectors as proof that
  `Renewal::Failed` rolls back and remains retryable before and after expiry;
  then run format, lint, and `test:wyrd`.

Neither correction requires a new product, public API, architecture, security,
compatibility, cross-service, concurrency, resource-ownership, or persistent-
data decision.

## Prior-finding closure

| Prior finding | Validation result and current-source evidence |
|---|---|
| `FIND-TASK-003-1` | **CLOSED under the approved human replacement.** Typed optional callback `iss` and conditional pre-token issuer comparison remain in the common exchange owner. |
| `FIND-TASK-003-2` | **CLOSED.** Browser API-key entry still uses the shared fixed-cost verifier/dummy path. |
| `FIND-TASK-003-3` | **CLOSED.** Cookie suffixes remain lookup hints and are server-resolved sequentially before rendering. |
| `FIND-TASK-003-4` | **CLOSED.** Canonical inventory includes every stored browser envelope, including expired rows, and exact-byte CAS remains the rewrap fence. |
| `FIND-TASK-003-5` | **CLOSED.** The production-built BFF trusted-TLS exercise remains in the identity journey. |
| `FIND-TASK-003-6` | **CLOSED.** Keycloak/Dex switching and standards-shaped mixed callbacks remain in the real browser journey. |
| `FIND-TASK-003-7` | **CLOSED.** The originally cited Rust items remain substantively documented. |
| `FIND-TASK-003-8` | **CLOSED.** The authoritative tenant id remains server-only in the session projection. |
| `FIND-TASK-003-9` | **CLOSED.** The unused lifetime variant and its SQL branch remain absent. |
| `FIND-TASK-003-10` | **CLOSED for ordinary refusal.** The rollback/relock path preserves issued authority to exact expiry. |
| `FIND-TASK-003-11` | **CLOSED.** The discovery field and parser retain substantive rustdoc and `# Errors`. |
| `FIND-TASK-003-12` | **CLOSED.** Distinct cookie hints resolve sequentially. |
| `FIND-TASK-003-13` | **CLOSED.** Explicit empty upstream configuration reaches URL validation and is refused. |
| `FIND-TASK-003-14` | **CLOSED.** Replay containment, ordinary refusal, and internal failure retain distinct transaction meanings. `FIND-TASK-003-17` is a new local documentation/proof gap, not a reopened runtime defect. |
| `FIND-TASK-003-15` | **CLOSED.** The shared API-key test module and live-key fixture now document their real roles and panic boundary. |

## Human-directed additions

- The issuer-binding direction remains satisfied by conditional RFC 9207
  handling and receives no retrospective finding ID.
- The real interactive connection-test direction remains satisfied by the
  shared authorization-code exchange, exact candidate/tester binding,
  permission recheck, transactional tested stamp/audit, and no-issuance
  result. It receives no retrospective finding ID.

## Validation result

**FIX_REQUIRED**

The final ledger contains `FIND-TASK-003-16` and `FIND-TASK-003-17`. Both are
bounded corrections within existing owners and approved revision-7 behavior;
no specification revision is required.
