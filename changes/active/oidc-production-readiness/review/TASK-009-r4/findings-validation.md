# TASK-009 round-4 structured Ponytail validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `0bd3686e8bb763b07376f84661946aadc6200bfb`
- Candidate tree: `86fb8e11681b4d2e60bba2aa7ee3bf2c4f134153`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Current remediation: `changes/active/oidc-production-readiness/review/TASK-009-r3/TASK-009-R3-platform-login-boundary-and-contract.md`
- Cumulative range: `35a53faa216b10651d85c96ce12e34f382cac637..0bd3686e8bb763b07376f84661946aadc6200bfb`
- Latest remediation delta: `04597909203463820b2033c12956f5fe6fcfe1f4..0bd3686e8bb763b07376f84661946aadc6200bfb`

The candidate object and tree matched the immutable subject before and after
validation. This repository has no `.codegraph/` directory, so validation used
the cumulative and latest Git diffs, repository search, and direct source and
caller inspection.

`FIND-TASK-009-5`, `FIND-TASK-009-14`, and `FIND-TASK-009-15` are withdrawn by
binding lead direction. They were not reopened, renamed, or implemented
indirectly. Placement, naming, structure, and wording alone were not treated as
blocking concerns.

## Discovery report reconciliation

The complete discovery set was present and readable:

- `task-review-behavior.md`: **PASS**, no proposed findings;
- `task-review-invariants.md`: **PASS**, no proposed findings;
- `standards-review.md`: **PASS**, no material findings;
- `maintainer-review.md`: **PASS**, no material findings;
- `system-review.md`: **PASS**, no material proposed findings;
- `domain-review-oidc-security.md`: **PASS**, no material findings;
- `domain-review-cache-concurrency.md`: **PASS**, no material findings; and
- `domain-review-persistence-tenancy.md`: **PASS**, no material proposed
  findings.

The proposed-finding union is empty. The reports do not materially conflict,
identify an uncovered reachable path, or leave a repeated-remediation source
untraced. A focused follow-up was therefore not triggered, and no
`followup-review.md` is required.

## Independent source validation

| Required validation | Source and caller evidence | Result |
|---|---|---|
| Trace the producer through every task-relevant consumer and sibling writer | `RelyingParty::{cached,discover,authorize,redeem,verify}` in `crates/shared/wyrd-auth-oidc/src/relying_party.rs` owns full human discovery, the one Moka cache, library state/nonce/S256 generation, code exchange, and ID-token verification. Production caller search reaches tenant begin, common tenant/test callback, candidate testing, platform configuration, and platform begin/callback. Workload setup alone uses `ScreenedHttp::provider_metadata`, while workload assertion verification alone uses `ExternalVerifier`. Platform connection writes converge on `upsert_platform_oidc_connection`; tenant state and connection work remain on their tenant owners. | PASS |
| Prove the reviewed paths are reachable and task-required | Tenant login and connection testing call the boot-owned `HumanConnections`; platform configure, begin, and callback obtain the single boot-owned `PlatformLogin` through `ServerAuth`. The served platform and OpenAPI tests exercise the latest contract delta. These are live server paths named by TASK-009, not dormant or test-only surfaces. | PASS |
| Preserve adjacent lifecycle, security, durability, and availability | Tenant callbacks derive tenant authority from consumed server state, re-check the exact bound connection, and issue only after library verification. Platform state remains single-use and separate from tenant state. Full platform discovery finishes before the audited durable upsert; a failed fetch inserts no new provider and changes no row. Provider failure remains request-scoped, and the global platform credential path is independent. | PASS |
| Apply the deletion and reuse ladder | The candidate deletes the handwritten human discovery, random state/nonce/PKCE, token POST, and human `ExternalVerifier` path. It reuses `openidconnect 4.0.1`, the existing `ScreenedHttp`, Moka, stable Wyrd errors, SQL capability types, and existing tests. No second client, cache, retry system, provider branch, audience option, migration mechanism, test harness, or compatibility surface is needed or present. Further deletion would remove required screened transport, cache, RFC 9207 binding, or durable state. | PASS |
| Consolidate shared causes across prior rounds | The earlier cache-lifetime failures converge on the process-owned `RelyingParty`; human protocol duplication converges on `openidconnect`; human/workload confusion converges on separate relying-party and `ExternalVerifier` owners; and platform setup/runtime divergence converges on the boot-owned platform relying party. The current candidate fixes each at that source rather than adding downstream guards. | PASS |
| Validate the latest public-contract correction | `configure_connection` declares its existing discovery/JWKS `503` in rustdoc and `utoipa`; `discovery_error` maps non-screening discovery failures to `WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`; the served platform test pins status and code while checking the row is unchanged; the served OpenAPI test pins the problem schema and stable code. Malformed and blocked input remain `400`. | PASS |

