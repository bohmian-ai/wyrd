---
id: TASK-004
kind: implementation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 5
requirements: [REQ-005, REQ-011, REQ-012, REQ-015, REQ-016, INV-001, INV-002, INV-003, INV-005, AC-004, AC-007, AC-009]
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
provider callback, the server seals the Wyrd credentials once in TASK-002's
existing pending-completion row, bound to the handoff ID, and shows only a
generic browser success page. The handoff row stores the verifier binding,
not a second token copy. No provider code, Wyrd token, or poll verifier is in
the browser URL/page. CLI polls
`POST /auth/cli-handoffs/{id}/claim` with the verifier. Pending returns a
bounded retry interval; complete atomically consumes the handoff and its
sealed completion and returns `{server_origin, tenant_id, principal_id,
access_token, refresh_token,
access_expires_at}` only to the verifier holder. Wrong verifier, tenant,
expiry, second claim, and cancellation return typed refusals and no tokens.
Throttle polling and audit completion/refusal. No second IdP app is used.

`wyrd-client` owns a versioned user credential record keyed by canonical
server origin and stable tenant ID: `{format_version, origin, tenant_id,
tenant_key, principal_id, access_token, access_expires_at, refresh_token,
generation}`. The CLI writes it into the one Wyrd credential file,
`~/.config/wyrd/credentials.toml` (human direction FIND-TASK-004-4), as a
`[[logins]]` table beside the user's other content, which every write
preserves. The file is user-owned `0600`; symlinks and unsafe ownership or
permissions fail closed; writes use atomic replacement. There is no other
credential file and no local encryption. `wyrd auth logout` removes the selected local
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

For renewal, all processes take an exclusive OS lock on the stable Wyrd
configuration directory, then reread the on-disk generation and refresh token. Before any
network refresh, persist `RefreshPending { generation, started_at }` by fsync
plus atomic replace while holding the lock; its refresh token stays in that
record, protected only by the file's `0600` user-only mode, so logout can
revoke its chain, but must never be sent again by a later process. The owner uses
the token once, persists the returned access/refresh pair and incremented
`Ready` generation by fsync plus atomic replace, then releases the lock.
An in-memory cache must revalidate generation before reuse across processes.
A loser observes the winner's new generation and does not replay its old
token. Crash or uncertain timeout leaves `RefreshPending`; a later process
must fail closed and require login because it cannot know whether the server
accepted the old token. It never retries that token or silently overwrites a
newer generation. Logout takes the same lock, first persists a
`LoggedOut` tombstone, then revokes remotely and deletes the secret. A
concurrent renewal cannot recreate a logged-out record; a crash after the
tombstone still blocks reuse. Lock timeout, corrupt/unsafe store, revoked
family, and wrong-tenant responses likewise fail without fallback to another
tenant's record. Routine API requests contact Wyrd, not the IdP.

## Ordered Implementation Scenarios

### Scenario 1 — Browser to CLI handoff

**Behavior.** The system browser completes SSO; the CLI receives Wyrd user authority through the existing sealed, bound, expiring, one-use completion without callback paste, redirect token, printed token, duplicate token store, or second IdP app. A missing or wrong verifier, wrong tenant, expired completion, and replay return no credentials.

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

## Implementation Evidence

