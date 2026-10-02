# TASK-009 round-2 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `1ddc10e21054ddc158f461e8c6d8aa862c32a067`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Prior remediation: `changes/active/oidc-production-readiness/review/TASK-009-r1/TASK-009-R1-relying-party-corrections.md`
- Round-2 remediation: `changes/active/oidc-production-readiness/review/TASK-009-r2/TASK-009-R2-relying-party-corrections.md`

The candidate stayed at the requested commit throughout the review. CodeGraph
was not indexed, so reviewers used repository search, commit-qualified source,
and direct caller tracing. `FIND-TASK-009-5` is withdrawn by
`review/TASK-009-r1/lead-direction-FIND-TASK-009-5.md` and was not reopened.

## Reconciled acceptance matrix

| Obligation | Implementation and proof | Result |
|---|---|---|
| Tenant, connection-test, and platform human login use `openidconnect` through the screened transport | Shared `RelyingParty`; recorded exact tests and complete identity journeys | PASS |
| OIDC ID-token validation follows the selected library and OIDC Core | The candidate overrides the library's additional-audience refusal with a trust-all callback | FAIL — `FIND-TASK-009-11` |
| RFC 9207 refusal occurs before token redemption | Tenant and platform callers use `verify_response_issuer`; served journeys prove zero token calls on refusal | PASS |
| Unknown-key refresh is bounded and overlapping misses coalesce | Moka `try_get_with`; focused final-candidate tests passed | PASS |
| Workload setup reads metadata without acquiring a JWKS availability dependency | Admin and boot use typed metadata-only discovery; workload verification remains on `ExternalVerifier` | PASS |
| Human `ExternalVerifier` ownership is deleted completely | Production human use is gone, but public rustdoc and callback fixture wiring still describe/build it | FAIL — `FIND-TASK-009-12` |
| Platform human setup performs standard discovery/JWKS validation through its process owner | The configure route reuses the workload metadata-only helper and bypasses the boot-owned relying party/cache | FAIL — `FIND-TASK-009-13` |
| Human audience is derived from client ID without an independent public setting | Request, view, and verifier are client-ID-derived | PASS |
| Audience schema evolution preserves trust and supported-version overlap | The migration immediately drops the old non-null column, silently reinterpreting mismatches and making old/new SQL mutually incompatible | FAIL — `FIND-TASK-009-14` |
| State, nonce, PKCE, screening, tenant/platform separation, role mapping, issuance, audit, and prohibited non-goals remain preserved | Source trace and recorded focused/aggregate verification | PASS |
| Exact named-test evidence is durable and zero-selection-safe | The remediation record names every exact selector and one selected passing test | PASS |

## Independent review results

| Review | Result | Material outcome |
|---|---|---|
| Behavior | FAIL | Additional-audience trust override |
| Invariants | PASS | No proposed finding |
| Repository standards | FAIL | Stale human-verifier rustdoc |
| Maintainer | FAIL | Same stale ownership plus dead callback fixture wiring |
| System resilience | FAIL | Immediate schema contraction breaks rolling overlap |
| OIDC/security domain | FAIL | Additional-audience trust override |
| Cache-concurrency domain | PASS | Prior cache finding closed; 3 focused tests passed |
| Persistence/tenancy domain | FAIL | Platform setup discovery and audience migration |
| Focused follow-up | RESOLVED | Narrowed platform discovery; consolidated migration claims |
| Structured Ponytail validation | COMPLETE | Four retained findings, no block or spec revision |

The follow-up was required because the persistence and resilience reviews
proposed different audience-migration correction boundaries and because the
platform setup claim conflicted with broader PASS reports. It resolved both
from repository authority and source. The validator then independently traced
all proposals and rejected any custom probe, cache service, audience option,
compatibility service, permanent check, or new harness.

## Validated finding ledger

| Finding | Status | Classification | Required correction boundary |
|---|---|---|---|
| `FIND-TASK-009-11` | CONFIRMED | DRIFT | Delete the trust-all additional-audience override and use `openidconnect`'s default refusal. |
| `FIND-TASK-009-12` | REVISED | DRIFT | Correct workload-only verifier rustdoc and delete dead human callback fixture wiring. |
| `FIND-TASK-009-13` | REVISED | REGRESSION | Route platform human configuration through the existing process-owned relying party's standard full discovery. |
| `FIND-TASK-009-14` | REVISED | VIOLATION | Make this an expand release: preflight mismatch, retain a derived constrained overlap column, dual-write from client ID, and defer contraction. |

## Prior-finding closure

`FIND-TASK-009-1` through `-4` and `-6` through `-10` remain closed with the
source and proof recorded in `findings-validation.md`. `FIND-TASK-009-5`
remains **WITHDRAWN — MUST NOT REOPEN**. The new migration finding does not
restore a public audience choice; the retained physical column is only the
derived overlap representation required by the repository's conventional
expand-and-contract deployment rule.

## Verification limits

The remediation record reports every exact named selector and the original
TASK-009 aggregate lanes green on code candidate `0b516e235`; the final commit
adds only that evidence. Reviewers reran the focused audience and cache tests
on the immutable final candidate. Existing proof does not cover the four
retained gaps: additional-audience refusal, removal of stale fixture ownership,
served platform configuration with broken/fresh JWKS, or old/new schema
overlap and mismatched-row migration.

## Verdict

**FIX_REQUIRED**

The four bounded findings are decision-complete in
`TASK-009-R2-relying-party-corrections.md`. No specification revision is
required.
