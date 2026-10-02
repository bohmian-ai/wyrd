# TASK-003 R6 findings validation

## Immutable subject and inputs

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
- Candidate tree: `c908c707b5809794a9828f091569aec5e965c083`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority: TASK-003 R2 and the human-authorized R3, R4, and R5 tasks
- Human directions: R1 conditional issuer binding, R2 real interactive connection testing, and R5 session-chain-only logout revocation. The R5 direction replaces FIND-18's earlier principal-family correction.

The complete cumulative diff, latest remediation diff, prior R1-R5 verdicts
and validation ledgers, all supplied human directions, repository and
applicable architecture/testing authorities, and every required R6 discovery
and follow-up report were available. The repository has no `.codegraph/`
index, so source navigation used Git, `rg`, and direct full-body inspection.
The candidate commit and tree remained fixed during validation.

## Discovery claim dispositions

| Discovery claim | Disposition | Validation |
|---|---|---|
| `task-review-behavior.md`: empty ledger | **VALIDATED** | The cumulative browser, connection-test, renewal, logout, tenant, and secret-handling paths remain on their existing owners. No behavior defect beyond the separately bundled harness change was proposed or found. |
| `task-review-invariants.md`: empty ledger | **VALIDATED** | The durable refresh-chain root, tenant-qualified FK, family lock, recursive descendant revocation, and browser-row wipe preserve the human-directed logout invariant. The report's harness row did not trace the bound dedicated-worker exception and therefore does not dispose of the system claim. |
| `standards-review.md`: empty ledger | **VALIDATED WITH HARNESS EXCEPTION** | Production/auth/SQL/UI standards claims are supported. Its harness assessment stops at the router-only topology and does not inspect `bind` assigning `Mode::InProcess` to a live Forge worker; that omission is covered by the retained finding below. |
| `maintainer-review.md`: empty ledger | **VALIDATED WITH HARNESS EXCEPTION** | The changed production owners and R5 proof remain maintainable and minimal. The statement that bound mode always delegates to its serve task is false for the dedicated Forge branch, which deliberately retains `Mode::InProcess`. |
| `domain-review-security-identity.md`: empty ledger | **VALIDATED** | No new security, RBAC, tenant-selection, secret, callback, or identity finding survives source validation. |
| `domain-review-persistence-concurrency.md`: empty ledger | **VALIDATED WITH HARNESS EXCEPTION** | Logout atomicity, refresh serialization, RLS, migration, and sealing claims hold. Its shutdown conclusion assumes every `Mode::InProcess` value has no live serve task; `bind` disproves that assumption for `BifrostTarget::ForgeWorker`. |
| `SYSTEM-R6-001` | **CONFIRMED** | `WyrdTestServer::shutdown` selects direct Bifrost shutdown by `Mode::InProcess`, while the dedicated Forge branch creates a live `serve_handle` and then assigns that mode. Bifrost rejects undrained Forge supervision and aborts storage before the later worker join. |
| `FOLLOWUP-R6-001` | **CONFIRMED** | The follow-up correctly narrows the defect to the supported ordinary `start_bound().shutdown()` dedicated-worker composition. Existing cluster/load teardown uses `shutdown_and_inspect` and is not part of the failing seam. |

## Source validation of the retained claim

The invalid state is produced at
`crates/wyrd/wyrd-testing/src/server.rs:3557-3572`. For
`BifrostTarget::ForgeWorker`, `bind` starts the production Forge worker, stores
its join handle in `serve_handle`, stores the shared cancellation token, and
then assigns `Mode::InProcess`. Thus `Mode::InProcess` is not the lifecycle
invariant documented by `shutdown`; it does not mean that no serve task exists.

The state reaches the faulty consumer at
`crates/wyrd/wyrd-testing/src/server.rs:767-783`. `shutdown` cancels the token,
then calls `Bifrost::shutdown` for every `Mode::InProcess` value before taking
or joining `serve_handle`. The public builder composition
`with_bifrost_target_for_test(BifrostTarget::ForgeWorker).start_bound()` reaches
this state without a private or fabricated caller.

