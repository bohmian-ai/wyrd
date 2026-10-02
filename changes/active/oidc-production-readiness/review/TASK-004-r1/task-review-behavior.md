# TASK-004 behavior review

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-004-laptop-clients.md`
- Binding human directions:
  - `review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  - `review/TASK-003-r2/human-direction-connection-test.md`
  - `review/TASK-003-r5/human-direction-FIND-TASK-003-18.md`
- Repository authority: `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`,
  `architecture/wyrd-doctrine.mdx`, and the applicable language references.

The candidate and `HEAD` both resolved to the candidate commit when this review
started and when the report was written. `.codegraph/` is absent, so repository
navigation used `rg`, the cumulative Git diff, and direct source inspection.

## Navigation map and caller paths

| Capability | Owner and principal consumers | Proof inspected |
|---|---|---|
| CLI handoff | `wyrd_auth::cli_logins::CliLogins` -> server adapters in `wyrd-server/src/auth/cli_login.rs` -> typed `wyrd-spec` contracts and tenant-scoped SQL | Auth router governor, handoff SQL/migration, login-state completion path, CLI journey |
| Saved-login lifecycle | `wyrd_client::saved_login::SavedLogins` / `SavedLoginSource` -> `ClientConfig::resolve_credential` -> `AuthMiddleware` -> all Rust/Python/TypeScript handles | Unit tests, concurrent-process journey, three SDK journeys, CLI commands |
| Refresh-chain logout | `CliLogins::end` -> `lock_refresh_family` -> `revoke_refresh_chain` | SQL recursion, auth tests, CLI journey, binding logout direction |
| Language projection | `client_from_options` -> Rust handles, PyO3 constructors, N-API constructors and TS wrappers | Python/TS declarations, integration journeys, generated-boundary tasks |
| Production wheel | `mise.toml` `check:py-wheel-no-testing` at commit `feac127a0` | Diff inspection plus a successful focused task run |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-011 / Scenario 1: browser SSO hands a CLI-held verifier one sealed, expiring, one-use Wyrd credential without a pasted callback | `CliLogins::{begin,claim,cancel}`, `auth_cli_handoffs`, `LoginInitiation::Cli`, CLI `LoginFlow` | `cli_oidc_handoff_journey`; handoff unit/PG tests | PASS |
| Wrong verifier, wrong tenant, expiry, replay, and cancellation return no credential | Tenant RLS plus verifier hash in `lock_cli_handoff`; atomic completion redemption and handoff deletion; cancel deletes bound login state | CLI journey covers wrong verifier/tenant, expiry, replay; auth PG test covers cancellation | PASS |
| Polling is bounded/throttled and completion/refusal is audited | Pending response returns two seconds; the entire auth router, including claim, is behind the shared IP governor; allowed claim audit is transactional and refusals are recorded best-effort | Route/source inspection | PASS |
| REQ-005: no provider secret, Wyrd token, or poll verifier is exposed in CLI output or generic browser completion | Secret-bearing wire fields use `SecretBearer`; callback returns a constant generic completion page; CLI prints only the provider URL and token-free summary | CLI journey asserts access/refresh tokens absent from output, page, and login URL | PASS |
| Saved record is versioned, origin/tenant/principal-bound, private, symlink/ownership/mode checked, atomically replaced and fsynced | `SavedLogin`, `SavedLogins::{prepare_dir,read,write,lock}` | Saved-login unit tests; concurrent journey's unsafe-store phase | PASS |
| Credential precedence is explicit -> access env -> workload env -> API-key env -> saved login -> credentials file; explicit still wins | `ClientConfig::resolve_credential`, `CredentialChain`, shared `client_from_options` | Shared-client precedence test and three SDK override phases | PASS |
| A supplied tenant selector binds **every** resolved authority and a mismatch fails instead of authenticating another tenant | The selector reaches workload routing and saved-login selection, but explicit bearer/API-key, `WYRD_ACCESS_TOKEN`, `WYRD_API_KEY`, and the credentials-file API key discard it | No journey covers a higher-priority credential belonging to the wrong tenant | **FAIL — BEH-001** |
| No-selector saved-login selection is origin-exact, unique, and ambiguous on multiple same-server tenants | `SavedLogins::select` filters canonical origin and requires one candidate | Shared unit test and all three SDK two-tenant journeys | PASS |
| Safe refresh persists `RefreshPending` before network IO, rotates once under a stable per-record OS lock, and persists the next generation | `SavedLogins::renew`; `AuthMiddleware::mint_into`; per-record lock and atomic writer | `concurrent_saved_renewal` proves one winner across processes and no replay | PASS |
| An in-memory cache revalidates saved-login generation/state before reuse across processes | `AuthMiddleware::bearer` returns any non-stale cached Renewable bearer without consulting `SavedLoginSource`; the source retains no selected generation | No test keeps one live client while another process renews, logs out, or leaves the record pending | **FAIL — BEH-002** |
| Crash/uncertain timeout, lock timeout, corrupt/unsafe store, revoked family, and wrong-tenant renewal fail closed without replay or source fallback | Production branches exist for pending, lock timeout, unsafe/corrupt, refresh refusal, and renewed-token tenant mismatch | The concurrent journey manually seeds `RefreshPending` and drives unsafe mode, but does not drive an actual uncertain network outcome or lock timeout; the task evidence records those omissions | **FAIL — BEH-003** |
| Logout tombstones under the same lock, deletes locally despite server outage, and revokes only that login's refresh chain | `SavedLogins::{begin_logout,finish_logout}`; `CliLogins::end`; recursive `revoke_refresh_chain` under the family lock | CLI journey proves online/offline logout and another login remains renewable; auth PG test proves ancestor-to-successor chain scope | PASS |
| Human-direction logout scope: logout never revokes the User's other browser/CLI/SDK chains | Chain-root descendant revocation, not `revoke_refresh_family` | `logout_revokes_only_its_own_chain` and CLI journey | PASS |
| REQ-016: deactivated/replaced human connection stops saved-login renewal while bounded access authority remains server-owned | Saved login renews through the existing refresh owner; `RefreshTokens::execute` checks the exact active connection revision | Existing refresh PG coverage; no duplicated client lifecycle authority | PASS |
| Rust, Python, and TypeScript constructors expose the shared tenant option and delegate all credential logic to `wyrd-client` | Rust handles, PyO3 surfaces/stubs, N-API surfaces/declarations all call `client_from_options` | Rust/Python/TS public journeys and generated declaration lanes recorded in task evidence | PASS subject to BEH-001/002 |
| AC-004: each first-class SDK makes an allowed and denied call, renews without IdP, rejects revocation, selects among two tenants, and permits explicit override | Shared Rust owner plus thin Python/TS projections | `saved_user_auth_journey`, `test_saved_user_auth_journey`, and `saved user auth journey` | PASS for covered flows; overall AC-004 remains FAIL through BEH-001/002/003 |
| Binding issuer and real-interactive connection-test directions remain intact | CLI handoff reuses the existing production callback/exchange and active tested connection; no issuer/test probe logic is forked | Source trace into `HumanConnections::begin_login` and existing callback | PASS |
| No second IdP app, provider-token authority, workload/human conflation, or language-specific credential store was added | One Web connection and server handoff; shared Rust store/client; thin language boundaries | Cumulative diff inspection | PASS |
| Python production wheel excludes the test harness | See dedicated assessment below | `mise run check:py-wheel-no-testing` passed on the clean rerun | PASS |

