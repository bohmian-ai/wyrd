# Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `6aedcda5166509001db0cc851a5bc74502b4b043`
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation tasks:
  `changes/active/oidc-production-readiness/review/TASK-003-r2/TASK-003-R2-production-ui-remediation.md`
  and
  `changes/active/oidc-production-readiness/review/TASK-003-r3/TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`
- Human directions:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  and
  `changes/active/oidc-production-readiness/review/TASK-003-r2/human-direction-connection-test.md`
- Approved specification: `SPEC-oidc-production-readiness` revision 7

The candidate remained at the stated commit during this review. I reviewed the
complete base-to-candidate change and used `8289fa298..6aedcda51` only to
locate the latest remediation owners. I did not modify reviewed source.
CodeGraph was unavailable because the repository has no `.codegraph/`
directory.

## Authorities Read

- `AGENTS.md`, especially ownership, struct-centered Rust, public contracts,
  documentation, and test-tier rules
- `architecture/agent-rules.md`
- `architecture/references/languages/maintainer-style.md`
- Applicable identity, tenant, UI, and public-surface authority in
  `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and
  `architecture/wyrd-security-posture.md`
- The original task, both supplied remediation tasks, both human directions,
  and the prior R1-R3 review artifacts

## Changed-Surface Coverage

| Surface | Changed owners, callers, and tests inspected | Maintainer assessment |
|---|---|---|
| Browser renewal orchestration | `BrowserSessions::{read,authority,current,renew}`; `Renewal`; `open_text`; `open_credential`; BFF read/authority handlers and `ServerSessions` consumers | The three transaction outcomes now remain visible on the existing owner. Replay containment commits, ordinary refusal uses the bounded relock, and internal failure drops the transaction. `MAINT-R4-002` covers the contradictory helper contract and missing direct proof for the explicitly flagged unopened-credential behavior. |
| Refresh classification | `RefreshError`, `RefreshError::is_refusal`, `RefreshTokens::execute`, ordinary HTTP refresh routing, browser-session caller, and tests | The classification is small and exhaustive, but its visibility unnecessarily expands the crate API; see `MAINT-R4-001`. |
| API-key classification and error projection | `ExchangeError`, `ExchangeError::is_refusal`, `ExchangeApiKey::execute`, `map_exchange_error_to_wyrd`, browser-session caller, connection recovery-key caller, and tests | The browser path reuses the existing exchange and public-error owners. The classifier has the same unnecessary public visibility; see `MAINT-R4-001`. |
| Shared issuance classification | `IssuanceError`, `IssuanceError::is_refusal`, `TenantTokenIssuer` issuance paths, refresh/API-key wrappers, and tests | One exhaustive lifecycle predicate prevents sibling wrappers from drifting. Its only production consumers are inside `wyrd-auth`, so public visibility is not earned; see `MAINT-R4-001`. |
| R3 transaction and recovery proofs | `proactive_refresh_replay_commits_containment_and_preserves_authority_until_expiry`; both internal-failure Postgres tests; ordinary-refusal control; classification unit test and helpers | The tests clearly prove replay containment and the common `Renewal::Failed` rollback/retry path using corrupt roles. They do not directly exercise the newly changed missing/unopenable stored-credential mapping. |
| R3 shared test documentation | `exchange_api_key::pg_tests` and `insert_live_api_key` plus sibling browser-session fixture consumers | The prior rustdoc defect is closed: the module states its shared role and the helper describes its seed and panic contract accurately. |
| Prior cumulative owners | Callback/provider exchange, candidate testing, connection lifecycle, login state, SQL browser-session persistence, sealing inventory/rewrap, BFF transport/session projection, SvelteKit routes/settings/chooser, migrations, generated schemas/docs, and identity/UI journeys | Current source retains the typed owners and generated parity established by the prior rounds. No new competing owner, dependency, migration, compatibility path, or test harness entered the R3 remediation. |

## Material Findings

### MAINT-R4-001 — Internal renewal classifiers became unrequested public API

- **Location:**
  `crates/wyrd/wyrd-auth/src/issuance.rs:285-300`,
  `crates/wyrd/wyrd-auth/src/refresh.rs:55-71`, and
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:88-108`
- **Governing principle:** The R3 remediation explicitly prohibits a new
  public contract. `AGENTS.md` and the maintainer guide require the narrowest
  contract needed by real callers; the Ponytail ladder rejects API surface
  without a current external consumer.
- **Evidence:** The candidate adds `pub fn is_refusal` to three error types in
  public modules. Repository-wide caller inspection finds production calls
  only within `wyrd-auth`: `BrowserSessions::renew` uses the refresh and
  exchange predicates, and those predicates use the issuance predicate. The
  remaining calls are the same crate's tests. No external crate calls any of
  the three methods.
- **Concrete maintenance cost:** These implementation-specific browser-renewal
  classifications are now callable crate API and therefore appear to promise
  stable semantics to unrelated consumers. A future change to renewal policy
  must account for external callers even though the task required only one
  private orchestration seam.
- **Smallest testable correction:** Change all three methods to `pub(crate)`;
  preserve the existing enums, exhaustive matches, callers, and tests. Format,
  lint, and the existing classification selector are sufficient proof. Add no
  trait or new classifier type.
