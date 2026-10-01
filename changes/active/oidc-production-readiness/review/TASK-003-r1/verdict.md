# TASK-003 review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `4e0ca8d2ecc2940724861cf6884b68f9adf65464`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Approved authority: `SPEC-oidc-production-readiness` revision 5 at
  `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20`

The candidate's current `spec.md` is draft revision 6. TASK-003 is explicitly
bound to revision 5, so the approved revision-5 blob above is the authority for
this review. The complete base-to-candidate range was reviewed. The checked-out
candidate remained unchanged throughout discovery, follow-up, and validation.

## Independent review results

| Required report | Result | Proposed findings |
|---|---|---|
| `task-review-behavior.md` | FAIL | `BEH-003-01` through `BEH-003-05` |
| `task-review-invariants.md` | FAIL | `INVREV-001` through `INVREV-003` |
| `standards-review.md` | FAIL | `STD-001` |
| `maintainer-review.md` | FAIL | `MAINT-001` through `MAINT-003` |
| `system-review.md` | FAIL | `SYSTEM-001` |
| `domain-review-security-identity.md` | FAIL | `SEC-ID-001`, `SEC-ID-002` |
| `domain-review-persistence-concurrency.md` | FAIL | `PC-001` |
| `followup-review.md` | RESOLVED | No new finding |
| `findings-validation.md` | FIX_REQUIRED | `FIND-TASK-003-1` through `FIND-TASK-003-9` |

All required discovery and validation reports are present. No reviewer was
unavailable, and no report was converted into a verification limit.

## Follow-up decision

A focused follow-up was required because the discovery reports disagreed about
the production tenant projection. It resolved both paths from source:

- forged session-cookie suffixes reach the rendered chooser, but the later
  server read prevents them from becoming tenant authority; the rendered
  selector still violates the task's separate prohibition on browser-derived
  tenant choices; and
- the empty production `tenantId` is reachable typed state and violates the
  required `SessionRead` projection, although current guards prevent a
  demonstrated storage or authorization effect.

No uncertainty remains that requires another discovery pass.

## Reconciled acceptance matrix

| Obligation | Reconciled implementation and proof | Result |
|---|---|---|
| REQ-003: authorized tenant settings projection | Settings project the existing server API with server-issued authority. The current journey proves staging, deactivation, allow, and denial, but not every required replacement action. | FAIL — `FIND-TASK-003-6` |
| REQ-005: sealed browser credentials, mandatory keyring, safe rotation | Browser credentials are sealed, but the canonical inventory/rewrap omits all browser-session envelopes and the internal channel may use non-loopback plaintext. | FAIL — `FIND-TASK-003-4`, `FIND-TASK-003-5` |
| REQ-006: canonical tenant login and server-bound callback | `/t/{tenantKey}/login`, fixed completion, configured public origin, and server-owned state are present. | PASS, subject to REQ-007 issuer binding |
| REQ-007: issuer-bound, fail-closed OIDC flow | The callback discards RFC 9207 `iss` before token-endpoint IO in a multi-issuer, shared-callback topology. | FAIL — `FIND-TASK-003-1` |
| REQ-009: replica-safe production BFF session | Postgres-backed sessions, one-use completion, cookie protections, CSRF, expiry, tenant checks, and logout exist. Unverified chooser hints, incomplete rotation, plaintext channel acceptance, and an incomplete private read contract remain. | FAIL — `FIND-TASK-003-3`, `FIND-TASK-003-4`, `FIND-TASK-003-5`, `FIND-TASK-003-8` |
| REQ-010: OIDC-off entry through existing credential authority | The session path reuses API-key exchange, but cheap early refusals bypass the shared fixed-cost verifier. | FAIL — `FIND-TASK-003-2` |
| REQ-015: independent tenant sessions and safe switching | Target sessions are server-revalidated. The chooser itself is browser-derived, and the required successful two-provider switch journey is absent. | FAIL — `FIND-TASK-003-3`, `FIND-TASK-003-6` |
| REQ-016: old-connection renewal cutoff | Renewal reuses connection lifecycle authority and the journey proves deactivation ends renewal while current access remains bounded. | PASS |
| REQ-018 / AC-009 task-local production UI truth | Production no longer depends on mock auth and callback schemas are generated, but sealing-key guidance becomes false for the new store. | FAIL — `FIND-TASK-003-4` |
| INV-001: untrusted browser/provider data cannot select identity | Effective session tenant comes from the server, but response issuer is discarded and chooser state is rendered from cookie names. | FAIL — `FIND-TASK-003-1`, `FIND-TASK-003-3` |
| INV-003: principal planes remain distinct | No platform, workload, or provider-group path was added to tenant-user authority. | PASS |
| INV-005: UI projects server-owned identity and permissions | Permissions remain server-owned, but the private session projection drops the authoritative tenant id and fabricates an empty value. | FAIL — `FIND-TASK-003-8` |
| AC-001: real OIDC-off UI | The real production BFF journey exists without an IdP or mock flag. Fixed-cost refusal and key rotation remain open. | FAIL — `FIND-TASK-003-2`, `FIND-TASK-003-4` |
| AC-002: provider login, role mapping, allow and deny | The Keycloak journey crosses both BFF replicas and proves mapped allow/deny. The callback still lacks the required issuer mix-up defense. | FAIL — `FIND-TASK-003-1` |
| AC-003: two provider tenants through the browser | The candidate uses one SSO tenant and one OIDC-off tenant; the required two-provider session/mutation/switch coverage is absent. | FAIL — `FIND-TASK-003-6` |
| AC-006: provider replacement through settings | Stage and deactivate are covered; test, activate, remove, replacement login, recovery, and non-inheritance are not. | FAIL — `FIND-TASK-003-6` |
| AC-007: BFF security and fault evidence | Replica, flow binding, replay, caller admission, CSRF, logout, and cutoff have evidence. Issuer mismatch, browser-session key rotation/keyless inventory, and TLS enforcement do not. | FAIL — `FIND-TASK-003-1`, `FIND-TASK-003-4`, `FIND-TASK-003-5` |
| Explicit non-goals and prohibited changes | No browser bearer storage, UI role mapper, password authority, production mock dependency, or commercial stub was added. The browser-derived tenant selector violates one explicit prohibition. | FAIL — `FIND-TASK-003-3` |
| Repository hard rules and minimality | The structural owners and SQL capability boundaries are sound. Mandatory rustdoc is incomplete, one typed projection contains a sentinel, and an unused lifetime branch is speculative drift. | FAIL — `FIND-TASK-003-7`, `FIND-TASK-003-8`, `FIND-TASK-003-9` |

