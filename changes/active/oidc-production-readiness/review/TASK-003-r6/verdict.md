# TASK-003 round-6 review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
- Candidate tree: `c908c707b5809794a9828f091569aec5e965c083`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority: TASK-003 R2 and the explicitly human-authorized R3,
  R4, and R5 tasks
- Human directions: R1 conditional issuer binding, R2 real interactive
  connection testing, and R5 session-chain-only logout revocation. The R5
  direction replaces FIND-18's earlier principal-family correction.

The complete cumulative range was reviewed. The latest remediation range
`989d0734b0a9b04f314ef4b52aa7d8510f26fe11..ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
was used to locate the R5 refresh-chain correction and the bundled
`wyrd-testing` shutdown fix. The candidate commit and tree remained fixed.
The repository has no `.codegraph/` directory.

## Independent review results

| Report | Result | Material proposals |
|---|---|---|
| `task-review-behavior.md` | PASS | Empty ledger. |
| `task-review-invariants.md` | PASS | Empty ledger. |
| `standards-review.md` | PASS | Empty ledger. |
| `maintainer-review.md` | PASS | Empty ledger. |
| `system-review.md` | FAIL | `SYSTEM-R6-001`: ordinary shutdown can drain/abort Bifrost before a bound dedicated Forge worker joins. |
| `domain-review-security-identity.md` | PASS | Empty ledger. |
| `domain-review-persistence-concurrency.md` | PASS | Empty ledger. |
| `followup-review.md` | RESOLVED | Confirmed the system claim and narrowed it to the ordinary dedicated-worker shutdown seam. |
| `findings-validation.md` | FIX_REQUIRED | Retained `FIND-TASK-003-19`. |

## Follow-up decision

A focused follow-up was required because the system reviewer found a reachable
dedicated Forge-worker lifecycle regression while the persistence and standards
reviews accepted the shutdown change. The follow-up traced both teardown seams:
cluster/load callers use join-first `shutdown_and_inspect`, but the public
`start_bound().shutdown()` dedicated-worker composition stores a live
`serve_handle` and assigns `Mode::InProcess`. The candidate's new mode-based
branch therefore calls Bifrost shutdown before joining that worker. The
uncertainty was resolved and the structured Ponytail validator independently
confirmed the narrowed finding.

## Reconciled acceptance matrix

| Obligation | Reconciled implementation and proof | Result |
|---|---|---|
| Production SSO and OIDC-off browser sessions are server-owned, tenant-bound, CSRF-protected, replica-safe, and browser-secret-free | The cumulative Rust server, Postgres session owner, private BFF channel, SvelteKit projection, and real browser journeys retain opaque cookies, server-only Wyrd authority, tenant verification, trusted TLS, and permission enforcement. | PASS |
| Candidate connection testing uses a real provider sign-in and conditional RFC 9207 issuer binding follows approved human direction | The common authorization-code exchange keeps exact revision/tester binding, PKCE/nonce/ID-token verification, conditional callback `iss`, and no User/session/credential issuance for tests. | PASS |
| Renewal refusal, replay containment, internal failure, sealing rotation, connection cutoff, and prior rustdoc/API boundaries remain distinct and closed | Existing owners and focused tests preserve the R2-R4 corrections and `FIND-TASK-003-1` through `-17`. | PASS |
| R5 logout revokes only the current session's refresh chain | The browser row stores its tenant-bound root refresh id; logout takes the existing User family lock, recursively revokes only that root and descendants, wipes the browser row in the same transaction, and leaves another login active. The focused Postgres test covers a committed successor and unreadable envelope. | PASS — `FIND-TASK-003-18` closed under the replacing human direction |
| API-key logout remains browser-only | API-key sessions carry no refresh-chain id; logout wipes browser state without revoking the operator API key. | PASS |
| The bundled in-process test-harness fix preserves sibling shutdown lifecycle ordering | Router-only in-process shutdown now settles Bifrost before fixture drop, but the same `Mode::InProcess` predicate also matches a bound dedicated Forge worker with a live `serve_handle`, causing Bifrost abort/storage settlement before worker join. | **FAIL — `FIND-TASK-003-19`** |
| Explicit non-goals remain excluded | No browser-held Wyrd credential, local password authority, UI role mapper, provider-specific bypass, compatibility path, new dependency, alternate audit owner, principal-wide logout revocation, or speculative lifecycle abstraction entered the cumulative candidate. | PASS |

## Validated finding ledger

### FIND-TASK-003-19 — Direct in-process shutdown can abort storage before a bound Forge worker joins

- **Status:** CONFIRMED
- **Classification:** REGRESSION
- **Discovery sources:** `SYSTEM-R6-001`, `FOLLOWUP-R6-001`
- **Violated obligation:** The authorized R5 harness correction must settle
  router-only in-process Bifrost owners before fixture release without moving
  role/storage teardown ahead of a live dedicated Forge worker.
- **Location:** `crates/wyrd/wyrd-testing/src/server.rs:771-778`, produced by
  the dedicated-worker branch at `crates/wyrd/wyrd-testing/src/server.rs:3557-3572`.
- **Evidence:** `bind` stores the live Forge worker in `serve_handle` and then
  assigns `Mode::InProcess`. `shutdown` tests only that mode and calls
  `Bifrost::shutdown` before taking or joining the handle. Forge supervision
  is therefore still undrained, so Bifrost takes its abort path and settles
  storage before the worker has completed its cancellation drain.
- **Observable consequence:** Ordinary shutdown of the supported bound
  dedicated-worker harness can manufacture a failed or incomplete Forge
  attempt, log the lifecycle failure, and still return success. Production
  serving and existing cluster/load teardown are not affected.
- **Required correction:** Keep `WyrdTestServer::shutdown` as the owner and
  select the direct router-only Bifrost drain from the existing fact
  `serve_handle.is_none()`, not `Mode::InProcess`. Update the adjacent rustdoc.
  Add no mode variant, lifecycle abstraction, dependency, or second shutdown
  owner.
- **Focused closure proof:** Exercise ordinary shutdown of the existing bound
  dedicated Forge-worker topology with admitted work and prove the worker
  completes its cancellation drain before required storage is released and
  no pre-join Bifrost abort occurs. Retain a router-only in-process proof that
  Bifrost/Oracle settles before the Postgres fixture is released.

## Prior-finding closure

`FIND-TASK-003-1` through `FIND-TASK-003-17` remain closed with current-source
evidence. `FIND-TASK-003-18` is closed under the R5 human replacement: logout
retires only the current login's refresh chain and preserves other same-User
browser, CLI, and SDK logins. All three human directions remain satisfied.
`FIND-TASK-003-19` is a new sibling-topology regression in the separately
authorized harness correction, not a reopening of identity or renewal behavior.

## Verification evidence and limits

The implementation record reports the focused R5 Postgres selector, all
browser-session tests, `mise run fmt`, `mise run lints`,
`mise run codegen:check`, `mise run check:tenant-isolation`,
`mise run test:sql`, `mise run test:wyrd`, `mise run test:identity:journey`,
and `git diff --check` green. Independent reviewers reran the R4 envelope test,
the R5 logout-chain test, migration idempotence, and one formerly aborting
router-only shutdown path successfully.

No existing focused test exercises ordinary `WyrdTestServer::shutdown` for a
bound dedicated Forge worker or observes worker-join versus storage-release
ordering. That missing proof accompanies the source-confirmed regression; it
is not a missing required reviewer or an unavailable immutable subject. Every
required report is present.

## Verdict

**FIX_REQUIRED**

`FIND-TASK-003-19` is one bounded test-harness correction. It requires no
specification, product, public API, security, compatibility, concurrency,
resource-ownership, or persistent-data decision.