- **Nearby pattern:** `DelegateError::is_store_failure` in
  `exchange_api_key.rs` is private because its classification belongs only to
  its owning workflow.

### MAINT-R4-002 — The newly retryable unopened-credential path has contradictory documentation and no direct check

- **Location:**
  `crates/wyrd/wyrd-auth/src/browser_sessions.rs:755-777,840-881`; callers at
  `browser_sessions.rs:475-612`; Postgres proof at
  `browser_sessions.rs:1098-1421`
- **Governing principle:** `AGENTS.md` and the maintainer guide require
  substantive Rust documentation to describe actual behavior and tests to
  prove a caller-relevant changed outcome. The active Ponytail rule requires
  one runnable check for a non-trivial branch.
- **Evidence:** `open_credential` now converts a missing or unopenable stored
  renewal credential to `Renewal::Failed(WyrdError::Internal)`, so `current`
  drops the transaction, returns the failure before or after access expiry,
  and leaves the browser row retryable. The adjacent `open_text` contract still
  says that a credential sealed under a retired key "ends its session," which
  is false for this new renewal caller. The classification unit test does not
  call `open_credential`, and the two Postgres internal-failure tests inject a
  corrupt role after successfully opening the stored credential. Thus the
  specifically flagged behavior is source-plausible but not directly pinned.
- **Concrete maintenance cost:** A maintainer deciding whether keyring mismatch
  is terminal receives opposite answers from adjacent helpers, and a later
  simplification back to `open_text(...)?` or `Renewal::Refused` can silently
  restore session-ending behavior while every added R3 test remains green.
- **Smallest testable correction:** Make `open_text` document only its returned
  `InvalidToken` result and leave session policy to its caller. Add one focused
  unit check that an absent or ciphertext-unopenable renewal credential maps to
  `Renewal::Failed(WyrdError::Internal)`; the existing Postgres tests already
  prove that any `Renewal::Failed` leaves the row retryable, so no new fixture
  or end-to-end journey is warranted.
- **Nearby pattern:** `open_completion` documents its own decoding failure,
  while its caller owns the surrounding login-flow consequence.

## Prior-Finding and Direction Closure

| Item | Current-source evidence | Result |
|---|---|---|
| `FIND-TASK-003-14` replay containment | `Renewal::Contained` preserves the refresh owner's staged family revocation/audit, commits before the bounded relock, and the focused Postgres test checks successor revocation, audit, pre-expiry authority, and post-expiry browser revocation. | CLOSED |
| `FIND-TASK-003-14` internal failures | Non-refusal refresh/API-key failures become `Renewal::Failed`; both Postgres tests prove rollback, request failure before and after expiry, and successful retry after repair. `MAINT-R4-002` is limited to documentation and direct regression proof for the separately flagged unopened-credential producer. | CLOSED WITH MAINTAINER FINDING |
| `FIND-TASK-003-15` shared test rustdoc | `exchange_api_key::pg_tests` and `insert_live_api_key` now carry accurate, cohesive rustdoc. | CLOSED |
| Prior R1/R2 findings | Current owners retain tenant-id projection, canonical sealing inventory, fixed session lifetime, bounded chooser reads, strict upstream parsing, conditional issuer binding, real provider test sign-in, and the prior documentation corrections. | CLOSED |
| Human issuer direction | Optional typed callback `iss` remains conditionally enforced by advertised provider support on the common exchange path. | CLOSED |
| Human real-sign-in direction | Candidate testing continues through the shared authorization-code exchange and stamps only the exact authorized candidate after verified callback completion. | CLOSED |

## Uncertain Preferences Kept Out of Findings

- `renewal_refused` is also set after `Renewal::Contained`, and the associated
  info message calls both outcomes "refused." A more neutral name and distinct
  diagnostic could be clearer, but the enum and surrounding rustdoc expose the
  transaction distinction, and the canonical containment audit remains the
  security evidence. This does not independently justify another remediation.
- Three `is_refusal` methods duplicate a small closed-set concept at three
  nested owners. Keeping each exhaustive match beside the error variants is
  easier to maintain than adding a trait or centralized policy table.
- The long Postgres tests are justified by their transaction and recovery
  assertions. Extracting another harness for the unopened-envelope case would
  add more machinery than the direct unit classification check requires.

## Verification Assessment

The R3 implementation record reports the four focused Postgres selectors, the
classification unit selector, `test:wyrd`, `test:identity:journey`,
`check:tenant-isolation`, formatting, lints, and `git diff --check` green. I
inspected the named tests and current lane evidence but did not rerun build,
Postgres, provider, browser, Cargo, mise, or pnpm commands in this review-only
role. The recorded internal-failure tests prove the shared rollback/retry
consumer path, but not the specifically flagged unopened stored-credential
producer.

## Overall Result

**FAIL**

The R3 runtime owner and transaction semantics are substantially clearer and
the prior findings close, but the candidate adds three unnecessary public
methods despite the no-public-contract constraint and leaves the flagged
unopened-credential behavior contradicted by adjacent rustdoc and without a
direct regression check.
