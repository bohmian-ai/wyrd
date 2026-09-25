---
id: TASK-004
kind: implementation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-011, REQ-012, REQ-015, REQ-016, INV-001, INV-002, INV-003, INV-005, AC-004, AC-007, AC-009]
depends_on: [TASK-002]
---

# CLI login and local SDK user credentials

## Outcome and Value

A person runs wyrd auth login once, completes browser SSO, and then uses Rust, Python, or TypeScript SDKs locally as that tenant User without pasting tokens or contacting the IdP for routine calls.

## Owners, Scope, Consumers, and Prohibited Changes

The Wyrd server owns the one-time login handoff and renewal decisions; wyrd-cli initiates login and manages the user-protected saved credential; wyrd-client owns selection, token exchange, rotation, and caching for all three SDKs. Language packages project this shared behavior only. A script's browser session is not an SDK credential. Do not add separate language-specific credential stores, expose refresh tokens to URLs/terminal output, use provider tokens as Wyrd authority, or turn a human login into a workload identity.

## Approach

1. Provide a short-lived one-time handoff bound to the initiated CLI login, using the same tenant Web provider registration.
2. Save renewable Wyrd user authority with server and tenant identity; support logout and revocation.
3. Resolve explicit credentials first, then the correct local user login; fail on ambiguous tenant selection.
4. Renew rotating human credentials safely across concurrent local processes and expose the shared behavior through Rust, Python, and TypeScript.

## Packet-local handoff and local authority contract

The server owns a one-use handoff row bound to `tenant_id`, connection ID,
random handoff ID, hash of a 256-bit CLI-held verifier, creation/expiry (at
most five minutes), and completion state. `POST /auth/cli-handoffs` accepts
tenant route key as pre-login routing context and returns
`{handoff_id, login_url, poll_verifier: Secret, expires_at}` over TLS. The CLI
opens `login_url`; TASK-002 binds `handoff_id` in one-use login state. After
provider callback, the server stores Wyrd credentials under the handoff and
shows only a generic browser success page. No provider code, Wyrd token, or
poll verifier is in the browser URL/page. CLI polls
`POST /auth/cli-handoffs/{id}/claim` with the verifier. Pending returns a
bounded retry interval; complete atomically consumes the handoff and returns
`{server_origin, tenant_id, principal_id, access_token, refresh_token,
access_expires_at}` only to the verifier holder. Wrong verifier, tenant,
expiry, second claim, and cancellation return typed refusals and no tokens.
Throttle polling and audit completion/refusal. No second IdP app is used.

`wyrd-client` owns a versioned user credential record keyed by canonical
server origin and stable tenant ID: `{format_version, origin, tenant_id,
tenant_key, principal_id, access_token, access_expires_at, refresh_token,
generation}`. The CLI writes it under the user's config directory with a
private directory and file, rejects symlinks/unsafe ownership or permissions,
and uses atomic replacement. `wyrd auth logout` removes the selected local
record and asks the server to revoke its refresh family; local deletion still
occurs if the server is unavailable, with a clear revocation warning.
Different tenants on one server remain separate records. Never print tokens
or secrets; `login status` may show only origin, tenant, principal, and expiry.

Extend `ClientConfig.tenant` and public Rust/Python/TypeScript client options
with the same optional tenant key/ID selector. Resolve explicit `credential`
first, then existing `WYRD_ACCESS_TOKEN`, `WYRD_WORKLOAD_TOKEN` with tenant,
`WYRD_API_KEY`, then the matching saved human login, then the existing
`credentials.toml [default].api_key` floor. Existing source order is
preserved; adding saved human login is one tier. If a tenant selector is
provided, every resolved authority must match it at exchange/authorization
or fail, never fall through to another source. Without a selector, saved
human login resolves only when exactly one record matches that canonical
server origin; zero falls through and multiple fail ambiguous. The explicit
credential still wins. Python and TS constructors expose `tenant` alongside
their existing `server_url/serverUrl` and `credential` options; both delegate
to `wyrd-client` and update public types/stubs from source.

