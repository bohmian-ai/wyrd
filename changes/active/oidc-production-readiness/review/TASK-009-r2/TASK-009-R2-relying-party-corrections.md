---
id: TASK-009-R2
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 11
task: TASK-009
candidate: 1ddc10e21054ddc158f461e8c6d8aa862c32a067
base: 35a53faa216b10651d85c96ce12e34f382cac637
findings: [FIND-TASK-009-11, FIND-TASK-009-12, FIND-TASK-009-13, FIND-TASK-009-14]
---

# TASK-009 R2 relying-party corrections

## Authority and outcome

Approved specification:
`changes/active/oidc-production-readiness/spec.md` revision 11. Original task:
`changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`.
This remediation applies to the cumulative candidate
`35a53faa216b10651d85c96ce12e34f382cac637..1ddc10e21054ddc158f461e8c6d8aa862c32a067`.

Make the human relying party follow standard `openidconnect` audience behavior,
finish removing the old human-verifier ownership, use the existing platform
relying party for platform setup discovery, and evolve the duplicate audience
column through the repository's ordinary expand-and-contract sequence.
`FIND-TASK-009-5` is withdrawn and must not be reopened or changed.

## Diagnoses and required corrections

### Standard additional-audience refusal — `FIND-TASK-009-11`

`RelyingParty::id_token_verifier` calls
`set_other_audience_verifier_fn(|_| true)`, replacing `openidconnect 4.0.1`'s
default refusal of untrusted additional audiences. The shared tenant,
connection-test, and platform paths therefore accept
`aud = [client_id, arbitrary_other]` when `azp = client_id`, even though Wyrd
has no trust configuration for the other audience. The existing focused test
asserts this nonstandard behavior.

Delete the override and use the library default. Keep the present `azp`
validation for a present authorized-party claim. Add no audience allowlist,
setting, profile, second verifier, or Wyrd-owned replacement check.

### Remove residual human `ExternalVerifier` ownership — `FIND-TASK-009-12`

Production human ID-token verification moved to `RelyingParty`, and
`ExternalVerifier` now serves only workload RFC 7523/JWT-bearer verification.
Rustdoc in `wyrd-auth-verify`, `ServerAuth`, and `AuthHandles` still claims
tenant/platform human consumers and includes ephemeral implementation-commit
history. The materially changed callback fixture still constructs and stores a
`PgIssuerResolver`, `JwksCache`, and `ExternalVerifier` that the callback no
longer reads.

Correct the existing rustdoc to describe workload verification only and remove
implementation-history prose. Delete the unused callback fixture construction,
fields, imports, and constants, and rename that fixture to the issuing-key and
human-connection state it actually builds. Preserve the production
JWT-bearer caller. Add no documentation file, fixture abstraction, or check.

### Use the platform human relying party during setup — `FIND-TASK-009-13`

`PUT /platform/oidc/connection` calls the workload-oriented
`discover_jwks_uri`, which reads typed discovery metadata but deliberately does
not fetch JWKS. The route can commit a platform human connection that standard
library discovery cannot use, and same-issuer reconfiguration does not replace
the provider already cached by the process-owned `PlatformLogin`. Prior
`FIND-TASK-009-3` required metadata-only behavior only for workload trusted-
issuer administration and boot seeding.

Route platform human configuration through the existing boot-owned
`PlatformLogin`/`RelyingParty` fresh discovery operation and persist the
returned provider's advertised `jwks_uri`. This must perform ordinary
`openidconnect` discovery and replace the same process cache used by begin and
callback before the durable replacement commits. Leave both workload callers
on `ScreenedHttp::provider_metadata`. Add no custom key-health probe, empty-key
policy, second cache, invalidation service, option, retry system, provider
branch, or cross-replica coordination.

### Expand the audience schema without changing trust — `FIND-TASK-009-14`

The existing platform row stores `expected_audience TEXT NOT NULL`; the base
reader selects it, the base writer supplies it, and the base verifier consumes
it. The candidate immediately drops the column and omits it from SQL. Running
the migration first breaks overlapping base readers/writers; running the
candidate first breaks candidate writes against the old non-null schema. A
pre-existing mismatch is also destroyed and silently changes accepted trust
from the stored audience to `client_id`.

Make this release the conventional expand step:

1. Preflight existing rows and abort atomically, using the repository's
   established non-secret repair instruction, when
   `expected_audience <> client_id`.
2. Retain `expected_audience` physically for the supported-version overlap
   window and enforce the native database equality invariant so an overlapping
   base writer cannot introduce a new mismatch.
3. Have the candidate writer derive the retained value from `client_id`.
4. Keep the candidate request, view, resolver, and verifier free of any
   independent audience input and continue consuming `client_id`.
5. Defer dropping the compatibility column to a later contract release after
   the supported base application has drained.

