# TASK-004 Security Domain Review

## Immutable subject

- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84`
- Candidate remained `HEAD` throughout this review.
- Scope: security, authentication, authorization, credential, and tenant trust
  boundaries changed by TASK-004. This review does not assess unrelated product
  behavior or propose optional hardening.

## Boundary and authority coverage

| Boundary | Authority applied | Source and proof inspected |
|---|---|---|
| Browser-to-CLI handoff | Spec revision 7 REQ-005, REQ-011, INV-001, INV-004, AC-004, AC-007; TASK-004 packet-local handoff contract | `wyrd-spec/src/auth/cli_handoff.rs`; `wyrd-auth/src/{cli_logins,login}.rs`; `wyrd-server/src/auth/cli_login.rs`; auth router and callback; handoff migration and SQL; CLI journey |
| Handoff tenant/connection binding, expiry, replay, cancellation | Security posture tenant authority and fail-closed rules; TASK-004 one-use contract | `TenantConn` handoff queries and forced RLS; login-state binding and redemption; `CliLogins::{begin,claim,cancel}`; shared auth governor |
| Refresh rotation and logout | Security posture refresh-token rotation/reuse rules; binding human direction `human-direction-FIND-TASK-003-18.md` | `CliLogins::end`; `refresh_by_hash`, family lock, recursive `revoke_refresh_chain`; `SavedLogins::{begin_logout,finish_logout}`; CLI and Postgres proofs |
| Local credential trust boundary | Spec REQ-012; TASK-004 private-store and concurrent-renewal contract | Complete `saved_login.rs`; its callers in CLI, client config, auth middleware, and three SDK projections; unit and concurrent journey source |
| Credential precedence and tenant selection | Spec REQ-012, REQ-015, INV-001, INV-003, INV-005; TASK-004 explicit precedence/tenant contract | `ClientConfig::resolve_credential`; `CredentialChain`; `AuthMiddleware`; Rust/Python/TypeScript constructors and saved-user journeys |
| Secret transport and exposure | Spec REQ-005, REQ-011, AC-007; security posture TLS and secret-exposure rules | `TokenExchange`; `HttpConfig::validate`; CLI login/logout; saved renewal; tracing annotations, redacted `SecretBearer`, static completion page, CLI output assertions |
| Audit and throttling | Spec REQ-017; TASK-004 audit/throttle requirement; repository audit rules | Transactional allowed claim audit, best-effort denied authentication audit, shared per-peer-IP auth governor, route wiring |
| Production Python wheel boundary | AGENTS PyO3/test-runtime boundaries; TASK-004 production-wheel verification | Commit `feac127a0`; `mise.toml`; Python feature registration and `pyproject.toml`; live `mise run check:py-wheel-no-testing` |
| Applicable binding directions | Approved human directions supplied by the caller | Issuer binding and real connection-test directions remain unchanged by this task. Per-login logout direction was traced through the new CLI revoke path. |

## Security Audit

### Critical

None.

### High

#### SEC-TASK-004-1 — Secret-bearing CLI and saved-login exchanges permit remote cleartext HTTP

- **Classification:** VIOLATION
- **Violated obligation:** TASK-004 requires the handoff response and claim over
  TLS and prohibits exposing the poll verifier, access token, or refresh token.
  Spec REQ-005 and the security posture likewise require secrets to cross the
  client/server boundary under TLS. The existing `HttpConfig` rule explicitly
  refuses remote cleartext because credentials are sent to `/auth/token`.
- **Location:** `crates/wyrd/wyrd-cli/src/auth/login.rs:73-80,
  191-220`; `crates/shared/wyrd-client/src/auth.rs:153-168`;
  `crates/shared/wyrd-client/src/config.rs:203-209`;
  `crates/shared/wyrd-client/src/transport/config.rs:201-229`.
- **Evidence:** `LoginArgs.server` and `LogoutArgs.server` are parsed only as a
  `Url`. `LoginFlow::new` and logout then construct `TokenExchange` directly.
  `TokenExchange::new` builds a Reqwest client without calling the already-owned
  `HttpConfig::validate` cleartext check. The same unchecked constructor is now
  created for saved-login renewal in `ClientConfig::resolve_credential`.
- **Plausible exploit:** a user points `wyrd auth login` at a remotely hosted
  `http://` Wyrd endpoint (or receives such an endpoint from deployment setup),
  completes the genuine provider login in the browser, and the CLI then sends
  its poll verifier and receives the Wyrd access/refresh pair over plaintext.
  An on-path attacker can read the verifier or returned renewable credential
  and act as that tenant User. Routine saved-login refresh and logout likewise
  send the refresh token over that cleartext endpoint.
- **Impact:** theft of renewable human authority and subsequent access under the
  victim's tenant permissions.
- **Required correction:** enforce the existing remote-cleartext refusal in the
  shared `TokenExchange` construction boundary so every handoff, token exchange,
  renewal, revoke, platform exchange, and direct CLI caller receives the same
  protection. Preserve loopback HTTP for local development. Add focused proof
  that remote HTTP login/logout and saved renewal are refused before sending a
  secret, while HTTPS and loopback HTTP remain usable.

#### SEC-TASK-004-2 — The public tenant selector does not bind higher- or lower-priority credentials

- **Classification:** INCORRECT
- **Violated obligation:** TASK-004 says that when a tenant selector is supplied,
  every resolved authority must match it at exchange/authorization or fail and
  must never fall through to another tenant's source. Spec REQ-012 and REQ-015
  require selection of only the intended server and tenant.
- **Location:** `crates/shared/wyrd-client/src/config.rs:174-212` and
  `crates/shared/wyrd-client/src/transport/credential.rs:227-250,
  296-338`.
