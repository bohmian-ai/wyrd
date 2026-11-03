# System resilience review

Verdict: **FAIL**. Proposed material findings: SYS-1, SYS-2, SYS-3.

## Subject and coverage

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `e0354f9bdf8608046833029252d8a526e0bd999d`.
- Candidate: `5c560663e77ae07c33d198862ba6d795b656097a`; HEAD independently checked and unchanged during source inspection.
- First full review; no carried-forward acceptance evidence.
- Authority: root AGENTS, SQL AGENTS, `changes/spec-driven-development.md`, `agent_docs/index.md`, maintainer guide, architecture authority map, browser-session/local-password sections of `architecture/wyrd-design.md` and `architecture/wyrd-security-posture.md`, operations readiness and deployment browser-session sections, approved spec revision 19, original TASK-004-REV-03.
- Scope: deployed local-auth availability, shared admission and durable state, cancellation, dependency failure/restart, and changed `dev:full` readiness/lifecycle. This is not an audit of unrelated existing lifecycle code.
- CodeGraph was used before source searches, including password owner/callers and timeout/lifecycle queries. Actual cumulative Git diff and surrounding owners/tests were inspected independently.
- Reports only. No source changes, Cargo/build/codegen, generated artifacts, credential reads, raw development output, or live development-password capture.

## Deployed paths and topology

| Path | Owners and propagation | State and availability boundary |
|---|---|---|
| Browser password login | BFF login action forwards to Rust `/auth/browser/password/login`; HTTP handler uses `BrowserSessions::password_login`; tenant lookup, failure admission, local credential read; process KDF semaphore; tenant transaction rechecks credential revision and creates existing opaque session | Rust replicas share Postgres counters, Users, grants and sessions. KDF semaphore is per process. No Node authority/cache or sticky routing added. |
| Password change | Selected cookie resolves through existing BrowserSessions; current password uses same Windows/check_password; replacement uses hash_password; locked revision recheck and revision advance commit together | Old password sessions fail their next SQL revision check on every replica. Independently owned API keys remain outside this transition. |
| Password reset/signup | Format/policy checks, existing tenant boundary; bounded hash; transactional one-use code consumption and password/session/User/grant mutations | SQL rollback prevents partial durable membership/reset consumption on failure. Successful reset advances revision and creates no session. |
| Other APIs and auth | `http/router.rs` merges auth routes and protected APIs then applies existing protective edge; every password handler inherits capacity/timeout/body limits. `/healthz` and `/readyz` remain outside this stack | Password work shares server CPU/memory and Tokio blocking resources with other capabilities. A failed password operation should refuse that request, not shut down the shared server. |
| Development | `dev:full` creates process groups for MockServer, repository Postgres/Rust wrapper, UI and canonical Python app; waits for new dev-login record, Rust readiness and UI login response before app setup/batch | Batch/setup rejection deliberately keeps ready services alive. Exit/interrupt trap stops app first and waits, then signals remaining groups. The Rust wrapper removes ephemeral login record during exit cleanup. |

Existing owners were extended: BrowserSessions, TenantConn, auth User/password_hash, auth session table and existing lazy tenant-write expiry purges. No separate scheduler, identity service, cache, durable session owner or development telemetry endpoint was introduced. `scripts/dev/example-app.sh` and the existing app-first trap remain the canonical source/wheel and drain owners.

## Failure and recovery assessment