## Required focused assessment: `check:py-wheel-no-testing`

Commit `feac127a0` **strengthens the boundary check; it does not weaken it**.

Before that commit, the task depended on `py:setup`, which installs the
testing-enabled editable build, and then tested `import wyrd.testing` in that
same environment. The import is expected to succeed there, so the old check
tested the wrong artifact and could not establish the production-wheel
boundary. This review confirmed that the development environment does expose
`wyrd.testing` after `py:setup`.

The new task:

1. builds a fresh default-feature wheel with no `testing` feature;
2. installs that exact wheel into an isolated, no-project `uv` environment;
3. positively imports `wyrd`, preventing a missing/broken installation from
   making the negative assertion vacuously pass; and
4. fails only if `wyrd.testing` imports from that production wheel.

That is the correct artifact and a strictly more credible assertion of the
boundary. `mise run check:py-wheel-no-testing` passed during this review. An
initial concurrent run failed in `maturin` while another process raced the
shared target staging rename; a clean rerun passed in 35.68 seconds. No check,
glob, feature set, or failure condition was relaxed.

## Proposed findings

### BEH-001 — INCORRECT: tenant selection is discarded for most higher-priority authorities

- **Violated obligation:** TASK-004 lines 60-72 require a supplied tenant
  selector to match every resolved authority at exchange/authorization or fail;
  REQ-012 and REQ-015 require intended-tenant selection without cross-tenant
  substitution.
- **Locations:**
  - `crates/shared/wyrd-client/src/config.rs:194-212`
  - `crates/shared/wyrd-client/src/transport/credential.rs:232-285`
  - `crates/shared/wyrd-client/src/auth.rs:520-552`