For renewal, all processes take an exclusive OS lock on a stable per-record
lock path, then reread the on-disk generation and refresh token. Before any
network refresh, persist `RefreshPending { generation, started_at }` by fsync
plus atomic replace while holding the lock; its refresh token is sealed in
that record but must never be sent again by a later process. The owner uses
the token once, persists the returned access/refresh pair and incremented
`Ready` generation by fsync plus atomic replace, then releases the lock.
An in-memory cache must revalidate generation before reuse across processes.
A loser observes the winner's new generation and does not replay its old
token. Crash or uncertain timeout leaves `RefreshPending`; a later process
must fail closed and require login because it cannot know whether the server
accepted the old token. It never retries that token or silently overwrites a
newer generation. Logout takes the same per-record lock, first persists a
`LoggedOut` tombstone, then revokes remotely and deletes the secret. A
concurrent renewal cannot recreate a logged-out record; a crash after the
tombstone still blocks reuse. Lock timeout, corrupt/unsafe store, revoked
family, and wrong-tenant responses likewise fail without fallback to another
tenant's record. Routine API requests contact Wyrd, not the IdP.

## Ordered Implementation Scenarios

### Scenario 1 — Browser to CLI handoff

**Behavior.** The system browser completes SSO; the CLI receives Wyrd user authority through a bound, expiring, one-use handoff without callback paste, redirect token, printed token, or second IdP app.

**RED.** Add a real-server CLI journey with success, replay, expiry, and wrong-initiator cases; observe the current pasted-callback flow or an accepted replay.

**GREEN.** Complete server handoff and CLI flow; rerun the journey.

**REFACTOR.** Reuse the server's login state and shared token-exchange path without a second issuer trust model.

### Scenario 2 — Local selection and override

**Behavior.** SDKs choose saved authority for the intended server and tenant, reject ambiguous same-server tenant selection, and honor an explicit credential override. Logout/revocation removes further renewal.

**RED.** Add focused shared-client tests and a real-server two-tenant SDK journey; observe no local user source or a wrong-tenant selection.

**GREEN.** Add the credential-store integration in wyrd-client and CLI management; rerun both scenarios.

**REFACTOR.** Preserve one credential chain for every language SDK.

### Scenario 3 — Automatic and concurrent renewal

**Behavior.** Long-running and separate local scripts renew short-lived Wyrd access without IdP visits. Concurrent clients sharing a saved login neither replay a rotated refresh token nor overwrite newer renewal state; unsafe renewal fails and permits login again.

**RED.** Add a concurrent-process renewal journey with the real server and negative expired/revoked credential cases; observe refresh-family replay or stale write.

**GREEN.** Complete shared safe renewal and persistence; rerun prior scenarios.

**REFACTOR.** Keep token refresh and cache logic in wyrd-client, with no Python/Node duplicate.

### Scenario 4 — First-class language journeys

**Behavior.** Rust, Python, and TypeScript public clients use a CLI-established user credential to make an authorized call and a denied call, renew, and fail after revocation. A machine credential still overrides when supplied explicitly.

**RED.** Add one client-to-server journey per language through each public SDK; observe missing ambient user auth.

**GREEN.** Project the shared client and update generated/public types; rerun all scenarios.

**REFACTOR.** Keep language bindings thin and public imports stable.

## Acceptance Criteria

AC-004 passes for all three SDKs, including same-server multi-tenant selection, override, replay safety, expiry, and revocation. Neither provider code nor Wyrd tokens appear in CLI output, callback URLs, or Python/TypeScript page data.

## Expected Write Set and Consumer Closure

Likely wyrd-server handoff contract, wyrd-spec, wyrd-cli, wyrd-client auth/config, Rust SDK, Python PyO3/package/stubs, TypeScript N-API/package/declarations, language journeys, and identity-lane wiring in mise.toml. Generated artifacts come from their owning source.

## Verification and Evidence