Commits: d4f4775e6, e7b7bfc01 (server handoff and per-chain revoke), 15a1790fc
(saved-login store), e73f0163e (credential precedence and SDK `tenant`),
7fa2f995b (`wyrd auth login/logout/status`), 4bbdfe90f (CLI journey and identity
targets), 4d6a72c3d (Rust SDK journey), ed50a2e61 (concurrent renewal), 00529eef2
(Python journey), 5bcb2b618 (TypeScript journey), c0c1d6053 (clippy), feac127a0
(production-wheel check).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| CLI browser login: one-use handoff, wrong verifier/tenant, replay, expiry refused; no token in output, page, or URL (AC-004/007) | `wyrd-server` CLI handoff routes; `crates/wyrd/wyrd-cli/src/auth/login.rs` | `cli_oidc_handoff_journey` (identity target `cli`) | PASS |
| Logout revokes only its own refresh chain; offline logout still removes the record and warns (FIND-TASK-003-18) | `SavedLogins::{begin_logout, finish_logout}`; `auth::login::logout` | `cli_oidc_handoff_journey` | PASS |
| Private store: 0700/0600, owner-checked, symlink-refusing, atomic, fail-closed on unsafe or corrupt | `crates/shared/wyrd-client/src/saved_login.rs` | `saved_login::tests::*` (5); `concurrent_saved_renewal` (unsafe_store) | PASS |
| Precedence: explicit > env tiers > saved login > credentials file; explicit machine credential overrides | `ClientConfig::resolve_credential` | `config::tests::saved_login_ranks_between_env_and_credentials_file`; override phase in all three SDK journeys | PASS |
| Same-server multi-tenant selection: ambiguous and unmatched refused, selection by route key or tenant id | `SavedLogins::select`; `tenant` option on every Rust/Python/TypeScript constructor | Rust, Python, and TypeScript `saved user auth` journeys | PASS |
| Allowed read, denied write as the CLI-established reader; renewal through Wyrd only, exactly one generation | `SavedLoginSource`, `SavedLogins::renew` | three SDK journeys (generation == before + 1) | PASS |
| Revoked login fails closed (`refresh_refused`), never retried (`refresh_pending`) | `SavedLogins::renew` persists RefreshPending before exchanging | three SDK journeys | PASS |
| Concurrent processes: one rotation, winner generation, no family revocation, crash-uncertain and unsafe stores fail closed, logout racing renewal never restores a record (AC-004) | reread under lock in `credentials.toml` | `concurrent_saved_renewal` (identity target `client`) | PASS |
| Generated artifacts current | error catalog, `.pyi`, `index.d.ts`, `error-codes.ts` | `codegen:check`, `ts:napi:check` | PASS |

Lanes (each run alone, all exit 0): `test:identity:journey` unfiltered and each
target (`cli`, `rust`, `client`, `python`, `typescript`); `test:cli:journey`;
`test:shared`; `test:wyrd-sdk`; `py:test:unit`; `py:test:integration` (72
passed); `py:typecheck`; `ts:test:unit`; `ts:test:integration` (25 passed);
`ts:typecheck`; `ts:napi:check`; `codegen:check`; `check:client-tier`;
`check:pyo3-scope`; `check:py-wheel-no-testing`; `check:workspace-hack`;
`test:principals:integration`; `fmt`; `lints`; `py:format`; `py:lints`.

Diagnoses:
- **Renewal refused as `tenant_mismatch`.** Evidence: the Rust SDK journey's
  renew phase failed with `(tenant_mismatch)`. Cause: `access_token_tenant`
  read a top-level `tenant_id`, but `AccessTokenClaims` carries the tenant at
  `principal.tenant_id`. Fix site: `saved_login.rs::access_token_tenant`, whose
  only caller is `SavedLogins::renew`. Unit test added.
- **The CLI journey's second login could not renew.** Evidence: `RefreshReused`
  after the logged-out token was presented. Cause: the journey presented the
  revoked token before renewing the second login. The server treats that as a
  replay and revokes every chain of the principal, which is the intended
  containment. Fix site: journey step order only.
- **`check:py-wheel-no-testing` always failed.** Cause: the check depended on
  `py:setup`, which installs the testing build. Fix site: the check now builds
  the default wheel and imports it in an isolated environment, with a positive
  `import wyrd` control.
- Python and TypeScript journeys need Keycloak. They carry the `identity`
  pytest marker or are excluded from `ts:test:integration`, so only the
  identity lane runs them.

Ceilings:
- Ctrl-C cancel has no journey coverage; it is covered by unit tests and code
  paths only. (The env-tier selector and lock-timeout ceilings were closed in r1.)
- The SDK journeys establish the login through the `HumanSso` handoff helper,
  which uses the same server handoff and `SavedLogin::from_cli_login` as the
  CLI, rather than through the `wyrd` binary. The CLI journey covers the binary.
- The focused `concurrent_saved_renewal` command needs `--run-ignored=all`; the
  identity target supplies it.
- Non-goals stayed excluded. No unrelated files changed, apart from the broken
  production-wheel check and the clippy line in `gateway_admin.rs` that this
  task's argument change triggered.

### Review r1 remediation