- **Evidence:** `resolve_credential` returns immediately when any explicit or
  environment credential exists, without carrying or checking `self.tenant`.
  `WYRD_ACCESS_TOKEN`, `WYRD_API_KEY`, an explicit token/key, and the
  `credentials.toml` floor therefore have no tenant check. For workload
  federation, `workload_token_from_env` prefers ambient `WYRD_TENANT` over the
  constructor's tenant override. If no saved record exists, selection also
  falls through to the unchecked credentials-file key. The new unit test
  positively expects an environment API key to resolve after setting a
  mismatching selector, and the implementation evidence records this ceiling.
- **Plausible exploit:** an operator has a tenant-A access token or API key in
  the environment from earlier work, then constructs a Rust, Python, or
  TypeScript client with `tenant="tenant-b"`. The explicit public selector is
  ignored; the server correctly derives tenant A from the valid credential, so
  a mutation intended for tenant B can execute against tenant A. An ambient
  `WYRD_TENANT=tenant-a` similarly overrides an explicit tenant-B selector for
  a workload assertion.
- **Impact:** cross-tenant confused-identity operations despite an explicit
  tenant safety boundary. Server authorization is not bypassed, but the client
  can exercise valid authority in the wrong tenant, which is the exact failure
  the selector contract forbids.
- **Required correction:** bind the effective tenant selector to every resolved
  credential in the shared client/auth owner. An explicit constructor selector
  must override ambient workload routing, and access tokens or tokens returned
  by API-key/workload/saved renewal must be refused before an application
  request when their verified/issued tenant does not match the selector. A
  credentials-file fallback must meet the same check. Add shared-client tests
  for explicit bearer, explicit/API-key, each environment tier, workload
  override, credentials-file fallback, and saved login; retain one public SDK
  journey proving the shared result rather than duplicating logic per language.

### Medium

None.

### Low / Defense In Depth

None. Optional hardening was excluded from this acceptance review.

### Positive Controls

- The handoff stores only a SHA-256 binding of a random 256-bit verifier; the
  raw verifier stays in the CLI and is redacted by `SecretBearer`.
- Forced RLS plus `TenantConn` binds handoff and login-state access to the route
  key's resolved tenant. The stored connection ID, unique initiation binding,
  database clock, row lock, completion deletion, and handoff deletion enforce
  connection binding, expiry, atomic one-use claim, and replay refusal.
- The shared auth router's existing per-peer-IP governor now wraps begin, claim,
  cancel, and revoke, so handoff polling consumes the same bounded auth budget
  as token guessing and login-state churn.
- Successful claim audit is appended in the same transaction that consumes the
  completion and handoff; audit failure rolls the credential handoff back.
  Refused authentication attempts use the canonical redacted auth audit path.
- The browser callback returns a static CLI-complete page, and tracing skips
  proofs and revoke bodies. Journey assertions cover no access/refresh token in
  CLI output, browser page, or authorization URL.
- Logout uses the existing principal-family lock but calls
  `revoke_refresh_chain` from the presented row. The recursive `rotated_from`
  traversal revokes that login and descendants while preserving a separate
  login for the same User, matching the binding human direction.
- The Unix saved-login store uses private directory/file modes, owner and
  symlink checks, stable OS locks, atomic replace plus fsync, a pre-network
  `RefreshPending` state, and a logout tombstone. Token-bearing types remain
  redacted in `Debug` and status output projects no secret.
- Rust, Python, and TypeScript constructors delegate credential selection and
  renewal to `wyrd-client`; no language-specific credential store or token
  rotation implementation was added.

## Production-wheel boundary check

The `feac127a0` change **strengthens rather than weakens**
`check:py-wheel-no-testing`:

1. The previous task ran after `py:setup`, which installs a
   `--features testing` editable build, and then tested that contaminated build.
   It therefore failed whenever setup worked and did not examine the production
   artifact named by the check.
2. The new task retains the negative `import wyrd.testing` assertion, builds the
   default wheel with the production feature set, installs that exact wheel in
   an isolated no-project environment, and adds a positive `import wyrd`
   control. The negative assertion can no longer pass merely because no Wyrd
   package was loaded, and the prior testing-enabled editable install cannot
   satisfy or contaminate it.
3. `sdks/wyrd-sdk-python/src/lib.rs` still registers `wyrd.testing` only under
   `cfg(feature = "testing")`; the default Maturin features in `pyproject.toml`
   do not enable that feature.

I ran `mise run check:py-wheel-no-testing` against the immutable candidate. It
built both the setup dependency and a separate default wheel, successfully
imported `wyrd` from the isolated wheel, refused `wyrd.testing`, and exited 0.

## Verification limits

- I inspected the complete cumulative diff and the full bodies of the security
  owners and their callers listed above. I did not rely on the implementation
  summary as proof.
- I ran only the explicitly requested production-wheel boundary check. The
  implementer's recorded identity, shared-client, CLI, SDK, codegen, lint, and
  boundary lane results were available but were not independently rerun in
  this domain pass.
- The CLI journey drives wrong verifier, wrong tenant, expiry, replay, secret
  absence, per-login logout, and offline deletion. It does not directly assert
  the staged handoff-claim audit rows or inject a claim-audit failure; the
  transactional source path is clear, but that audit behavior lacks focused
  regression proof in this candidate.

## Result

**FAIL** — `SEC-TASK-004-1` permits renewable user secrets over remote
cleartext HTTP, and `SEC-TASK-004-2` leaves the explicit tenant selector unable
to bind most credential tiers. Both are reachable on the shipped CLI/SDK paths
and violate approved security and tenant-selection behavior.
