# TASK-003 R4 focused follow-up review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `6aedcda5166509001db0cc851a5bc74502b4b043`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority: `TASK-003-R2-production-ui-remediation.md` and the
  explicitly authorized `TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`
- Human directions: issuer binding and real interactive connection testing

The candidate resolved to the stated object before source inspection and again
before this report was written. The repository has no `.codegraph/` directory,
so this pass used the immutable diffs, repository search, and direct source
inspection. This follow-up investigates only the two assigned conflicts and
does not vote an overall task verdict.

## Paths and authorities inspected

- `AGENTS.md` §§11, 12, 15, and 16;
  `architecture/agent-rules.md`;
  `architecture/references/languages/{rust-core,maintainer-style,spec-driven-development,testing-workflows}.md`
- revision-7 spec, TASK-003, both supplied remediation tasks, both human
  directions, and every R4 discovery report
- complete base-to-candidate diff and the R3 locator diff
- `crates/wyrd/wyrd-auth/src/browser_sessions.rs`, including
  `BrowserSessions::{current,renew}`, `Renewal`, `open_text`,
  `open_credential`, their complete callers, unit tests, and the four focused
  Postgres renewal tests
- `crates/wyrd/wyrd-auth/src/{issuance,refresh,exchange_api_key,lib}.rs`, all
  definitions and workspace callers of the three `is_refusal` methods, and the
  external users of their owning error types
- `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts` for
  the final cookie/error consumer of browser-session failures

## A. Unopenable stored renewal credential

### Runtime resolution

The reviewers agree on the runtime result, and source proves it. For both
renewal modes, `BrowserSessions::renew` calls `open_credential`
(`browser_sessions.rs:585-598`). A missing envelope or one that `open_text`
cannot decrypt/parse becomes `Renewal::Failed(WyrdError::Internal)`
(`browser_sessions.rs:765-777`). `BrowserSessions::current` returns that error
without committing, revoking, or serving the stored access token
(`browser_sessions.rs:503-528`). The BFF retains the cookie for this non-401
failure. The approved behavior is therefore correctly implemented: the
request fails closed and the unchanged session can retry after keyring or data
repair.

### Documentation resolution

The documentation conflict is material. `open_text` still claims that a
credential no held key opens "ends its session"
(`browser_sessions.rs:753-757`), although its new renewal caller deliberately
converts that exact condition into a retryable internal failure. The newly
added classification-test rustdoc likewise says an "unusable credential" is a
refusal "wherever" it surfaces (`browser_sessions.rs:840-845`), while the
new `open_credential` contract states the opposite for a stored renewal
envelope (`browser_sessions.rs:765-770`). The helper's raw `InvalidToken`
return remains correct for access-token and CSRF callers, but session policy
belongs to each caller; the unconditional lifecycle sentence is no longer an
accurate contract after the R3 change.

This is governed by the hard Rust documentation rule in `AGENTS.md` §16 and
`architecture/agent-rules.md`: materially affected Rust documentation must
describe actual workflow side effects. It is not documentation-only, because
the same changed producer lacks a regression check.

### Proof resolution

The existing proof does not directly cover this producer. The two focused
Postgres internal-failure tests successfully open the stored credential and
then inject a corrupt role. They prove that an already-produced
`Renewal::Failed` rolls back and remains retryable, but they remain green if
`open_credential` regresses to `Renewal::Refused`. The classification unit test
does not call `open_credential` at all.

Because this is new core behavior, the implementer explicitly flagged it, and
the pure private helper is straightforward to exercise, the absence is a
material test gap under `AGENTS.md` §12 (new core behavior has Rust tests when
practical) and the task workflow's Red-Green requirement. No new Postgres or
browser harness is needed. The smallest closure is one focused unit test that
passes both `None` and ciphertext sealed under an unheld key and asserts
`Renewal::Failed(WyrdError::Internal)`, while the existing Postgres tests
continue to prove the transaction consequence.

### Proposed finding `FOLLOWUP-R4-001`

- **Classification:** `VIOLATION`
- **Location:** `crates/wyrd/wyrd-auth/src/browser_sessions.rs:753-777,840-881`
- **Violated obligation:** materially affected Rust contracts must accurately
  describe lifecycle behavior, and practical new core behavior must have a
  runnable regression check.
- **Consequence:** adjacent contracts give opposite answers about whether a
  retired/unheld sealing key ends a browser session, and the current test set
  cannot detect restoring the old terminal `Renewal::Refused` mapping.
- **Smallest correction:** make `open_text` document only its returned
  `InvalidToken` and caller-owned policy; narrow the classification-test prose
  to actual credential/lifecycle refusals; add one direct unit test of
  `open_credential` for missing and unopenable envelopes. Retain the existing
  Postgres tests; add no new fixture or end-to-end journey.

Resolution: this is **one combined documentation-and-test finding**, not a
runtime defect and not merely a verification limit.

## B. Visibility of renewal classifiers

The three methods are public through public modules:

- `IssuanceError::is_refusal` — `issuance.rs:285-300`
- `RefreshError::is_refusal` — `refresh.rs:55-71`
- `ExchangeError::is_refusal` — `exchange_api_key.rs:88-108`
- module exposure — `lib.rs:14-15,24`

Repository-wide caller tracing finds no external method consumer. Production
calls are confined to `wyrd-auth`: `BrowserSessions::renew` calls the refresh
and exchange classifiers, and those classifiers call the issuance classifier.
The remaining calls are `wyrd-auth` tests. Other crates legitimately consume
the public error types and variants, but none consumes these methods; narrowing
the methods does not narrow the error types or any HTTP, SDK, schema, or stable
error contract.

`architecture/references/languages/rust-core.md` requires `pub(crate)` by
default and `pub` only for an intentional public surface. R3 also explicitly
forbids a new public contract. No task requirement, remediation outcome, or
caller earns public visibility here. This is therefore a hard bounded
repository-rule violation, not optional API polish.

### Proposed finding `FOLLOWUP-R4-002`

- **Classification:** `DRIFT` / repository standards violation
- **Location:** `crates/wyrd/wyrd-auth/src/issuance.rs:294`,
  `crates/wyrd/wyrd-auth/src/refresh.rs:64`, and
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:97`
- **Violated obligation:** public visibility must be intentional and the R3
  correction must add no public contract.
- **Consequence:** a private browser-renewal classification detail becomes a
  callable crate API with an unnecessary compatibility/documentation burden.
- **Smallest correction:** change only the three methods to `pub(crate)` and
  retain their existing exhaustive matches and tests. No trait, shared policy
  type, or new abstraction is warranted.

## Overall result

**RESOLVED**

Both conflicts are source-resolved. The retryable envelope-open runtime
behavior is correct, but its contradictory local rustdoc and missing direct
producer check form one material finding. The three public classifier methods
form a second material finding because all callers are crate-local and the
governing visibility rule plus R3 non-goal require `pub(crate)`.