Run every newly named Rust/Python/TypeScript test with its exact focused mise exec command and owning setup. Extend the repository-managed mise run test:identity:journey lane with the provider-backed CLI and all three SDK journeys, then run it. Broader lanes: mise run test:cli:journey; mise run test:shared; mise run test:wyrd-sdk; mise run py:test:unit; mise run py:test:integration; mise run py:typecheck; mise run ts:test:unit; mise run ts:test:integration; mise run ts:typecheck; mise run ts:napi:check; mise run codegen:check; mise run check:client-tier; mise run check:pyo3-scope; mise run fmt; mise run lints; mise run py:format; mise run py:lints. Include the production-wheel boundary check when Python exports change. Retain redacted three-language and concurrent-renewal evidence.

Planned selectors: `cli_oidc_handoff_journey` in existing `wyrd-cli
--test cli`; `saved_user_auth_journey` in existing `wyrd-sdk-rust --test
cards_state`; `test_saved_user_auth_journey` in Python
`tests/integration/auth/test_saved_user_auth.py`; `saved user auth journey`
in TypeScript `tests/integration/saved-user-auth.test.ts`; and
`concurrent_saved_renewal` in existing `wyrd-client --test
pg_auth_e2e_against_fixture`. With the identity lane's
Postgres/IdP setup, focused commands are:

```text
mise exec -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored=all -E 'test(=cli_oidc_handoff_journey)'
mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test cards_state --run-ignored=all -E 'test(=saved_user_auth_journey)'
mise exec -- cargo nextest run --locked -p wyrd-client --test pg_auth_e2e_against_fixture -E 'test(=concurrent_saved_renewal)'
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/auth/test_saved_user_auth.py::test_saved_user_auth_journey
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/saved-user-auth.test.ts -t 'saved user auth journey'
```

The CLI test proves browser handoff success, wrong initiator, expiry, replay,
and no token in URL/output (AC-004/007). Each public SDK journey uses the
CLI-created record for an allowed and denied API call, refresh without IdP,
revocation failure, explicit override, and two same-server tenant records
(AC-004). The concurrent test starts separate processes against the real
server and asserts one rotation, winner generation, no replay-family
revocation, fail-closed crash/uncertain-timeout/unsafe-store behavior, and
logout racing renewal never restores a saved record (AC-004). Extend
`test:identity:journey:inner` to run the CLI and all three
public SDK selectors with built Python wheel and N-API addon, preserving the
existing server tests. Keep `cli_oidc_handoff_journey` ignored in the
Postgres-only `test:cli:journey` lane; it runs only under the provider-backed
identity lane. The CLI lane remains a regression for its existing Card tests.
List Rust selectors with `mise exec -- cargo nextest list` and Vitest
selectors with its listing mode before treating a green lane as evidence.

Extend the focused identity setup wrapper with targets `cli`, `rust`,
`client`, `python`, and `typescript`; each target uses the exact inner
selector command above, checks one selected test, and inherits the same
Postgres/Keycloak/Dex setup. The Python wheel and TS addon are built before
their selected journey. From a clean checkout, run `mise exec -- env
WYRD_IDENTITY_TARGET=cli WYRD_IDENTITY_FILTER=cli_oidc_handoff_journey mise
run test:identity:journey`; repeat with `rust`/`saved_user_auth_journey`,
`client`/`concurrent_saved_renewal`,
`python`/`test_saved_user_auth_journey`, and
`typescript`/`saved user auth journey`. The unfiltered lane selects all
these journeys plus the existing server and UI journeys.

## Material Stop Conditions

Stop if safe renewal needs changed server replay semantics, credentials cannot be tenant-selected without public contract change, or a language SDK must duplicate durable auth behavior.

## Authority Links

[Approved spec](../spec.md); [AGENTS.md](../../../../AGENTS.md); [agent rules](../../../../architecture/agent-rules.md); [security posture](../../../../architecture/wyrd-security-posture.md); [Python API](../../../../architecture/references/languages/python-api-and-stubs.md); [TypeScript guide](../../../../architecture/references/languages/typescript-guide.md).