The retained column is an internal derived overlap representation, not a
public setting or second trust choice. Do not add a compatibility service,
mode, toggle, alias, fallback, or permanent checker.

## Constraints and preserved behavior

- Keep `openidconnect 4.0.1`, its standard verifier behavior, and the one
  screened `ScreenedHttp` transport; keep the `oauth2` reqwest feature off.
- Preserve SSRF screening, DNS pinning, HTTPS policy, no proxy, no redirects,
  response/time bounds, S256 PKCE, state, nonce, RFC 9207, and one unknown-key
  rediscovery.
- Preserve the process-local Moka cache and its single-flight behavior; make no
  cross-process request-count or invalidation guarantee.
- Preserve tenant selection, single-use state, role mapping, issuance, audit,
  platform/tenant separation, connection revisions, secret sealing, and
  workload `expected_audience`.
- Preserve workload metadata-only setup and workload key retrieval through
  `ExternalVerifier`/`JwksCache`.
- Add no dependency, provider-specific branch, public compatibility surface,
  cache setting, retry policy, health probe, distributed coordination, new
  test harness, or new permanent check.
- Do not modify the withdrawn provider-error behavior from
  `FIND-TASK-009-5`.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-009-11` | A token with `[client_id, other]` is refused even when `azp = client_id`; the ordinary single-audience token still succeeds and mismatched present `azp` still fails. |
| `FIND-TASK-009-12` | Human callback/platform docs and fixtures no longer claim or construct `ExternalVerifier`; workload JWT-bearer production wiring remains intact. |
| `FIND-TASK-009-13` | Served platform configuration refuses unavailable or undecodable advertised JWKS without replacing the durable row; same-issuer reconfiguration refreshes the current process's provider before its next begin/callback; workload setup still makes zero JWKS requests. |
| `FIND-TASK-009-14` | Equal legacy rows upgrade and remain readable across old/new projections; candidate writes store the derived overlap value; mismatched legacy rows abort atomically; post-migration base-shaped mismatched writes are refused; no public independent audience input returns. |

## Focused and broader proof

Use Red-Green-Refactor and record exact zero-selection-safe commands.

- Extend the existing `id_token_refusals_fail_closed` proof for the
  multi-audience refusal and run it with its exact `mise exec -- cargo nextest`
  selector.
- Run the existing callback test target after removing dead verifier fixture
  wiring, and inspect the corrected rustdoc alongside the unchanged workload
  caller.
- Extend the existing served platform administration journey to cover failed
  JWKS discovery without mutation and same-issuer cache replacement. Retain
  the exact workload admin and boot zero-JWKS tests.
- Extend the existing platform identity migration/SQL coverage for equal and
  mismatched legacy rows, old/new projections, candidate derived writes, and
  the database equality invariant.
- Run every specifically named changed Rust test with an exact selector. Then
  run the narrowest repository-owned lanes covering the final surfaces,
  including `mise run test:identity:journey`, `mise run test:wyrd`,
  `mise run test:principals:integration`, `mise run test:shared`,
  `mise run codegen:check`, `mise run docs:check`, `mise run fmt`,
  `mise run lints`, `mise run check:client-tier`,
  `mise run check:pyo3-scope`, `mise run check:unwrap-audit`, and
  `mise run check:workspace-hack`. Run language lanes only if their source or
  generated contracts change; otherwise retain the already recorded TASK-009
  evidence and state why it remains applicable.

Route this task directly to `$wyrd-implement`.

## Remediation Evidence

Commits: `769024a02` (FIND-11/12/13), `7327f2030` (nextest group, lead-approved),
and this commit (refusal journey corrected for FIND-11, evidence). Base `35a53faa2`.
`FIND-TASK-009-14` is withdrawn by `lead-direction-FIND-TASK-009-14.md`: migration
`20261002000001` is unchanged, with no preflight, overlap column or dual-write.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-009-11` | `set_other_audience_verifier_fn(|_| true)` deleted from `relying_party::id_token_verifier`; `verify_authorized_party` still checks a present `azp` | RED: `id_token_refusals_fail_closed` with the new "untrusted additional audience" case accepted `aud=[client, other], azp=client`. GREEN after deletion. `tenant_callback_refusal_journey` refuses the same token served, and a single-audience `azp=client` token completes | PASS |
| `FIND-TASK-009-12` | `wyrd-auth-verify` `ExternalClaims`/`VerifiedExternalIdentity`/`ExternalVerifier`/`verify_external_against`, `ServerAuth::{external_verifier,trusted_issuer_resolver}` and `AuthHandles`/`build_auth_handles` rustdoc now describe workload `jwt-bearer` only, with the commit-history prose removed. The callback fixture is renamed `test_state_with_human_connections` and no longer builds `PgIssuerResolver`/`JwksCache`/`ExternalVerifier` (imports removed). The production `jwt-bearer` wiring is unchanged | `auth::callback` target: 18/18 pass; `test:wyrd` 0 (includes `jwt_bearer`) | PASS |
| `FIND-TASK-009-13` | `PUT /platform/oidc/connection` discovers through `PlatformLogin::relying_party().discover` (full `openidconnect` discovery, replaces the process cache entry before the row commits); errors map through the shared `admin::routes::discovery_error`; workload admin/boot stay on `ScreenedHttp::provider_metadata` | RED (route reverted): "unavailable key set" configure returned 200. GREEN: `federated_platform_sign_in_runs_through_the_served_callback` refuses an unavailable and an undecodable key set with the stored `client_id` unchanged; a same-issuer reconfiguration makes a newly published `mock-2` key verify at callback with 0 discovery requests during an outage. Workload zero-JWKS tests pass | PASS |