Human decisions applied, recorded in
`review/TASK-004-r1/human-direction-FIND-TASK-004-4.md`:
1. **One credential file (FIND-4).** Saved logins live in
   `~/.config/wyrd/credentials.toml`; the separate `logins/` store is gone.
   There is no local encryption. The file is 0600 and user-only, and the
   client fails closed when it is unsafe. The lock, generation,
   RefreshPending, LoggedOut and atomic-replace semantics run against that
   file and preserve the user's other content.
2. **Tenant key only (FIND-1).** `tenant` is a tenant route key. It selects
   the saved login and names the tenant for the workload-token exchange,
   winning over an ambient `WYRD_TENANT`. An explicit credential, an access
   token, or an API key already names its tenant, so a set `tenant` beside
   one is refused with `WYRD_CLIENT_400_CONFIG_INVALID` ("this credential
   already names its tenant"). There is no claim parsing and no id
   comparison. The id-comparison commit 741449736 was reverted by ce6cc4bab.
   - Consequence: `client.tenant`/`WYRD_TENANT` feeds the selector, so an
     ambient `WYRD_TENANT` together with an API key or access token is now
     refused. Unset it for machine credentials.
3. **Token cache (follow-up).** `~/.config/wyrd/tokens` is deleted, together
   with `client.token_cache.path` and `WYRD_TOKEN_CACHE_PATH`. With
   `kind = "disk"`, the access token exchanged from `[default].api_key` is
   cached beside that key in `credentials.toml`. The write goes through the
   shared locked, atomic, 0600, content-preserving writer in
   `credentials_file.rs`. Workload and renewable tokens stay in memory.

Commits: 2cd10e99f (FIND-4), 6e4c30ec2 (FIND-5), 1f2d50c03 (FIND-6),
efe624f8c (FIND-7), 914844ef8 (FIND-2), f95131a79 (FIND-3), 741449736 then
ce6cc4bab (rejected approach and its revert), 05f649dce (FIND-1), fcf5c366a
(token cache), ab853480f (TS declarations), 2d8e5220f and the following
commit (lane fixes).

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-4: one credentials file, no separate store, fail-closed on unsafe | `credentials_file.rs::CredentialsFile`; `saved_login.rs::SavedLogins` | `saved_login::tests::logins_live_in_credentials_toml_beside_user_content`; `concurrent_saved_renewal` (unsafe_store) | PASS |
| FIND-1: key-only selector; refused beside a self-naming credential; selector routes the workload token | `ClientConfig::refuse_selector`; `SavedLogins::select`; `workload_token_from_env` | `config::tests::tenant_selector_is_refused_beside_a_self_naming_credential` (red before green); override phase in the Rust, Python, and TypeScript journeys | PASS |
| FIND-2: a cached saved-login bearer revalidates the durable generation | `AccessTokenSource::revalidates_cache`; `AuthMiddleware` | `auth::tests::saved_login_cache_revalidates_every_use` | PASS |
| FIND-3: lost-response renewal and lock timeout fail closed | `LossyProxy` relay in `pg_auth_e2e_against_fixture.rs` | `concurrent_saved_renewal`: one request with `200 OK` then `refresh_pending` and no retry; `lock_timeout` after 30s with no request | PASS |
| FIND-5: TokenExchange refuses remote cleartext (loopback only) | `TokenExchange::new` reuses the transport policy | `auth::tests::token_exchange_refuses_remote_cleartext`; `cli_oidc_handoff_journey` | PASS |
| FIND-6: Python constructors and Bifrost wrappers accept and forward `tenant` | `python/wyrd/bifrost/__init__.py`; stubs | `test_every_public_constructor_accepts_and_forwards_tenant`; `py:typecheck` | PASS |
| FIND-7: canonical transactional audit for CLI refresh-chain revocation | `CliLogins::end` appends `auth.cli_login.logout` via `append_auth_audit` | `cli_logins::pg_tests::logout_revokes_only_its_own_chain`: one event; a refused append rolls back | PASS |
| Token cache in credentials.toml | `CredentialsFile::{cached_api_key_token, cache_api_key_token}`; `AuthMiddleware::persist` | `auth::tests::disk_cache_writes_token_beside_its_api_key`; `renewable_tokens_are_never_persisted` | PASS |

Lanes (each run alone; every lane's final run exited 0):
- Identity targets `rust`, `client`, `cli`, `python`, `typescript`, then
  `test:identity:journey` unfiltered.
- `fmt`, `lints`, `py:format`, `py:lints`, `codegen:check`,
  `check:client-tier`, `check:pyo3-scope`, `check:py-wheel-no-testing`,
  `check:workspace-hack`, `docs:check`.
- `test:shared`, `test:wyrd-sdk`, `test:cli:journey`,
  `test:principals:integration`, `test:wyrd` (2352 passed).
- `py:test:unit` (501 passed), `py:typecheck`, `py:test:integration`.
- `ts:test:unit`, `ts:typecheck`, `ts:napi:check`, `ts:test:integration`.

r1 diagnoses:
- **`lints` failed.**
  - Symptom: clippy `too_many_lines` on `logout_revokes_only_its_own_chain`,
    and `type_complexity` on the selector tier table.
  - Fix: seeding moved into `seed_two_cli_logins`, and the tier table was
    flattened to `Option<(name, value)>`. Assertions are unchanged.
- **`check:workspace-hack` failed.**
  - Cause: the `toml_edit` `serde` feature that the new credentials writer
    uses had not been hoisted.
  - Fix: ran `cargo hakari generate`.
- **`docs:check` failed.**
  - Cause: `api/errors.md` lacked `WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE`.
  - Fix: regenerated it with `docs:generate`.
- **`py:test:unit` failed.**
  - Symptom: `test_wyrd_server_url_alone_sets_both_endpoints` used
    `http://wyrd.internal`, which FIND-5 now refuses on purpose.
  - Fix: the test now uses `https://`. Its subject, endpoint derivation, is
    unchanged.
- **`test:wyrd` exit 124.** The first run hit the 595s shell timeout during a
  9-minute compile. It was not a test failure; the rerun passed.
- **Not rerun after the late fixes.** After the workspace-hack and test-only
  fixes, `lints` was rerun clean, which compiles every target. The other
  lanes were not rerun.

Human decision 4 (commit 83e4c597b): saved-login renewal trusts the server, as
gh, gcloud, az, and aws sso do. It sends the refresh token, stores what the
server returns, and uses it. The client-side tenant re-check is removed:
`access_token_tenant`, its test, and the renewal `tenant_mismatch` branch. The
earlier "Renewal refused as `tenant_mismatch`" diagnosis is superseded. Nothing
replaced it. `tenant_mismatch` remains only as the `select` reason for an
unmatched selector, and the three journeys still assert it there. `fmt`, `lints`,
and `test:shared` (724 passed) exited 0. The identity lane was not rerun because
no journey referenced the removed check.

### Spec revision 8 verification

Spec revision 8 supersedes the handoff and renewal text above: CLI login uses the
RFC 8628 device grant, and RefreshPending, generation, LoggedOut,
lock_timeout, and the per-login format_version are gone. Implementation:
061f1906f, 9f5859303, and cf2b637d3, which removes stale handoff wording from
`/auth/login` and `TokenExchange` docs.

Audit against revision 8:
- Device grant: `POST /auth/device_authorization`, `/auth/device`, and the
  `urn:ietf:params:oauth:grant-type:device_code` grant at `/auth/token`.
  Errors are `authorization_pending`, `slow_down`, `access_denied`,
  `expired_token`, and `invalid_grant`.
- No CLI handoff remains in code, SQL, docs, or tests.
- The browser `BeginLogin` flow is unchanged; only the CLI branch was removed.
- Without `tenant`, the newest login for the server is used.
- Logout deletes the record first, then revokes best-effort and warns.
- The 0600, symlink, and directory checks still fail closed.

Lane exit codes (each lane run alone):

| Lane | Exit |
|---|---|
| fmt, lints, py:format, py:lints | 0 |
| codegen:check, check:client-tier, check:pyo3-scope, check:py-wheel-no-testing, check:workspace-hack, docs:check | 0 |
| py:test:unit, py:typecheck, ts:test:unit, ts:typecheck, ts:napi:check, test:wyrd-sdk | 0 |
| test:identity:journey (all targets and unfiltered), test:shared, test:cli:journey, test:principals:integration, py:test:integration, ts:test:integration, test:wyrd | not run: Docker daemon absent on host |