No discovery proposal required a `CONFIRMED`, `REVISED`, or `REJECTED`
decision. Independent inspection found no new reachable behavioral, security,
tenancy, durability, public-contract, or deletion defect. Assigning
`FIND-TASK-009-17` would therefore be unsupported.

## Final deduplicated finding ledger

**Empty.** The discovery union was explicitly validated against the complete
cumulative candidate, latest remediation delta, task-required callers,
sibling consumers and writers, applicable authorities, and existing proof.

## Prior-finding closure

| Finding | Candidate source evidence | Result |
|---|---|---|
| `FIND-TASK-009-1` | Boot builds one `PlatformLogin`, stores it in `ServerAuth`, and served configure, begin, and callback share its relying party and cache. | CLOSED |
| `FIND-TASK-009-2` | Tenant/test and platform callbacks verify RFC 9207 response `iss` before token redemption. | CLOSED |
| `FIND-TASK-009-3` | Workload setup uses metadata-only `ScreenedHttp::provider_metadata`; workload JWKS retrieval remains with `ExternalVerifier`. | CLOSED |
| `FIND-TASK-009-4` | `RelyingParty::cached` uses Moka `try_get_with`; unknown-key handling invalidates and re-enters that path once. | CLOSED |
| `FIND-TASK-009-5` | Lead direction requires conventional provider diagnostics and no extra redaction mechanism. | **WITHDRAWN — NOT REOPENED** |
| `FIND-TASK-009-6` | Human verification derives its sole trusted audience from `client_id`; no independent human audience input remains. | CLOSED |
| `FIND-TASK-009-7` | The unused lower-level `RelyingParty::http` accessor is absent. | CLOSED |
| `FIND-TASK-009-8` | Authorization uses the library's random state, nonce, and S256 PKCE constructors; no Wyrd length knob remains. | CLOSED |
| `FIND-TASK-009-9` | The materially changed production owners, fields, helpers, and tests retain substantive adjacent rustdoc. | CLOSED |
| `FIND-TASK-009-10` | The remediation record supplies exact, zero-selection-safe named-test evidence; final candidate executions selected and passed the two R3 tests. | CLOSED |
| `FIND-TASK-009-11` | No additional-audience override exists; the standard verifier refusal and adjacent `azp` rule remain. | CLOSED |
| `FIND-TASK-009-12` | Human callback paths consume `VerifiedIdToken`; `ExternalVerifier` remains only on the workload RFC 7523 path. | CLOSED |
| `FIND-TASK-009-13` | Platform setup performs full discovery through the same process-owned cache used by begin/callback, before persistence. | CLOSED |
| `FIND-TASK-009-14` | Lead direction accepts direct removal of the unreleased duplicate audience column and forbids an added overlap mechanism. | **WITHDRAWN — NOT REOPENED** |
| `FIND-TASK-009-15` | Lead direction accepts the public `PlatformLogin::relying_party` accessor because its placement has no material consequence. | **WITHDRAWN — NOT REOPENED** |
| `FIND-TASK-009-16` | Platform configuration's runtime `503`, route declaration, served failure assertions, and served OpenAPI response now agree. | CLOSED |

## Verification assessment

The immutable candidate has repository-wrapped exact evidence for:

- `federated_platform_sign_in_runs_through_the_served_callback`: one selected,
  one passed;
- `the_served_document_describes_the_composed_surface`: one selected, one
  passed; and
- the three focused Moka cache/concurrency tests: three selected, three passed.

Those are the narrowest lanes for the latest remediation and the cache claim.
The final candidate also records green formatting and lint evidence. Under the
binding human direction, full user journeys run at change review; their absence
here is neither a verification limit nor a reason to require another mechanism,
check, file, setting, or option.

## Validation result

**PASS — explicitly validated empty finding ledger.**

All required reports are present, their empty union is supported by source and
caller tracing, every non-withdrawn prior finding is closed, the three withdrawn
findings remain untouched, and no unresolved disagreement or missing source
blocks the review.
