# TASK-004 system-resilience review

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84`
- Authority: approved `changes/active/oidc-production-readiness/spec.md` revision 7; `TASK-004-laptop-clients.md`; the three supplied TASK-003 human directions, including the binding decision that logout revokes only the selected login's refresh chain.
- Repository authority read: `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `TESTING.md`, spec-driven development, and maintainer style.

The candidate commit remained `7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84` throughout this review.

## Deployed paths and recovery assessment

| Changed path | Failure or interruption traced | Result and recovery evidence |
|---|---|---|
| CLI begin -> server auth router -> `CliLogins::begin` -> Postgres handoff -> ordinary OIDC login | IdP discovery outage, database outage, server restart, connection replacement | IdP/DB failures remain request-scoped typed errors; they do not crash the shared server. Handoff and login state are Postgres-backed and sealed with the deployment keyring, so a surviving keyring lets another replica continue. `HumanConnections::begin_login` rechecks that the handoff is live, and a connection change is refused. A begin failure after the handoff insert leaves only an unclaimable expiring row. |
| Browser callback -> sealed completion -> CLI claim | Callback/claim on different replicas, replay, wrong/expired verifier, audit failure | Tenant RLS, the locked handoff row, and `redeem_login_completion` serialize the durable state. Successful claim deletes handoff and completion in the same transaction as required audit; an audit failure rolls the transaction back. Wrong, expired, cancelled, and replayed claims return no credential. The shared auth governor bounds handoff polling with the other auth routes. |
| CLI polling and Ctrl-C cancel | Delayed callback, process interruption, server unavailable during cancel | The CLI follows the server's two-second bounded retry hint. Ctrl-C attempts the idempotent cancel and never saves a credential; inability to reach the server is reported and the local process exits. `delete_cli_handoff` removes both the handoff and its bound login state; a callback that already consumed state cannot later commit a completion after cancellation. No focused cancellation journey was supplied. |
| Saved-login renewal -> OS lock -> `RefreshPending` -> `/auth/token` -> atomic `Ready` | Concurrent processes, client timeout/cancellation, client crash, server/DB outage | The stable lock serializes processes. The pending record is file- and directory-synced before network IO. A transport timeout or crash leaves `RefreshPending`, and later processes refuse to replay it. A cancelled async waiter does not start a second refresh: `AuthMiddleware::pending_mint` retains the blocking mint for the next waiter. A successful winner persists the rotated pair and generation before releasing the lock. The remaining in-process cache defect is SYS-001. |
| Logout -> local tombstone -> `/auth/revoke` -> local delete | Concurrent renewal, local crash, server outage, refresh racing logout | The same record lock makes the tombstone precede revocation. Renewal cannot rewrite a tombstone. Server revocation uses the principal-family advisory lock and `revoke_refresh_chain`, so a successor committed by a racing rotation is included while other login chains remain valid. Server failure still deletes the local secret and emits a warning. This conforms to the binding human direction. |
| Unsafe/corrupt local store | Symlink, foreign/open Unix ownership or mode, malformed record, interrupted replace | Selection fails closed and does not fall through to another saved tenant. Temp-file sync, atomic replacement, and directory sync protect crash recovery. Unix unsafe-mode/corruption paths have direct tests; equivalent Windows ACL/ownership behavior is not evidenced. |
| Rust/Python/TypeScript public SDKs | Binding/runtime failure and revoked/expired saved login | The language bindings construct the shared Rust `ClientConfig` and shared auth owner; they do not duplicate refresh state. The three journeys exercise allowed/denied calls, renewal, explicit override, revocation, and multi-tenant records. Shared-client failures propagate as stable client errors rather than terminating the embedding process. Tenant enforcement outside saved-login selection remains defective as SYS-002. |
| Identity-lane wiring | Renamed selector, omitted provider-backed journey, production wheel accidentally containing test harness | Each new target proves exactly one test selection before execution; the unfiltered lane runs CLI, Rust, shared-client concurrency, Python, and TypeScript after the server/UI journeys. Python and TypeScript general integration lanes exclude only the Keycloak-dependent identity tests. The wheel-boundary check assessment is below. |

## Material findings

### SYS-001 — A live process reuses an in-memory saved-login token without revalidating the durable generation