- **Evidence:** `ClientConfig::resolve_credential` returns immediately when an
  explicit or environment credential exists. Only the workload variant carries
  the selector. `ResolvedCredential::BearerToken` and `ApiKey` retain no
  selector, direct bearer use performs no tenant check, and API-key exchange
  derives the key's tenant without comparing the requested tenant. The same is
  true of the credentials-file floor. Every language constructor reaches this
  shared path.
- **Observable consequence:** a caller can request tenant A while supplying an
  explicit/environment/file credential for tenant B; the credential still wins
  and authenticates tenant B instead of failing the mismatch. That is not
  explicit-override semantics—the override should win only after satisfying the
  requested tenant binding.
- **Required testable correction:** preserve the exact precedence order, but
  carry the requested tenant constraint through the selected authority and
  enforce it at the existing exchange/authorization owner before any request is
  sent as that authority. Add focused shared-client coverage and a real
  two-tenant journey that supplies a tenant-B bearer/API key while selecting
  tenant A and proves a stable mismatch refusal with no lower-tier fallback.
  Cover the explicit, access-token environment, API-key environment, and file
  floor forms through the shared owner rather than per-language guards.

### BEH-002 — MISSING: live clients reuse cached saved-login tokens without generation revalidation

- **Violated obligation:** TASK-004 lines 74-90 require an in-memory cache to
  revalidate the saved record's generation before reuse and require pending,
  logged-out, unsafe, or newer state to fail closed or use the winner.
- **Locations:**
  - `crates/shared/wyrd-client/src/auth.rs:520-529`
  - `crates/shared/wyrd-client/src/saved_login.rs:297-306,360-449,596-622`
- **Evidence:** the first saved-login request calls `SavedLoginSource::mint`,
  but `AuthMiddleware` then caches only bearer plus expiry. Every subsequent
  non-stale request returns that bearer at `auth.rs:524-527` without reading
  the record. `SavedLoginSource` keeps the stem but no selected generation, and
  is not invoked on the cache-hit path.
- **Observable consequence:** after another process advances the generation,
  leaves `RefreshPending`, makes the store unsafe, or logs out/deletes the
  record, an already-running client continues using its cached authority until
  access expiry. It neither observes the winner nor applies the required
  fail-closed local state.
- **Required testable correction:** make a Renewable cache hit consult its
  owning source before reuse, and have `SavedLoginSource` validate the record
  under the existing stable lock against its current generation/state. Reuse
  `SavedLogins` rather than adding a second store or language-specific cache.
  A real-process test must keep one client alive, mutate the record from a
  second process through renewal, pending, and logout transitions, and prove
  the first client observes each transition before returning a cached bearer.

### BEH-003 — MISSING: required lock-timeout and uncertain-network renewal proof is absent

- **Violated obligation:** TASK-004 lines 83-90 and 164-171 require fail-closed
  proof for an uncertain timeout and lock timeout in the concurrent saved-login
  evidence; AGENTS.md section 11 makes user-observable negative journey flows
  part of completion.
- **Location:**
  `crates/shared/wyrd-client/tests/pg_auth_e2e_against_fixture.rs:218-315`.
- **Evidence:** `concurrent_saved_renewal` manually rewrites the record to
  `RefreshPending` and proves a later process refuses it. It never drives a
  refresh request whose outcome becomes uncertain, so it does not prove the
  client persisted pending before IO and retained it after timeout. No test
  holds the stable record lock through `LOCK_DEADLINE` and observes
  `lock_timeout`. Repository search finds no other coverage. The implementation
  evidence itself records both ceilings.
- **Observable consequence:** the two failure paths that protect against
  refresh replay and indefinite local contention can regress while the named
  identity target remains green. A pre-seeded final state is not proof of the
  transition that must produce it.
- **Required testable correction:** extend the existing concurrent renewal
  journey (or its narrow real-server supporting integration test) to drive one
  refresh with an ambiguous timeout after `RefreshPending` is durable, then
  prove no later process resends that token; separately hold the existing lock
  through its production deadline and prove the stable `lock_timeout` refusal
  without fallback. Do not add another credential store, retry path, or custom
  lock abstraction.

## Verification assessment

The task records successful runs for the identity journey and its five focused
targets, CLI/shared/SDK suites, Python and TypeScript unit/integration/type
lanes, codegen, boundary checks, formatting, and lints. Those recorded results
are consistent with the inspected test wiring but are not substitutes for the
missing scenarios above. This reviewer independently ran only the specifically
requested production-wheel boundary task; its clean rerun passed.

## Overall result

**FAIL**

The candidate implements the principal handoff, store, rotation, logout,
language projection, and production-wheel boundary correctly for the covered
paths. It does not yet satisfy the task exactly because tenant selection is not
enforced for every authority, cached saved-login authority does not revalidate
generation/state, and two explicitly required renewal failure paths lack their
named proof.
