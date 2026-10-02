# TASK-009 round-2 structured Ponytail validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `1ddc10e21054ddc158f461e8c6d8aa862c32a067`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Prior verdict and ledger: `changes/active/oidc-production-readiness/review/TASK-009-r1/{verdict.md,findings-validation.md}`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-009-r1/TASK-009-R1-relying-party-corrections.md`
- Human withdrawal: `changes/active/oidc-production-readiness/review/TASK-009-r1/lead-direction-FIND-TASK-009-5.md`

The candidate remained unchanged while this validation was performed. I read
the complete cumulative diff, the remediation delta, every required discovery
and follow-up report, the applicable repository and deployment authorities,
the full bodies of the cited owners and consumers, the base implementations,
and the installed `openidconnect 4.0.1` verifier behavior. `.codegraph/` is
absent, so navigation used repository search, commit-qualified source, and
direct caller tracing.

The standing human direction controls this ledger: Wyrd uses the published
standard or the conventional mechanism used by comparable projects. A custom
probe, cache-invalidation service, audience option, compatibility mode,
permanent checker, or new test harness is not an acceptable correction.
`FIND-TASK-009-5` is **WITHDRAWN** and is neither reopened nor reassigned.

## Proposal validation

| Discovery proposal | Result | Source validation and disposition |
|---|---|---|
| `BEH-R2-001`, `OIDC-SEC-R2-001` | **CONFIRMED** | `id_token_verifier` replaces `openidconnect`'s secure standard default with `set_other_audience_verifier_fn(|_| true)`, and the focused test requires an arbitrary co-audience to succeed. Tenant login, candidate completion, and platform login all reach this shared verifier. Retained as `FIND-TASK-009-11`. |
| `REPO-R2-TASK-009-1`, `MNT-R2-001` | **REVISED** | The production and public rustdoc still assigns human login to workload-only `ExternalVerifier`, and the materially changed callback fixture still constructs and stores that unused verifier. These are one residual cause: the deleted human-verifier ownership was not removed from the module's maintained description and test wiring. Retained as `FIND-TASK-009-12`. |
| `PERSIST-TEN-001` | **REVISED** | The platform configure route is a reachable human-setup path under REQ-004, but it reuses the workload metadata-only helper and does not touch the already process-owned human relying party. Retain only standard full `openidconnect` discovery and same-process cache replacement. Reject a custom key-health probe, an empty-key policy beyond the library, and any cross-replica invalidation promise. Retained as `FIND-TASK-009-13`. |
| `PERSIST-TEN-002`, `SYS-R2-001` | **REVISED** | The immediate column drop both silently reinterprets existing trust and makes base/candidate SQL mutually incompatible during the repository's required overlap window. A preflight followed by the same-release drop does not fix overlap; retaining the column without constraining old writers does not preserve the invariant. Consolidated as `FIND-TASK-009-14`. |
| Cache-concurrency report | **REJECTED (no new finding)** | Moka `try_get_with`, shared clones, bounded invalidation, and the focused overlap tests close prior `FIND-TASK-009-4`. A generation protocol or every-interleaving guarantee would be DRIFT. |
| Invariant report's empty ledger | **ACCEPTED AS COVERAGE, not a verdict** | Its producer-to-sink trace supports closure of the prior findings, but it did not inspect the standard audience override, residual docs/test wiring, platform setup discovery, or rolling schema seam deeply enough to displace the source-backed proposals above. |
| Follow-up resolution | **REVISED** | Its schema reconciliation is source-correct and conventional. Its platform discovery reconciliation is retained only at the standard library discovery/cache boundary; the proposed extra notion of an independently “usable” JWKS is not required beyond successful library discovery. |

## Final deduplicated finding ledger

### FIND-TASK-009-11 — CONFIRMED — DRIFT: human ID-token verification trusts every additional audience

- **Discovery sources:** `BEH-R2-001`, `OIDC-SEC-R2-001`.
- **Violated obligation:** TASK-009 and specification REQ-007 require OIDC Core
  1.0 section 3.1.3.7 audience validation through `openidconnect`. The client
  has one configured human audience, its client ID. OIDC Core requires
  rejection of additional audiences the client has not established as trusted,
  and the selected library implements that rule by default.
