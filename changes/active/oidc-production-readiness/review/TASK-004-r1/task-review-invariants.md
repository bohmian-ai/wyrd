# TASK-004 invariant review

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate and review-time `HEAD`: `7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7; `changes/active/oidc-production-readiness/tasks/TASK-004-laptop-clients.md`; and the binding human directions in `TASK-003-r1/human-direction-FIND-TASK-003-1.md`, `TASK-003-r2/human-direction-connection-test.md`, and `TASK-003-r5/human-direction-FIND-TASK-003-18.md`.
- Reviewed range: the complete `base..candidate` diff (69 files, 4,885 insertions and 163 deletions), not the implementation-evidence summary.
- CodeGraph: unavailable because the repository has no `.codegraph/` directory.

The issuer-binding and real-sign-in directions remain intact in this range. The logout direction is directly relevant: the new `/auth/revoke` path follows descendants of only the presented refresh row and does not call principal-wide family revocation.

## Producer-to-sink navigation and invariant trace

| Authority/value | Producer and durable owner | Consumers/sinks checked | Result |
|---|---|---|---|
| CLI handoff ID and verifier | `CliLogins::begin`; `wyrd.auth_cli_handoffs`; `HumanConnections::begin_login` | browser login state, callback completion, `CliLogins::claim`, cancel route, CLI poll loop | Binding, expiry, tenant RLS, one-use redemption, and verifier proof hold. Poll cadence is advisory only; no server throttle state or middleware exists. |
| Saved-login identity | `CliLogin` returned by the server; `SavedLogin::from_cli_login`; record stem from origin and tenant ID | CLI status/logout, `SavedLogins::select`, `ClientConfig::resolve_credential`, all SDK constructors | Saved-login selection is exact. The same selector is not enforced for explicit credentials, access-token/API-key environment tiers, or the credentials-file API-key floor. |
| Refresh generation and token | CLI completion writes `Ready`; `SavedLogins::renew` writes `RefreshPending`, exchanges, then writes incremented `Ready` | competing processes, `AuthMiddleware` cache, logout | File-lock rotation prevents two processes from replaying the same token. The middleware cache can reuse a token without rereading the record generation. `RefreshPending` also persists the raw refresh token, not a sealed value. |
| Logout state and revocation | `SavedLogins::begin_logout` writes `LoggedOut`; `/auth/revoke`; `CliLogins::end` | `revoke_refresh_chain`, `finish_logout`, concurrent renewals | Tombstone-before-network holds. Revocation is limited to the presented row and its descendants; another login for the same User stays active. |
| First-class SDK projection | shared `ClientConfig` and `SavedLoginSource` | Rust `Cards`, Python PyO3 constructors, TypeScript N-API constructors and public options | Rust owns the behavior and Python/TypeScript remain thin projections. Journey coverage exists in all three languages, subject to the gaps below. |
| Production Python wheel boundary | `mise.toml` `check:py-wheel-no-testing` at commit `feac127a0` | built default wheel in an isolated environment | Strengthened, not weakened: the old check imported the testing-enabled development install created by its `py:setup` dependency and therefore tested the wrong artifact. The candidate builds a default-feature wheel, positively imports `wyrd` from that wheel in an isolated environment, then requires `import wyrd.testing` to fail. |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-011 / task scenario 1: browser-to-CLI handoff is server-bound, expiring, verifier-held, one-use, and emits no credential in browser URLs/pages or CLI output | `cli_logins.rs:109-152,176-285`; `login.rs:134-153`; `cli_login.rs:77-136`; CLI `LoginFlow` | `cli_logins::pg_tests::only_the_verifier_holder_polls_and_cancel_ends_the_login`; `cli_oidc_handoff_journey` recorded in task evidence | PASS for binding and redemption |
| Task handoff contract: polling is throttled and completion/refusal is audited | claim answers `retry_after_seconds = 2`; success audit is in the claim transaction; refusals call best-effort auth audit | No test attempts a claim earlier than the interval or proves server enforcement | FAIL — INV-REV-002 |
| REQ-012: saved record is keyed by canonical server and tenant, private, symlink/ownership/mode checked, and atomically replaced | `saved_login.rs:173-183,219-298,461-570,717-730` | `saved_login::tests`; unsafe-store phase in `concurrent_saved_renewal` | PASS on the exercised Unix path |
| Task renewal contract: `RefreshPending` keeps the refresh token sealed while retaining it for logout | `SavedLoginState::RefreshPending` contains `SecretBearer`; `SavedLogins::write` serializes the record directly | `SecretBearer::serialize` emits the raw string; no at-rest seal/unseal proof exists | FAIL — INV-REV-003 |
| REQ-012 / task scenario 3: concurrent processes do not replay a rotated refresh token or overwrite a newer generation; crash/uncertain outcome fails closed | stable per-record OS lock; reread under lock; durable pending-before-network; generation increment after rotation | `concurrent_saved_renewal` proves one rotation across four child processes, pending refusal, unsafe-store refusal, and logout race | PASS for file-level rotation |
| Task renewal contract: an in-memory cache revalidates on-disk generation before reuse across processes | `SavedLoginSource::mint` rereads under lock, but `AuthMiddleware::bearer` returns any fresh cached renewable token before calling `mint` | No long-lived-client test mutates generation/logout from a sibling process between two requests | FAIL — INV-REV-004 |
| REQ-012 / task scenario 2: exact saved-login selection, ambiguity refusal, and no wrong-tenant fallback | `SavedLogins::select` filters canonical origin then tenant key/ID and fails mismatch/ambiguity | shared-client unit test and Rust/Python/TypeScript journeys cover saved-login ambiguity and mismatch | PASS for the saved-login tier |
| Task credential contract: explicit > environment > saved login > file floor, and every authority matches a supplied tenant selector at exchange/authorization | `ClientConfig::resolve_credential` preserves precedence but returns explicit/environment authority before selector validation and resolves the file floor without selector validation | Existing override journeys use a machine credential from the same selected tenant; there is no mismatched explicit/env/file authority test | FAIL — INV-REV-001 |
| REQ-012: routine renewal contacts Wyrd, not the IdP; unsafe or refused renewal does not fall through | `SavedLoginSource` exchanges only at Wyrd `/auth/token`; errors remain `RefreshPending`; credential resolution returns the saved-login error | three SDK journeys cover renewal and revoked/pending refusal | PASS |
| Binding logout direction: logout revokes only that login's refresh chain, even from a stale token; other User logins remain valid | `CliLogins::end` looks up the presented token, locks its principal, and calls `revoke_refresh_chain(row.id, ...)` | `logout_revokes_only_its_own_chain`; CLI journey | PASS |
| REQ-012: logout tombstones before remote work and local deletion still happens when remote revocation fails | `begin_logout` writes `LoggedOut`; CLI warns on revoke error and always calls `finish_logout` | local tombstone unit test; CLI journey evidence | PASS |
| INV-005 / scenario 4: Rust, Python, and TypeScript project one Rust-owned credential chain | shared-client owner plus thin PyO3/N-API constructor plumbing | Rust, Python, and TypeScript saved-user journeys; generated declarations present | PASS |
| AC-004: all SDKs use CLI-established authority for allowed/denied calls, renew, refuse revocation, handle same-server tenants, and honor explicit override | language journeys cover these nominal cases | recorded identity-lane results and source inspection | FAIL overall because selector enforcement and cache-generation obligations are not covered or satisfied |
| Production wheel excludes test-only harness behavior | `mise.toml:1245-1264` builds the default wheel and imports it with `uv run --isolated --no-project --with "$wheel"` | `mise run check:py-wheel-no-testing` run during this review (exit 0) | PASS; commit `feac127a0` strengthens the check |
| Non-goals: no provider token as Wyrd authority, second IdP app, language-specific store/renewal implementation, workload conversion, or principal-wide logout revocation | shared Rust owner; existing Web provider; Wyrd tokens only; per-chain revoke | cumulative diff inspection | PASS |

## Proposed findings

### INV-REV-001 — INCORRECT: tenant selection does not constrain higher- and lower-priority credential authorities

- Violated obligation: TASK-004 lines 60-70 require every resolved authority to match a supplied tenant selector at exchange/authorization or fail; REQ-012 and INV-001 prohibit choosing authority for another tenant.
- Location: `crates/shared/wyrd-client/src/config.rs:194-212`; `crates/shared/wyrd-client/src/transport/credential.rs:264-286`.
- Evidence: `resolve_credential` returns the explicit/environment chain at lines 195-202 before any tenant match. If those tiers are absent, the credentials-file API key at line 212 is also returned without a match. Only `SavedLogins::select` and the saved refresh response check tenant identity. `ResolvedCredential::ApiKey` carries no selector and `BearerToken` is returned verbatim. The existing journeys deliberately prove an explicit same-tenant machine key wins, so they cannot detect this gap.
- Observable consequence: `Cards::new(..., tenant = tenant_a)` can silently act as an explicit, environment, or file credential belonging to tenant B. The server still confines that credential to B, but the client violates the caller's explicit tenant intent rather than failing, so reads/writes can target the wrong tenant.
- Required testable correction: keep precedence unchanged, but make the shared Rust credential/exchange boundary validate the resulting Wyrd authority against `ClientConfig.tenant` before it is usable for any request. Do not patch each SDK. Add focused shared-client proof for mismatched explicit access token, API-key environment/file authority, and workload tenant, plus one public SDK journey demonstrating refusal; retain same-tenant explicit override.

### INV-REV-002 — MISSING: the anonymous claim endpoint does not enforce polling throttle

- Violated obligation: TASK-004 line 47 requires polling to be throttled.
- Location: `crates/wyrd/wyrd-auth/src/cli_logins.rs:212-285`; `crates/wyrd/wyrd-server/src/auth/cli_login.rs:88-136`; `crates/wyrd/wyrd-sql/src/queries/auth/cli_handoffs.rs:45-53`.
- Evidence: a pending claim always returns the constant two-second recommendation, but the row stores no last-claim deadline or attempt state, the lock query imposes no cadence, and the anonymous route has no claim-specific rate-limit layer. Any caller may immediately repeat the request and execute tenant resolution, a transaction, and row locking without limit.
- Observable consequence: a nonconforming or hostile poller can ignore `retry_after_seconds` and drive unbounded database work on an anonymous authentication route; the server has not implemented the required throttle.
- Required testable correction: enforce a bounded server-side claim cadence at the existing handoff owner or an already-existing server rate-limit mechanism, preserving indistinguishable wrong-verifier/tenant/expired refusals and one-use semantics. Add a real handler/owner test proving an early repeat is throttled and a claim after the allowed interval proceeds.

### INV-REV-003 — VIOLATION: `RefreshPending` persists the refresh token in plaintext rather than sealed

- Violated obligation: TASK-004 lines 74-80 require the pending record's refresh token to be sealed.
- Location: `crates/shared/wyrd-client/src/saved_login.rs:79-100,401-405,558-568`; producer serialization in `crates/wyrd-spec/src/auth/secret_bearer.rs:50-53`.
- Evidence: `RefreshPending` directly owns `SecretBearer`; `SavedLogins::write` serializes the complete enum to JSON; and `SecretBearer::serialize` emits its raw string. There is no seal/unseal step or opaque sealed field. The file-mode checks are a separate protection and do not satisfy the explicit sealed-record state.
- Observable consequence: a crash or uncertain timeout deliberately leaves a replay-sensitive refresh token on disk indefinitely in raw form. Re-presenting that consumed token can trigger the server's principal-wide refresh-reuse containment, affecting other sessions, which is precisely why pending state must retain it only in sealed form for logout.
- Required testable correction: use the repository's existing secret-sealing mechanism appropriate to local saved credentials to persist only sealed pending-token bytes, opening them only for the selected record's logout. Keep Ready credential behavior, the pending fail-closed rule, and per-chain logout scope unchanged. Add a persistence test proving the raw refresh token is absent from the pending file and logout can still revoke its chain after reload.

### INV-REV-004 — INCORRECT: fresh middleware cache entries bypass generation revalidation

- Violated obligation: TASK-004 lines 81-89 require an in-memory cache to revalidate generation before reuse across processes and require logout/renewal state not to be resurrected or silently ignored.
- Location: `crates/shared/wyrd-client/src/auth.rs:520-529`; `crates/shared/wyrd-client/src/saved_login.rs:596-620`.
- Evidence: `AuthMiddleware::bearer` returns a fresh cached token at lines 523-527 without invoking `SavedLoginSource::mint`; only `mint` rereads the record under its lock. The middleware cache contains token and expiry but no observed saved-login generation. The process-race journey starts fresh child processes, so it never exercises two requests from a long-lived client around a sibling process's generation change or tombstone.
- Observable consequence: after another process rotates, replaces, or logs out the record, a long-running client continues using its stale cached access token until the middleware's time skew or a 401 forces minting. It has not revalidated the shared generation as required and cannot observe a tombstone before cache expiry.
- Required testable correction: make the shared saved-login credential path consult the durable record generation/state before returning an in-memory token, reusing `SavedLogins` as the sole authority and without duplicating logic in SDKs. Add a two-request, two-process test in which a sibling rotates and then tombstones the record between calls; the original client must observe the new generation and then the logout instead of returning its old cache entry.

## Verification assessment and limits

- The task records green identity journeys for CLI, Rust, shared-client concurrency, Python, and TypeScript plus the broader shared/SDK/type/codegen/boundary lanes. Source inspection confirms those tests exist and exercise the nominal behaviors stated above.
- The focused journeys do not exercise mismatched non-saved credentials, enforced poll cadence, sealed pending bytes, or a long-lived middleware cache across sibling-process state changes. Those are proof gaps for reachable required behaviors, not optional hardening.
- The Python production-wheel check was inspected before and after `feac127a0` and run directly. Its artifact selection and positive-control import make it materially stronger than the prior check, which inspected the testing-enabled development installation.
- No provider qualification claim was inferred from mock-provider tests.

## Overall result

**FAIL**

The cumulative candidate establishes the principal handoff, exact saved-login selection, locked rotation, thin SDK projections, and per-login logout scope, but four required invariants remain unsatisfied: selector binding for every credential tier, server-enforced polling throttle, sealed pending-refresh persistence, and cache generation revalidation.
