# TASK-009 review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `6578921d8ced6316c850e4d8f16bd101630a7056`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Routed prior direction: `changes/active/oidc-production-readiness/review/TASK-004-r2/lead-direction-routing.md`

The candidate remained unchanged throughout discovery, follow-up, and independent validation. The review applied the standing direction that Wyrd uses established standards and conventional mechanisms; no retained correction introduces a Wyrd-only mechanism, check, file, setting, or option.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Tenant login and connection testing use `openidconnect` discovery, PKCE, state, nonce, code redemption, and ID-token verification | `RelyingParty` is used by `HumanConnections`; relying-party tests and identity journeys cover success and negative claims | PASS |
| Platform-administrator login uses the same relying party and retained RFC 9207 response-issuer binding | The platform owner uses `RelyingParty`, but the shipped callback carries no response `iss`, performs no RFC 9207 comparison, and its claimed journey bypasses callback completion | FAIL — `FIND-TASK-009-2` |
| Human issuer metadata and JWKS use one per-issuer Moka cache | Tenant ownership is long-lived; platform handlers rebuild the owner per request, and the replacement cache no longer coalesces overlapping misses | FAIL — `FIND-TASK-009-1`, `FIND-TASK-009-4` |
| Unknown `kid` causes at most one forced rediscovery per redemption and then succeeds or fails closed | The redemption path has one retry branch and terminal second verification | PASS, subject to restoring native process-local miss coalescing in `FIND-TASK-009-4` |
| Unsafe discovery/JWKS destinations and token redirects are refused; redirect targets receive no request | All relying-party HTTP uses `ScreenedHttp`; redirect proof covers 307/308 and zero target hits | PASS; routed `FIND-TASK-004-13` is closed for TASK-009 |
| Workload RFC 7523 verification remains on `ExternalVerifier` without changed setup behavior | Verification stays on `ExternalVerifier`, but workload admin and boot setup now eagerly require JWKS through human RP discovery | FAIL — `FIND-TASK-009-3` |
| Provider secrets and security-sensitive values do not reach errors, traces, or logs | Provider-controlled OAuth description/URI text is retained in a loggable error chain | FAIL — `FIND-TASK-009-5` |
| Human ID-token audience is derived from the configured client ID, not entered twice | Tenant resolution derives it; the shared redemption input and platform contract still carry an independent audience | FAIL — `FIND-TASK-009-6` |
| The library and public surface contain only required, standard/conventional mechanisms | An unused transport accessor and a Wyrd-owned state/nonce length remain | FAIL — `FIND-TASK-009-7`, `FIND-TASK-009-8` |
| Changed Rust items satisfy repository documentation rules | One new helper has inaccurate rustdoc and the displaced helper has none | FAIL — `FIND-TASK-009-9` |
| Every named test has exact zero-selection-safe execution evidence | Aggregate lanes are recorded, but exact selectors/listing commands for named tests are absent | FAIL — `FIND-TASK-009-10` |
| `oauth2`'s `reqwest` feature is disabled and no second unscreened client exists | Manifests and feature evidence show the screened adapter is the only human RP transport | PASS |
| Post-regeneration final-candidate evidence is proportionate | `9ef532660` changed only generated Hakari feature-union entries and lock edges; final all-feature/all-target lints plus `check:workspace-hack` compile and verify that delta | PASS; no runtime rerun is required solely for `9ef532660` |
| Non-goals remain excluded | No provider branch, SAML, SCIM, BFF/device-grant redesign, distributed cache, custom retry system, or new compatibility surface was added or is required | PASS |

## Independent review results

| Report | Result |
|---|---|
| `task-review-behavior.md` | FAIL — one proposed regression |
| `task-review-invariants.md` | FAIL — platform RFC 9207 and completion proof |
| `standards-review.md` | FAIL — owner lifetime, rustdoc, exact-selector evidence |
| `maintainer-review.md` | FAIL — three DRIFT proposals |
| `system-review.md` | FAIL — platform lifetime plus a disputed verification proposal |
| `domain-review-oidc-security.md` | FAIL — audience, log disclosure, platform proof |
| `domain-review-cache-concurrency.md` | FAIL — cache coalescing regression |
| `followup-review.md` | RESOLVED — all material conflicts resolved from source |
| `findings-validation.md` | 10 retained findings; no block and no spec revision required |

The follow-up was required because reports conflicted on platform owner lifetime, RFC 9207 closure, cache semantics, and post-Hakari verification sufficiency. It narrowed platform composition to the served platform flow, consolidated platform RFC 9207 with its real callback proof, narrowed cache concurrency to prior Moka coalescing, confirmed the workload regression, and established that the final generated-only Hakari delta needed no repeated runtime lane.

## Validated finding ledger

The decision-complete evidence and corrections are in `findings-validation.md`.

| ID | Classification | Retained outcome |
|---|---|---|
| `FIND-TASK-009-1` | INCORRECT | Reuse one process-owned platform login/relying-party owner across served begin and callback requests. |
| `FIND-TASK-009-2` | MISSING | Carry and verify standard RFC 9207 `iss` on the real platform callback and prove that route end to end. |
| `FIND-TASK-009-3` | REGRESSION | Restore metadata-only screened discovery for workload setup; leave JWKS retrieval with the workload verifier. |
| `FIND-TASK-009-4` | REGRESSION | Restore Moka's process-local coalescing for overlapping issuer misses and bounded unknown-key refresh. |
| `FIND-TASK-009-5` | VIOLATION | Discard provider-controlled OAuth description, URI, body, and source text before forming loggable errors. |
| `FIND-TASK-009-6` | DRIFT | Derive human audience from client ID and delete the duplicate platform human setting. |
| `FIND-TASK-009-7` | DRIFT | Delete the unused `RelyingParty::http` accessor. |
| `FIND-TASK-009-8` | DRIFT | Use the library's conventional state and nonce constructors; delete the Wyrd length knob. |
| `FIND-TASK-009-9` | VIOLATION | Correct the two changed helpers' adjacent rustdoc. |
| `FIND-TASK-009-10` | VIOLATION | Run and record exact selectors/listing commands for every named test. |

## Verification limits and prior-finding closure

The recorded aggregate lanes remain credible for behavior they execute, and final `lints` plus `check:workspace-hack` are sufficient for the generated-only workspace-hack regeneration. They do not prove the unexercised platform callback, concurrent cache coalescing, workload discovery-with-JWKS-outage case, provider-error log redaction, or exact named-test selection. The remediation task requires focused proof for those gaps and the task's broader final lanes on the remediated candidate.

TASK-004 round-2 `FIND-TASK-004-13` is closed on the server side: the screened adapter disables redirects for the token request and the test proves the redirect target receives zero requests. No other TASK-004 finding was routed to TASK-009.

## Verdict

**FIX_REQUIRED**

Remediation task: `TASK-009-R1-relying-party-corrections.md`.
