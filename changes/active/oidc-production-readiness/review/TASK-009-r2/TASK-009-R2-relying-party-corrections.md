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