The consequence is deterministic from the shared lifecycle owner.
`crates/wyrd/wyrd-server/src/state.rs:1967-2000` requires Forge supervision to
be quiesced before Oracle, Scribe, and storage drain; lines 2015-2020 reject an
unjoined supervisor, and `shutdown` then runs the abort path. That abort closes
storage while the worker join has not occurred. The worker contract at
`crates/vala/vala-bifrost-redux/src/forge/worker.rs:2042-2089` says cancellation
stops new claims but drains admitted work before returning. The candidate
therefore moves role/storage settlement ahead of the live worker whose admitted
work may still require object I/O. The later two-second join and warning cannot
restore the ordering, and `shutdown` may still return `Ok(())`.

Sibling paths do not broaden the finding. Ordinary bound API servers retain
`Mode::Bound` and let `BoundServer::run` join supervision before Bifrost drain.
Cluster and load owners call `shutdown_and_inspect`, which cancels and joins
the serve handle before inspection and drop. Router-only `start_in_process`
servers have no serve handle and need the R5 direct drain to prevent the
original Oracle/database-drop abort.

## Final deduplicated finding ledger

### FIND-TASK-003-19 — Direct in-process shutdown can abort storage before a bound Forge worker joins

- **Discovery sources:** `SYSTEM-R6-001`, `FOLLOWUP-R6-001`
- **Validation status:** **CONFIRMED**
- **Classification:** `REGRESSION`
- **Violated obligation:** The authorized R5 harness correction must settle
  router-only in-process Bifrost owners before fixture release without moving
  Bifrost role/storage teardown ahead of a live dedicated Forge worker. The
  repository's Bifrost lifecycle requires Forge supervision to quiesce before
  storage closes.
- **Exact location:**
  `crates/wyrd/wyrd-testing/src/server.rs:771-778`, produced by
  `crates/wyrd/wyrd-testing/src/server.rs:3557-3572`.
- **Evidence:** The producer stores a live dedicated-worker join handle and
  assigns `Mode::InProcess`. The consumer tests only that mode and calls
  `Bifrost::shutdown` before joining the handle. Bifrost sees
  `supervision_drained == false`, returns `"Forge supervision did not join
  before shutdown"`, and aborts selected owners including storage.
- **Observable consequence:** Ordinary shutdown of the supported bound
  dedicated-worker harness can close storage while admitted Forge work is
  still draining, manufacture a failed or incomplete Forge attempt, log the
  lifecycle failure, and still report successful harness shutdown. Production
  serving and existing cluster/load teardown are not affected.
- **Decision-complete correction:** Keep `WyrdTestServer::shutdown` as the
  sole owner. Select the R5 direct Bifrost drain from the existing ownership
  fact `serve_handle.is_none()`, not from `Mode::InProcess`. A router-only
  in-process server then retains cancellation plus direct drain before fixture
  drop, while any live serve task—including the bound dedicated Forge
  worker—is cancelled and joined before its existing teardown/drop path.
  Update the adjacent rustdoc so it states the no-serve-task condition. Add no
  mode variant, lifecycle abstraction, dependency, or second shutdown owner.
- **Focused closure proof:** In the existing test-server lifecycle coverage,
  start the existing bound `ForgeWorker` topology with admitted work, invoke
  ordinary `WyrdTestServer::shutdown`, and prove the worker completes its
  cancellation drain before required storage is released and no pre-join
  Bifrost abort occurs. Retain one router-only in-process shutdown proof that
  Oracle/Bifrost settles before the Postgres fixture is released. Run the exact
  named selectors through the owning `mise` test task, then the narrowest Wyrd
  and Bifrost harness lanes affected by the shared shutdown owner.

The correction is the first Ponytail rung that preserves both required
topologies: reuse the already-present serve-handle ownership fact. A new enum,
trait, shutdown coordinator, or generalized lifecycle state machine would add
indirection without improving the invariant.

## Prior-finding closure