| Failure/recovery | Evidence and consequence | Assessment |
|---|---|---|
| Postgres unavailable before/during local auth | All SQL/pool errors in password/login/change/reset paths propagate through `store_error`; BrowserSessions::resolve likewise refuses rather than using cached authority. TenantConn drop rolls back uncommitted work. | Request-level unavailable failure; no new shared-process termination found. Existing store-outage test closes app pool in `pg_browser_sessions.rs:575–576`; new local operations are source-traced but not independently outage-tested here. |
| Restart/rolling replica replacement | Local credentials/revisions, failure windows and session digests live in Postgres. `live_browser_session` joins current User revision; BrowserSessions::resolve resolves current principal/grants. No process-local authority is required to recover a session. | Source supports cross-replica persistence and revocation. New local tests run one in-process server; explicit task-required local change/reset A → old cookie B proof is absent from inspected tests. Production multi-replica tests cover SSO/recovery, not the new local lifecycle. No restart guarantee is inferred solely from a green healthy-path test. |
| Cancellation during KDF | Existing edge timeout drops the handler while `spawn_blocking` can continue; permit is owned by the dropped future. | FAIL, SYS-1. |
| Parallel guessing, including across replicas | Admission reads committed failures and commits before KDF; concurrent requests consume no durable admission slot. | FAIL, SYS-2. |
| Fixed failure-window expiry | Statement-time deadlines, conditional expired-window restart, and RLS-scoped purge at next failure write in local_passwords.rs. Similar invitation/reset/session purges run at existing write boundaries. | Reuses existing auth cleanup pattern; no unsupported new background-maintenance obligation invented. Restart cannot erase counters; window is not extended on every failure. |
| Setup/batch failure after services ready | `mise.toml:1327–1349` branches preserve server/UI/MockServer and report failure; final wait keeps environment inspectable. | Source preserved; lifecycle test explicitly exercises rejected setup. A failed batch branch is source-supported, not executed by this reviewer. |
| UI startup failure | New readiness predicate requires UI, but loop checks only Rust child. | FAIL, SYS-3. |
| Ctrl-C after readiness | Existing trap sends TERM to app group and waits before signalling server/UI/MockServer groups; wrapper removes dev-login record; process groups retain child ownership. | Source preserved. Existing lifecycle test asserts stopped owned listeners and removal of login record. No new drain mechanism needed. |

## Proposed findings

### SYS-1 — KDF permits are released while cancelled blocking work still runs

- Classification: INCORRECT; material resource-admission failure.
- Obligation: approved spec abuse policy and task lines 88–98 require at most four KDF operations per process; task Scenario 2 explicitly preserves app/auth availability under bounded hashing work.
- Exact changed location: `crates/wyrd/wyrd-auth/src/passwords.rs:229–235`, especially permit at 232 and `spawn_blocking(work)` at 233.
- Producer-to-consumer proof: `hash_password` and `check_password` both use `bounded`; signup/login/change/reset and setup reach those functions. `http/router.rs` merges auth routes into `routed` and applies `apply_protected_edge` at its `protected` construction. `EdgeTimeout::call` in `http/middleware/edge_timeout.rs:159–164` drops the in-flight response future on timeout. `_permit` belongs to `bounded`'s async future, so drop returns it immediately. The blocking closure captures only `work`; it neither holds nor returns the permit. Tokio's locally installed source (`tokio-1.53.3/src/task/blocking.rs:106–118`) explicitly documents that started blocking work cannot be aborted and continues during shutdown/cancellation.
- Failing scenario: four password operations start blocking derivation; their callers time out or are cancelled before the blocking work completes; replacement requests acquire the now-free permits and start more work while the original four continue. A configured short edge deadline or slow/contended host makes this path reachable without any malformed input. Current permits bound surviving waiters rather than running KDFs.
- Observable consequence: more than four Argon2 jobs and their memory allocations can overlap in a shared Rust server. Request cancellation defeats the approved resource ceiling and can hurt unrelated auth/API work. There is no need to crash or fail closed at process scope.
- Smallest correction: keep the existing process semaphore and blocking-pool owner; transfer permit lifetime into the blocking closure so only completion of the actual work releases it. No queue/framework/new dependency or public contract change is required. Preserve existing errors and secret handling for both hash and verify callers.
- Focused closure proof: deterministic in-module test using the existing bounded owner with four barrier-controlled blocking closures; cancel the awaiting tasks after all closures start, then show a fifth cannot enter until one actual closure finishes. This needs no real Argon2 timing, synthetic host load, server stack or additional harness.

### SYS-2 — Failure counters do not atomically admit password work