- **Exact location:**
  `crates/shared/wyrd-auth-oidc/src/relying_party.rs:620-665`, especially line
  638; contradictory success assertion at
  `crates/shared/wyrd-auth-oidc/src/relying_party.rs:1038-1043`.
- **Producer-to-consumer evidence:** `CodeRedemption::client_id` is produced by
  the resolved tenant or platform connection. `RelyingParty::verify` passes it
  to `id_token_verifier`. The verifier's
  `set_other_audience_verifier_fn(|_| true)` marks every other `aud` value
  trusted before `verify_authorized_party` checks only `azp`. Tenant callback,
  connection-test completion, and platform callback all converge on this
  owner. The installed `openidconnect 4.0.1` default is `false`, and its API
  cites the same OIDC rejection rule.
- **Observable consequence:** Wyrd accepts a signed human ID token addressed
  jointly to its client and an arbitrary client for which Wyrd has no trust
  configuration. Depending on the caller, that can issue a tenant identity,
  mark a candidate tested, or issue a platform session.
- **Decision-complete correction:** Delete the trust-all override and use the
  library's existing default. Retain the current `azp` validation for a present
  authorized-party claim. Add no audience allowlist, setting, profile, second
  verifier, or Wyrd-owned audience mechanism.
- **Focused closure proof:** In the existing relying-party refusal test, prove
  `[client_id, other]` is refused even with `azp = client_id`; retain success
  for the ordinary single `client_id` audience and refusal for a mismatched
  present `azp`.

### FIND-TASK-009-12 — REVISED — DRIFT: the removed human `ExternalVerifier` path remains in maintained docs and callback fixture wiring

- **Discovery sources:** `REPO-R2-TASK-009-1`, `MNT-R2-001`.
- **Violated obligation:** TASK-009 requires deletion of the human
  `ExternalVerifier` path. Repository documentation rules require materially
  changed owners to describe their actual consumers and prohibit ephemeral
  implementation history. Tests should construct only dependencies the
  exercised behavior uses.
- **Exact location:**
  `crates/shared/wyrd-auth-verify/src/lib.rs:148-176,371-377`;
  `crates/wyrd/wyrd-server/src/components/auth/state.rs:22-24`;
  `crates/wyrd/wyrd-server/src/boot/auth.rs:37-56`;
  `crates/wyrd/wyrd-server/src/auth/callback.rs:1231-1282`.
- **Producer-to-consumer evidence:** The cumulative change deletes
  `ExternalVerifier::verify_id_token_against`; the only production consumer of
  `ServerAuth::external_verifier` is the RFC 7523 JWT-bearer path. Human tenant
  and platform tokens now verify through `RelyingParty`. Nevertheless,
  `ExternalClaims`, `VerifiedExternalIdentity`, `ExternalVerifier`,
  `ServerAuth`, and `AuthHandles` still claim human/platform ownership, one
  comment cites old implementation commits, and the callback test helper builds
  `PgIssuerResolver` + `JwksCache` + `ExternalVerifier` and stores them in
  `ServerAuth` although `AuthorizationCodeExchange` no longer reads them. All
  callback tests call that helper, so the dead wiring is maintained, not an
  unreachable forgotten test.
- **Observable consequence:** Maintainers are directed to the workload
  verifier for human-login changes, and callback tests falsely advertise a
  dependency on the deleted path, making reintroduction of duplicate human
  verification more likely.
- **Decision-complete correction:** Update the existing rustdoc to describe
  `ExternalVerifier` and its server handles as workload RFC 7523/JWT-bearer
  verification only and remove implementation-commit prose. Delete the unused
  external-verifier/resolver/JWKS construction and fields from the callback
  fixture; rename that existing fixture to describe the issuing-key and human-
  connection state it actually builds. Delete imports/constants left with no
  caller. Preserve production JWT-bearer wiring. Add no new documentation
  file, abstraction, fixture layer, or documentation check.
- **Focused closure proof:** Run the existing callback test target and inspect
  source to confirm no human callback or platform-login documentation/test
  setup claims an `ExternalVerifier` dependency, while the production
  JWT-bearer caller remains unchanged.

