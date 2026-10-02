# Concurrency and durability domain review

## Immutable subject

- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84`
- Authority: approved `changes/active/oidc-production-readiness/spec.md` revision 7; `TASK-004-laptop-clients.md`; repository rules; and the three supplied TASK-003 human directions. The logout direction is directly applicable: logout may retire only the selected login's refresh chain. The issuer-binding and real-sign-in-test directions do not change this review's concurrency or durability boundary.
- Candidate stability: `HEAD` was the requested candidate before and after inspection.

## Reviewed boundary and source coverage

| Boundary | Producer, writer, and consumer paths inspected | Result |
|---|---|---|
| Saved-login serialization | `SavedLogins::{save,renew,begin_logout,finish_logout}`, stable `.lock` file, reread under lock, and `SavedLoginSource::mint` in `crates/shared/wyrd-client/src/saved_login.rs`; callers in `ClientConfig::resolve_credential`, CLI login/logout, and test helpers | The OS lock covers each record mutation; renewal writes `RefreshPending` before network IO and writes the incremented `Ready` generation only after a successful response. Logout writes a token-free tombstone before remote revoke and deletion. Atomic temporary-file replacement plus file and directory `fsync` is present. |
| Cache coherence across processes | `AuthMiddleware::{bearer,force_refresh,mint_into}` in `crates/shared/wyrd-client/src/auth.rs`; `ResolvedCredential::Renewable` and `SavedLoginSource` | **Fails:** `bearer` returns a fresh cached token without invoking the saved-login owner, so generation and tombstone changes are not revalidated. See `DCD-1`. |
| Crash, timeout, refusal, and cancellation | `SavedLogins::renew`; `TokenExchange::exchange`; blocking mint lifecycle and retained `pending_mint` in `AuthMiddleware`; `concurrent_saved_renewal` | Source leaves `RefreshPending` for client transport/timeout failures and for server refusals, preventing replay. A cancelled async waiter does not cancel the blocking mint, and later callers join it. The required actual uncertain-timeout proof is absent. See `DCD-2`. |
| Logout race and remote failure | `SavedLogins::{begin_logout,finish_logout}`; CLI `logout`; `CliLogins::end`; `revoke_refresh_chain`; sequential per-chain server test, provider-backed offline-logout journey, and local renewal/logout race | Tombstone serialization prevents a later local renewal from resurrecting the record. Remote failure still deletes the local secret and emits a warning. Server revocation follows `rotated_from` descendants under the existing family lock, leaving a separate login chain intact. |
| One-use SQL handoff | `CliLogins::{begin,claim,claim_in,cancel}`; `lock_cli_handoff`, `delete_cli_handoff`, `redeem_login_completion`; login-state consume/complete/redeem queries; callback completion | Claim locks the handoff row and deletes the sealed completion plus handoff in the same tenant transaction. Concurrent claim or cancel serializes on the same handoff row; only the committing winner can return credentials. Callback completion races either leave a completed row for a later claim or observe cancellation and roll back issuance. |
| Tests asserting durable outcomes | saved-login unit tests; `concurrent_saved_renewal`; `logout_revokes_only_its_own_chain`; `cli_oidc_handoff_journey`; sequential wrong-verifier/cancel/replay tests | Multi-process rotation proves one generation increment and a still-renewable winner. Logout proves tombstone deletion, offline deletion, and per-chain scope. Handoff replay is proven sequentially; concurrent claim/cancel/callback behavior is supported by transaction/lock inspection but not directly scheduled by a test. |

## Material proposed findings

### DCD-1 — INCORRECT: fresh in-memory cache bypasses required generation revalidation

- **Violated obligation:** TASK-004 lines 74–85 require every process to reread the on-disk generation and explicitly state: "An in-memory cache must revalidate generation before reuse across processes." REQ-012 requires concurrent local clients not to replay or overwrite newer renewal state and permits logout/revocation of the saved session.
- **Exact location:** `crates/shared/wyrd-client/src/auth.rs:520-530` returns `CachedToken.access_token` immediately for a fresh `ResolvedCredential::Renewable` cache entry. `crates/shared/wyrd-client/src/saved_login.rs:596-620` is the only path that locks and rereads the saved record, and it is not called on that cache hit. The cached value contains only token and expiry (`auth.rs:88-99`), not the generation read from disk.
- **Evidence and reachability:** `ClientConfig::resolve_credential` constructs a renewable saved-login source (`config.rs:203-209`), and all SDK calls share its `AuthMiddleware`. After that process mints once, another process can renew, replace the login, or persist `LoggedOut`; until the first process's bearer reaches the ordinary 30-second stale boundary, `AuthMiddleware::bearer` returns its old token without reading the record. The four-process journey creates a new client per child and therefore never exercises a long-lived client's cache after another process changes the generation.
- **Observable consequence:** a long-running Rust, Python, or TypeScript client can continue presenting an access token cached from an older saved-login generation after another process has renewed or logged out that record. This is the precise cross-process stale-cache state the task prohibited; reactive `401` is not generation revalidation and does not help while the old access token remains valid.
- **Required testable correction:** keep cache coherence in the existing `SavedLoginSource`/`AuthMiddleware` ownership boundary and make a renewable cache hit validate the record's current generation/state before returning it. Reuse the existing stable record identity and lock/read mechanism; do not add a second credential cache. Add a focused multi-process proof in which one long-lived client first caches a token, another process advances the generation and separately tombstones the record, and the first client observes each change rather than returning its unvalidated cached bearer.

### DCD-2 — MISSING: the required uncertain-timeout recovery journey is not exercised

- **Violated obligation:** TASK-004 lines 74–90 require a crash or uncertain timeout to leave `RefreshPending`, never retry the token, and require login. Its verification contract at lines 168–171 says `concurrent_saved_renewal` asserts fail-closed crash/**uncertain-timeout** behavior.
- **Exact location:** `crates/shared/wyrd-client/tests/pg_auth_e2e_against_fixture.rs:200-215` implements `leave_renewal_pending` by directly replacing a `Ready` fixture with `RefreshPending`; lines 262–265 then prove only that an already-pending record is refused. No test interrupts a renewal after the durable transition, produces a transport timeout with an uncertain server outcome, or observes the number of refresh requests.
- **Evidence and reachability:** production source writes pending before `TokenExchange::exchange` (`saved_login.rs:401-413`) and maps client transport failures to `refresh_pending` (`saved_login.rs:422-428`), but the named journey bypasses both operations. The implementation evidence itself concedes that only the preconstructed crash state is driven.
- **Observable consequence:** the required proof would stay green if a later change moved the pending write after network IO, retried on timeout, or restored `Ready` following an uncertain response. The current test establishes the pending-state consumer, not the transition and recovery boundary that prevents rotated-token replay.
- **Required testable correction:** extend the existing concurrent saved-renewal journey to drive one real renewal through the production client, make its outcome uncertain after the durable pending transition (including the case where the server may have accepted it), then start a separate process and prove it refuses without issuing a second refresh request or overwriting the record. Keep the existing manually-pending assertion only as a narrow state-consumer check if it remains useful.

## Verification limits

- I did not rerun the provider-backed identity lane. The task records it as green, but those reported results do not close `DCD-1` or `DCD-2` because their assertions do not create either state.
- The 30-second lock-timeout branch has no located runnable assertion despite the implementation evidence claiming unit coverage; source inspection shows it returns `lock_timeout` without fallback. This is a proof limit, not a separate domain finding because the task's named concurrent journey does not expressly require a timed lock-contention case.
- Simultaneous handoff claim/cancel/callback scheduling is not directly tested. The reviewed SQL uses one transaction and a common handoff-row lock/delete order, so no reachable atomicity defect was found.
- File ownership and symlink TOCTOU hardening belongs to the separate credential/security review. This report evaluated their effect on durability only.

## Overall result

**FAIL**

The persisted renewal state machine, per-record OS locking, atomic replacement, logout tombstone, per-chain server revocation, and SQL one-use handoff are coherent. Acceptance is blocked by the live cache path that bypasses explicit generation revalidation and by the missing required uncertain-timeout transition proof.