- Classification: INCORRECT; material cross-replica guessing/admission failure.
- Obligation: task lines 88–98 and spec abuse policy require ten tenant/name failures and one hundred trusted-source failures per fixed fifteen-minute window, shared Postgres state and atomic updates, with rate admission before KDF allocation.
- Exact changed locations: `passwords.rs:534–548`, `passwords.rs:608–624`, `Windows::admit` at 738–755 and `Windows::refuse` at 760–772; SQL `attempt_refusal` in `queries/auth/local_passwords.rs:227–249`.
- Producer-to-consumer proof: `admit` issues only a SELECT. Login then commits its transaction before entering `check_password`. Change follows the same read/commit/check pattern. Failed work updates the counter later through `refuse`; the upsert is atomic, but admission is not. No row lock/reservation/serialization crosses that gap, and KDF-permit acquisition does not recheck either window. All replicas read the same stale admitted state legitimately.
- Failing scenario: account has nine failures. A burst of requests across one or more replicas all reads nine and commits while KDF slots are busy. Every request is now entitled by local control flow to run a password guess, including those still waiting after the first failure advances the shared counter to ten. The burst can be far larger than the remaining one attempt; repeating with distinct usernames similarly bypasses the source allowance. Cancellation before post-KDF recording also leaves no durable admission charge.
- Observable consequence: attacker-controlled parallelism expands the approved guessing budget and leaves already-admitted work consuming the shared KDF pool after the shared limit was reached. Merely counting eventual failures exactly does not close the admission boundary.
- Smallest correction boundary: existing shared Windows/SQL auth-attempt owner, used by both login and password change. Make admission of a verification atomically consume the available fixed-window budget before expensive work, preserving the documented failure semantics, expiry and bounded cancellation recovery. Rechecking only on each HTTP route, adding process-local locks, or changing the final upsert alone cannot close cross-replica admission. No new external service is justified; implementation may need a small forward schema/query change at this existing owner.
- Focused closure proof: synchronize concurrent attempts from two existing Wyrd replicas at one remaining account/source allowance, and prove the remaining budget bounds which work is admitted; assert stable 429/Retry-After, fixed window end, recovery after expiry, and cancellation cannot leave a permanent lock. Existing `local_signup_is_tenant_bound_and_rate_limited` tests ten sequential failures only and does not exercise this gap.

### SYS-3 — UI startup exit leaves development startup waiting forever

- Classification: REGRESSION; changed development startup failure propagation.
- Obligation: REQ-043/AC-026 inspectable normal development workflow and preserved owned-child lifecycle; the candidate newly makes UI sign-in readiness a prerequisite for completing startup.
- Exact changed location: `mise.toml:1314–1320`.
- Proof: UI is started as a background child; `pids` stores its PID. The new readiness condition requires `$ui/t/dev/login`, but the loop checks only `$server`. If `pnpm install --frozen-lockfile` fails or UI process exits before binding, the background child's failure does not trigger parent `set -e`, Rust remains healthy and every iteration sleeps. `wait -n` at 1350, which would propagate the failed child and invoke cleanup, is unreachable. This is a normal startup dependency failure, not an invented deployment topology.
- Observable consequence: `mise run dev:full` hangs indefinitely with live Rust/Postgres/MockServer children and never reaches a ready environment, a batch failure report, or automatic cleanup. The failed component cannot recover without the developer interrupting/restarting.
- Smallest correction: retain the current process groups and readiness gate, but observe the already-owned required UI child while waiting; report its startup failure and exit through the existing trap. No new supervisor or general retry/timeout machinery is needed. Preserve deliberate post-readiness setup/batch inspectability and app-first drain.
- Focused closure proof: exercise the existing dev lifecycle owner with UI startup forced to exit before readiness; assert bounded nonzero parent completion and cleanup of owned children. Keep successful early readiness and rejected setup behavior intact. Do not record live passwords or raw launcher output in review artifacts.

## Verification limits and requested proof

This report is source/diff analysis. No Cargo/build/generator or live development run was performed; the orchestrator owns sequential checks. Existing new password tests cover policy, correct/incorrect credentials, sequential throttling, local revision lifecycle and concurrent invitation redemption; they do not prove KDF cancellation, concurrent admission, or failed UI startup. Existing production auth source supports SSO/replica regression coverage but does not contain the task's requested local password replica scenario.

The implementation test commands are in task V1–V12 and remain orchestrator-owned. None of their existing selectors directly reproduces SYS-1/SYS-2/SYS-3, so rerunning healthy-path tests alone would not resolve these source findings. Focused new closure assertions above are sufficient; broad gates or a new load harness are unnecessary.

No unrelated hardening, alternate identity/session service, background scheduler, or speculative recovery requirement is proposed. Remaining risks are the three reachable failures above and the stated unexecuted recovery proof.