Diagnoses:

- `platform_admin_e2e` (full binary, default profile): `recovery_is_refused_for_every_state_but_active`
  failed at server start with "sorry, too many clients already". Cause: under the
  default nextest profile, up to 32 per-test WyrdTestServers boot concurrently
  against Postgres `max_connections=400`, and the binary was outside the existing
  `postgres-fixtures` ceiling that `pg_router_smoke` uses for the same reason. Fix
  site: `.config/nextest.toml`, one override (lead-approved, `7327f2030`).
- `test:identity:journey` exit 100: `tenant_callback_refusal_journey` asserted that
  `aud=[client, another-client], azp=client` completes, which is the nonstandard
  acceptance FIND-11 removes. The callback returned 401 "`another-client` is not a
  trusted audience" (WYRD_LOG trace). Fix site: the journey. That token is now a
  refusal case, and the success case is a single-audience token with `azp=client`.

Exact named tests (candidate code `769024a02` plus the journey fix; Postgres
wrapper where needed). All exited 0:

```
mise exec -- cargo nextest run --locked -p wyrd-auth-oidc --lib -E 'test(=relying_party::tests::id_token_refusals_fail_closed)' => exit=0, 1 test run: 1 passed, 46 skipped
mise exec -- cargo nextest run --locked -p wyrd-auth-oidc --lib -E 'test(=relying_party::tests::a_valid_id_token_maps_its_identity)' => exit=0, 1 test run: 1 passed, 46 skipped
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^auth::callback::/)' => exit=0, 18 tests run: 18 passed, 479 skipped
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=boot::issuer::pg_tests::issuer_boot_discovery_happy_path_fills_jwks_uri)' => exit=0, 1 test run: 1 passed, 496 skipped
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=boot::issuer::pg_tests::issuer_boot_discovery_recovers_after_transient_blip)' => exit=0, 1 test run: 1 passed, 496 skipped
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::admin::routes::pg_tests::create_persists_sealed_secret_and_get_redacts)' => exit=0, 1 test run: 1 passed, 496 skipped
mise exec -- cargo nextest run --locked -p wyrd-server --test platform_admin_e2e -E 'test(=federated_platform_sign_in_runs_through_the_served_callback)' => exit=0, 1 test run: 1 passed, 38 skipped
mise exec -- cargo nextest run --locked -p wyrd-server --test platform_admin_e2e -E 'test(=an_operator_configures_and_removes_federated_platform_sign_in)' => exit=0, 1 test run: 1 passed, 38 skipped
mise exec -- cargo nextest run --locked -p wyrd-server --test platform_admin_e2e -E 'test(=a_connection_cannot_name_an_unresolvable_issuer)' => exit=0, 1 test run: 1 passed, 38 skipped
mise exec -- env WYRD_IDENTITY_TARGET=server WYRD_IDENTITY_FILTER=tenant_callback_refusal_journey mise run test:identity:journey => exit=0, 1 test run: 1 passed, 30 skipped
```

Lanes (one at a time, `CARGO_TARGET_DIR` set):

| Lane | Exit |
|---|---|
| `mise run fmt` | 0 (rerun after journey fix: 0) |
| `mise run lints` | 0 (rerun after journey fix: 0) |
| `mise run codegen:check` | 0 |
| `mise run docs:check` | 0 |
| `mise run check:client-tier` | 0 |
| `mise run check:pyo3-scope` | 0 |
| `mise run check:unwrap-audit` | 0 |
| `mise run check:workspace-hack` | 0 |
| `mise run test:shared` | 0 |
| `mise run test:principals:integration` | 0 |
| `mise run test:identity:journey` | 100, then 0 after the journey fix (unfiltered) |
| `mise run test:wyrd` | 0 (2344 passed) |

The language lanes were not rerun. r2 changes no Python, TypeScript or SDK
source, and no generated contract (`codegen:check` 0 with no diff). The r1
language-lane evidence on `0b516e235` therefore still applies.

Non-goals: no audience allowlist, setting, second verifier, key-health probe,
empty-key policy, second cache, invalidation service, retry, provider branch, or
`FIND-TASK-009-5`/`-14` change.