## Validated finding ledger

The decision-complete evidence and corrections are in
`findings-validation.md`. The retained ledger is:

| Finding | Classification | Validated cause |
|---|---|---|
| `FIND-TASK-003-1` | INCORRECT | Authorization responses are not bound to their issuer before token exchange. |
| `FIND-TASK-003-2` | VIOLATION | Browser API-key refusals bypass exactly-one fixed-cost verification. |
| `FIND-TASK-003-3` | VIOLATION | The tenant chooser renders unverified cookie hints. |
| `FIND-TASK-003-4` | INCORRECT | Canonical key inventory and rewrap omit durable browser-session ciphertext. |
| `FIND-TASK-003-5` | VIOLATION | The private BFF credential channel permits non-loopback plaintext. |
| `FIND-TASK-003-6` | MISSING | Required multi-provider and replacement browser journeys are absent. |
| `FIND-TASK-003-7` | VIOLATION | Mandatory rustdoc is missing or attached to the wrong item. |
| `FIND-TASK-003-8` | MISSING | The production private session projection fabricates an empty tenant id. |
| `FIND-TASK-003-9` | DRIFT | `SessionLifetime::Until` is an unused speculative capability. |

## Verification limits

- Reviewers inspected the candidate's recorded green evidence but did not rerun
  Cargo-, mise-, pnpm-, or provider-backed lanes during this read-only review.
- The task records green filtered and unfiltered identity journeys, UI tests and
  typecheck, format, lints, codegen, `test:wyrd`, focused callback and cards
  tests, and `git diff --check`. Those checks do not exercise the nine
  validated gaps.
- CodeGraph was unavailable because this repository has no `.codegraph/`
  directory; reviewers used repository-native search and direct caller/source
  inspection.

## Prior-finding closure

This is the first review round for TASK-003. There are no prior `FIND-*` items
to close or preserve.

## Verdict

**FIX_REQUIRED**

The bounded remediation is specified in
`TASK-003-R1-production-ui-remediation.md`. No validated correction requires a
specification revision or a new product, public API, architecture, security,
concurrency, or persistent-data decision.