### FIND-TASK-009-13 — REVISED — REGRESSION: platform human setup bypasses the process-owned relying party's full discovery

- **Discovery source:** `PERSIST-TEN-001`, narrowed by `followup-review.md`.
- **Violated obligation:** Specification REQ-004, routed into TASK-009, says
  human setup discovers provider endpoints and JWKS and validates the issuer.
  TASK-009 assigns human discovery/JWKS and the per-issuer cache to the selected
  `openidconnect` relying party. The metadata-only path restored by prior
  `FIND-TASK-009-3` belongs to workload setup, whose separate verifier owns key
  retrieval.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/platform/identity.rs:196-267`;
  metadata-only helper at
  `crates/wyrd/wyrd-server/src/components/admin/routes.rs:735-775`;
  existing human owner at
  `crates/wyrd/wyrd-auth/src/platform_login.rs:83-170` and
  `crates/shared/wyrd-auth-oidc/src/relying_party.rs:397-435`.
- **Producer-to-consumer evidence:** `PUT /platform/oidc/connection` calls the
  same `discover_jwks_uri` used by workload issuer creation. That helper invokes
  `ScreenedHttp::provider_metadata`, which deliberately reads only the
  discovery document and returns metadata with no fetched key set. The route
  commits the row without using `ServerAuth::platform_login`. Later begin and
  callback requests use that process-owned `PlatformLogin` and its independent
  cached/full discovery path. Tenant candidate setup already demonstrates the
  appropriate existing owner operation: fresh `RelyingParty::discover`
  replaces the issuer's cached provider before the tested path proceeds.
- **Observable consequence:** Platform configuration can report success when
  standard library discovery cannot fetch or decode the advertised JWKS, and
  reconfiguration of an already cached issuer leaves that process's next login
  on its old discovered provider until normal expiry or an unknown-key refresh.
  Workload setup is correctly unaffected by JWKS availability.
- **Decision-complete correction:** Route platform human configuration through
  the already process-owned `PlatformLogin`/`RelyingParty` fresh discovery
  operation and persist the returned provider's `jwks_uri`. Keep the two
  workload callers on `ScreenedHttp::provider_metadata`. Use the selected
  library's ordinary discovery validation as-is: add no custom key-health
  probe, empty-key policy, cache, invalidation service, setting, retry system,
  provider branch, or cross-replica coordination.
- **Focused closure proof:** Through the existing served platform configure
  route, prove an unavailable or undecodable advertised JWKS refuses the write;
  after priming an issuer, change its standard discovery/key response,
  reconfigure that issuer, and prove the next begin/callback on that process
  uses the fresh provider. Retain the workload admin and boot proofs that setup
  performs zero JWKS requests.

### FIND-TASK-009-14 — REVISED — VIOLATION: the audience migration contracts the schema in the expand release and silently changes existing trust

- **Discovery sources:** `PERSIST-TEN-002`, `SYS-R2-001`, reconciled by
  `followup-review.md`.
- **Violated obligation:** The repository migration contract requires
  expand-and-contract when adjacent application versions overlap: both readers
  must work, writers must emit the overlap representation, and destructive
  contraction occurs only after the old version drains. REQ-004 makes the new
  human audience equal to `client_id`; INV-004 requires the trust transition to
  fail closed rather than silently reinterpret a stored value.
- **Exact location:**
  `crates/wyrd/wyrd-sql/migrations/20261002000001_platform_oidc_client_audience.sql:1-4`;
  candidate reader/writer at
  `crates/wyrd/wyrd-sql/src/queries/platform/identity.rs:59-124`;
  base reader/writer at
  `35a53faa2:crates/wyrd/wyrd-sql/src/queries/platform/identity.rs:60-129`;
  original schema at
  `crates/wyrd/wyrd-sql/migrations/20260601000023_platform_identity.sql:38-53`.
- **Producer-to-consumer evidence:** The base schema stores
  `expected_audience TEXT NOT NULL`; the base reader selects it, the base writer
  supplies it, and the base verifier consumes it. The candidate migration
  drops it immediately, while the candidate reader/writer omit it and the
  verifier consumes `client_id`. Migration-first breaks every overlapping base
  read/write; application-first breaks candidate writes against the base
  non-null schema. An existing mismatch is destroyed and silently changes the
  accepted audience. The repository's tenant-human migration already uses a
  preflight for this exact trust mismatch.
- **Observable consequence:** No rolling/blue-green ordering keeps platform
  OIDC configuration and login compatible, ordinary image rollback is broken,
  and an upgraded mismatched row changes authentication trust without operator
  repair or surviving evidence.
- **Decision-complete correction:** Make this release the conventional expand
  step: preflight existing rows and abort with the established non-secret
  repair instruction when `expected_audience <> client_id`; retain the
  physical column only as an overlap representation; enforce the native
  equality invariant so an overlapping base writer cannot create a new
  mismatch; and have the candidate writer derive the retained value from
  `client_id`. Keep the candidate request/view/verifier free of any independent
  audience input. Contract the column only in a later release after the
  supported base application has drained. Add no public alias, toggle,
  migration mode, compatibility service, or second trust choice.
- **Focused closure proof:** Exercise the existing migration/SQL test surface:
  an equal-valued base row upgrades and is readable by both the base-shaped
  projection and candidate reader; a candidate write stores the retained value
  equal to `client_id`; a mismatched pre-existing row aborts atomically with its
  prior data intact; and a post-migration base-shaped mismatched write is
  refused by the database invariant. Existing contract/codegen and served
  platform tests must continue proving there is no public
  `expected_audience` input.

## Prior-finding closure

| Prior finding | Current source evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | `boot::install_auth` builds one `PlatformLogin`, stores it in `ServerAuth`, and both served platform handlers borrow it; the real route journey covers callback reuse during discovery/JWKS outage. | **CLOSED** |
| `FIND-TASK-009-2` | `PlatformCallbackRequest` carries optional RFC 9207 `iss`; `PlatformLogin::complete` checks it against consumed state and cached metadata before token redemption; the served route journey covers match, missing, and mismatch. | **CLOSED** |
| `FIND-TASK-009-3` | Workload admin creation and boot seeding call metadata-only `ScreenedHttp::provider_metadata`; workload key retrieval remains on `ExternalVerifier`/`JwksCache`. `FIND-TASK-009-13` is the distinct platform-human caller accidentally left on that workload path. | **CLOSED** |
| `FIND-TASK-009-4` | `RelyingParty::cached` uses Moka `try_get_with`; unknown-key handling invalidates and re-enters it once; focused concurrent miss and rotation tests pass. | **CLOSED** |
| `FIND-TASK-009-5` | Explicit human lead direction withdraws the finding and requires no code change. | **WITHDRAWN — MUST NOT REOPEN** |
| `FIND-TASK-009-6` | Human redemption derives verifier audience from `client_id`; the public platform request/view and runtime row expose no second human audience setting. `FIND-TASK-009-14` concerns only the required physical overlap representation, not a restored public trust choice. | **CLOSED** |
| `FIND-TASK-009-7` | `RelyingParty::http` is absent and no equivalent escape hatch was added. | **CLOSED** |
| `FIND-TASK-009-8` | Authorization uses `CsrfToken::new_random`, `Nonce::new_random`, and the library S256 PKCE generator; the Wyrd length constant is absent. | **CLOSED** |
| `FIND-TASK-009-9` | `require_usable_jwks` and `not_tested_reason` each have accurate adjacent rustdoc. | **CLOSED** |
| `FIND-TASK-009-10` | The remediation evidence records exact zero-selection-safe selectors/listing for every named test, each selecting one passing test. | **CLOSED** |

## Validation result

- **Validated ledger:** four new findings: `FIND-TASK-009-11` through
  `FIND-TASK-009-14`.
- **`SPEC_REVISION_REQUIRED`:** **No.** Every correction uses the selected
  OIDC library's default behavior, an existing Wyrd owner, ordinary source
  cleanup, or the repository's explicit and conventional database
  expand-and-contract rule.
- **`BLOCKED`:** **No.** The immutable subject, complete source/caller trace,
  authorities, required reports, and follow-up resolution were available.

The candidate requires bounded remediation. None of the retained corrections
requires a novel Wyrd mechanism, setting, dependency, cache, retry policy,
coordination service, public compatibility surface, permanent check, or test
harness.