- Classification: `INCORRECT`
- Violated obligation: TASK-004's renewal contract requires: "An in-memory cache must revalidate generation before reuse across processes." It also requires a concurrent logout or unsafe renewal not to be hidden by a stale process-local view.
- Location: `crates/shared/wyrd-client/src/auth.rs:520-529`; `crates/shared/wyrd-client/src/saved_login.rs:300-306,364-400,596-620`.
- Evidence: `AuthMiddleware::bearer` returns any non-stale `CachedToken` immediately. `SavedLoginSource` holds no observed generation, and its disk reread occurs only when `mint()` is called after the middleware cache becomes stale or a 401 forces refresh. Therefore process A can populate its cache, process B can rotate, tombstone, delete, or replace the record, and process A will continue reusing its cached token without observing the changed generation/state.
- Observable system consequence: long-running Rust, Python, or TypeScript processes do not converge on the winning cross-process generation and do not observe local logout/store state at the required reuse boundary. The separate-process journey starts a fresh client in every child, so it cannot expose this path.
- Testable correction: make the shared saved-login credential owner validate the record's current generation and usable state before accepting a process-local cached token, while preserving the existing single-flight mint and bounded access-token semantics. Add one long-lived-client test: cache a token in process/client A, mutate the durable record through process/client B (successful rotation and logout/tombstone), then prove A observes the new generation or fail-closed state before reusing its cache and never replays the old refresh token.

### SYS-002 — `ClientConfig.tenant` is ignored by explicit, access-token, API-key, and credentials-file authorities

- Classification: `INCORRECT`
- Violated obligation: TASK-004 lines 60-69 require every resolved authority to match a supplied tenant selector at exchange/authorization or fail, without falling through. REQ-012/REQ-015 require operation only as the intended server tenant.
- Location: `crates/shared/wyrd-client/src/config.rs:194-212`; `crates/shared/wyrd-client/src/transport/credential.rs:160-169,232-250`; downstream `AuthMiddleware::bearer` and API-key exchange.
- Evidence: `resolve_credential` returns the first explicit/environment credential before saved-login selection. `ExplicitToken` carries no selector check; `ApiKey` exchange carries no configured selector; the credentials-file floor is likewise returned without one. Only workload routing and `SavedLogins::select` use `self.tenant`. The task's own implementation evidence records this as a ceiling: "The tenant selector is not enforced on the env-variable credential tiers."
- Observable system consequence: a caller configuring tenant B can silently authenticate as an explicit or ambient tenant-A authority. Routes whose tenant comes solely from the verified token can then operate in or return tenant A instead of failing the caller's intended tenant selection. Server RLS prevents privilege escalation into B, but it does not prevent this wrong-tenant operation.
- Testable correction: preserve credential precedence, but carry the configured tenant constraint through the one shared credential/exchange path and reject a bearer or exchanged token whose verified/issued tenant differs before it is used for an application request. Prove the behavior for explicit bearer, explicit/API-key environment and credentials-file sources, with a positive same-tenant control and no fallback to saved or file credentials after mismatch.

## `check:py-wheel-no-testing` assessment

Commit `feac127a0e762c757bdb1db0fdf0cb2f8a39cb15` strengthens the boundary check.

Before that commit, the task depended on `py:setup`, which installs `wyrd-sdk-python` with the `testing` feature, and then imported from that development environment. It therefore did not inspect the production artifact it claimed to protect. The candidate now:

1. builds a wheel with the default `pyproject.toml` feature set (which includes `python` but not the crate's `testing` feature);
2. creates a fresh temporary output directory;
3. installs/imports that exact wheel through `uv run --isolated --no-project --with "$wheel"`;
4. first requires `import wyrd` to succeed, preventing a broken/unloaded wheel from making the negative import vacuously green; and
5. fails if `import wyrd.testing` succeeds in the same isolated wheel environment.

This changes the check from the wrong testing-enabled subject to the shipped default wheel and adds a positive control; it does not broaden an allowlist, suppress a failure, or skip package loading. A direct `mise run check:py-wheel-no-testing` completed successfully against the candidate during this review (`Finished in 147.15s`), including the default-wheel build and both isolated imports.

## Verification limits

- The supplied evidence reports every TASK-004 identity target and the unfiltered identity lane green, plus CLI/shared/SDK, Python, TypeScript, codegen, boundary, format, and lint lanes. I did not treat those reports as proof of the two unexercised paths above.
- There is no focused Ctrl-C/cancel journey, lock-timeout journey, actual HTTP-timeout renewal journey, or long-lived-client generation-change journey. The crash-state test manually writes `RefreshPending`; it establishes the recovery response but not the interruption timing itself.
- The CLI handoff is durably replica-neutral in source, but the existing two-replica journey is the BFF/UI topology rather than an explicit begin-on-replica-A/claim-on-replica-B CLI test.
- Private ownership/mode checks are proven on Unix only; no Windows ACL evidence was supplied.

## Overall result

`FAIL`

The server handoff, request-scoped outage behavior, durable refresh-pending protocol, per-login logout revocation, shared language ownership, identity-lane wiring, and strengthened production-wheel check are resilient. SYS-001 and SYS-002 leave required cross-process convergence and tenant-intent enforcement unsatisfied.