| Prior finding | Current-source validation |
|---|---|
| `FIND-TASK-003-1` | **CLOSED under human direction.** Typed optional callback `iss` and conditional exact comparison remain in the common pre-token exchange owner. |
| `FIND-TASK-003-2` | **CLOSED.** Browser API-key refusal still uses the shared fixed-cost verification/dummy path. |
| `FIND-TASK-003-3` | **CLOSED.** Cookie suffixes remain lookup hints resolved through server-owned session reads before rendering. |
| `FIND-TASK-003-4` | **CLOSED.** The canonical sealing inventory includes stored browser envelopes, including expired rows, with exact-byte compare-and-swap rewrap. |
| `FIND-TASK-003-5` | **CLOSED.** Non-loopback plaintext remains refused and the production BFF journey retains a trusted-TLS private operation. |
| `FIND-TASK-003-6` | **CLOSED.** Keycloak/Dex switching, genuine callback parameters, and provider-replacement journeys remain present. |
| `FIND-TASK-003-7` | **CLOSED.** The previously cited Rust items retain substantive rustdoc. |
| `FIND-TASK-003-8` | **CLOSED.** Authoritative tenant identity remains server-returned and server-only. |
| `FIND-TASK-003-9` | **CLOSED.** The unused lifetime abstraction and dual SQL branch remain absent. |
| `FIND-TASK-003-10` | **CLOSED.** Ordinary proactive renewal refusal preserves only the issued token until exact stored expiry. |
| `FIND-TASK-003-11` | **CLOSED.** Discovery parsing and its fallible boundary remain accurately documented and tested. |
| `FIND-TASK-003-12` | **CLOSED.** Distinct chooser hints are deduplicated and resolved sequentially. |
| `FIND-TASK-003-13` | **CLOSED.** Only absence selects the loopback default; explicit empty or unsafe upstream configuration is refused. |
| `FIND-TASK-003-14` | **CLOSED.** Replay containment, ordinary refusal, and internal renewal failure retain distinct commit/rollback outcomes. |
| `FIND-TASK-003-15` | **CLOSED.** Shared API-key test support retains accurate ownership and panic documentation. |
| `FIND-TASK-003-16` | **CLOSED.** The renewal classifiers remain crate-private. |
| `FIND-TASK-003-17` | **CLOSED.** Missing or unopenable renewal credentials remain retryable internal failures with direct local proof and aligned documentation. |
| `FIND-TASK-003-18` | **CLOSED under the replacing R5 human direction.** The durable session chain root, existing User family lock, recursive descendant revocation, and browser wipe retire only that login's refresh chain without decrypting it; the separate same-User login remains active. |

`FIND-TASK-003-19` is not a reopening of an earlier identity or renewal
finding. It is the bounded sibling-topology regression introduced by the
authorized test-harness shutdown correction bundled into R5.

## Human-directed additions

| Direction | Validation |
|---|---|
| R1 conditional RFC 9207 issuer binding | **CLOSED.** Present issuer is exact-matched; absence is refused only for advertising providers; unrelated response parameters remain tolerated. |
| R2 real interactive connection test | **CLOSED.** Candidate testing uses the ordinary authorization-code/PKCE/nonce/ID-token boundary, stamps only the exact authorized revision, and creates no User, credential, completion, or browser session. |
| R5 replacement for FIND-18 | **CLOSED.** Logout revokes only the current login's refresh chain, preserves other browser/CLI/SDK chains, and leaves API-key logout browser-only. |

The human authorizations for R3, R4, and R5 are accepted as supplied and do
not change the retained harness regression's correction boundary.

## Verification limits

- This validation is source-based. It did not rerun the broad Cargo,
  Postgres, Keycloak/Dex, browser, or pnpm lanes already recorded by the
  implementation and discovery reports.
- The recorded green `test:wyrd` run proves the router-only Oracle teardown
  symptom was corrected; it does not exercise ordinary shutdown of a bound
  dedicated Forge worker.
- No focused proof currently observes worker-join versus storage-release
  ordering on the retained path. That missing proof is part of
  `FIND-TASK-003-19`, not a reason to broaden the finding or mark validation
  blocked: the ordering violation follows directly from the inspected
  producer, consumer, Bifrost abort contract, and Forge worker contract.

## Validation result

**FIX_REQUIRED**

The final ledger contains one bounded finding, `FIND-TASK-003-19`. Its minimum
correction reuses the existing serve-handle ownership fact and preserves the
R5 router-only drain. No specification, product, public API, security,
compatibility, concurrency-semantics, resource-ownership, or persistent-data
decision is required.
