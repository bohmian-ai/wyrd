# TASK-005 R2 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `323ce32118ec72752a7736b8d42dd957abf6a094`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Prior review: `changes/active/oidc-production-readiness/review/TASK-005-r1/`
- Remediation reviewed: `changes/active/oidc-production-readiness/review/TASK-005-r1/TASK-005-R1-doc-contract-accuracy.md`

The candidate remained at the stated commit throughout discovery, follow-up,
validation, and verdict preparation. The repository has no `.codegraph/`
directory, so reviewers used the cumulative and remediation diffs, `rg`, and
direct source inspection.

The standing direction was binding: Wyrd uses standard OAuth 2.0/OIDC and
vetted libraries without invented protocol machinery. Documentation blocks
only when it is false, misleading, or omits task-required behavior; placement,
naming, structure, phrasing, and whitespace preferences do not block. The
closed RFC 8693 API-key exchange, ingress-owned `POST /auth/device` rate
limiting, best-effort RFC 7009 revocation, origin-normalized client base URL,
and access-token validity through expiry were not reopened.

## Independent review results

| Review | Result | Material proposals |
| --- | --- | --- |
| Behavior | FAIL | `BHV-R2-001`, `BHV-R2-002` |
| Invariants | FAIL | `INV-R2-001` |
| Repository standards | FAIL | `STD-TASK-005-R2-1`, `STD-TASK-005-R2-2`, `STD-TASK-005-R2-3` |
| Maintainer | FAIL | `MAINT-R2-001` |
| System resilience | PASS | None |
| Security domain | FAIL | `SEC-R2-001` |
| Focused follow-up | RESOLVED | Consolidated endpoint taxonomy and plane-separation claims; classified the whitespace-only discrepancy as non-blocking |
| Ponytail validation | COMPLETE | Retained `FIND-TASK-005-3`, `FIND-TASK-005-4`, `FIND-TASK-005-7`, and `FIND-TASK-005-8` |

All required independent reports are present. The focused follow-up was needed
because the remediation-round proposals exposed a common endpoint-taxonomy
source and sibling consumers not traced in round 1. It resolved the overlap
without an unreviewed path or authority conflict. The fresh Ponytail validator
then checked every proposal against source and produced the final ledger.

## Reconciled acceptance matrix

| Obligation | Reconciled evidence | Result |
| --- | --- | --- |
| REQ-001 / AC-001: OIDC remains optional and existing UI, SDK, operator, and machine paths remain available | Cumulative docs preserve OIDC-off recovery and independent machine credentials; no executable behavior changed | PASS |
| REQ-005: stored provider and secret-bearing workload-issuer secrets use the existing sealing boundary | Cloud-identity and configuration docs now match Human refusal and secret-bearing workload issuer persistence | PASS; prior `FIND-TASK-005-2` closed |
| REQ-018: setup, recovery, client selection, human/workload separation, and operator guidance match the shipped system | Generic IdP setup wrongly requires a confidential client, and the Rust override example names a nonexistent field | FAIL — `FIND-TASK-005-7`, `FIND-TASK-005-8` |
| REQ-021 / INV-003: OAuth endpoints use their standard endpoint-specific wire and tenant/platform planes remain distinct | Sibling authorities and guides still generalize browser/form response shapes and collapse platform exchange into tenant client identification | FAIL — `FIND-TASK-005-3`, `FIND-TASK-005-4` |
| INV-001 / INV-004: tenant selection and OIDC trust remain server-bound and fail closed | State, PKCE, nonce, issuer/JWKS, callback, and provider-failure guidance remain aligned | PASS |
| INV-005: UI and SDKs project server-owned authority | BFF custody, shared-client ownership, and generated language projections remain intact | PASS except the unusable Rust documentation example under `FIND-TASK-005-8` |
| INV-006 and task non-goals | No hosted signup, social login, SAML, SCIM, certified-provider implementation, compatibility route, or new protocol mechanism entered the diff | PASS |
| AC-002 / AC-003: self-hosted and hosted connection setup and tenant isolation are documented | Callback, one-active-connection, role mapping, and tenant-isolation evidence remain present | PASS |
| AC-004 and FIND-TASK-004-8: first-class clients describe RFC 8628 saved login, tenant selection, and newest-login behavior | Shared-client and generated declarations agree, but the Rust programmatic override example cannot compile | FAIL — `FIND-TASK-005-8` |
| AC-005 / AC-006 / AC-007: machine independence, replacement, recovery, logout, and bounded token lifetime are accurate | Workload, activation-stamp, BFF cookie, best-effort logout, and access-token-expiry corrections match source | PASS; prior `FIND-TASK-005-5` and `FIND-TASK-005-6` closed |
| AC-008: provider-agnostic standard OIDC | No provider branch or certification claim entered the implementation; provider notes remain examples | PASS |
| AC-009: architecture, public docs, CLI/UI/SDK surfaces, generated declarations, and shipped behavior agree | Four validated documentation-contract boundaries remain | FAIL — final ledger below |
| Documentation-only scope and locked decisions | No runtime, dependency, endpoint, storage, retry, or compatibility behavior changed; all standing decisions remain intact | PASS |

## Prior-finding closure

| Prior finding | Status |
| --- | --- |
| `FIND-TASK-005-1` | CLOSED: form-endpoint RFC errors and Wyrd-code logging are now scoped to their actual owners. |
| `FIND-TASK-005-2` | CLOSED: trusted issuers are workload-only and secret-bearing variants retain their sealing requirement. |
| `FIND-TASK-005-3` | NOT CLOSED, REVISED: sibling concept and rustdoc consumers still collapse tenant and platform routes/client identification. |
| `FIND-TASK-005-4` | NOT CLOSED, REVISED: sibling architecture and public guidance still generalize endpoint-specific OAuth request, refusal, and status shapes. |
| `FIND-TASK-005-5` | CLOSED: the stateless API boundary and BFF-owned encrypted cookie are accurately distinguished. |
| `FIND-TASK-005-6` | CLOSED: activation is documented as an exact-revision stamp check with no provider re-probe. |

## Validated finding ledger

### FIND-TASK-005-3 — REVISED — INCORRECT

The cross-plane identity guide still sends platform credential issuance through
tenant `/auth/token` and implies tenant-style refresh behavior, while the OAuth
module rustdoc incorrectly says `/auth/platform/token` identifies a registered
client through `OAuthClients`. Production owns platform API-key exchange at
`/auth/platform/token` through `PlatformSessions`, and platform OIDC completes
as an access-only session. Correct the two sibling descriptions without adding
an alias, client registration, refresh path, or shared runtime abstraction.

### FIND-TASK-005-4 — REVISED — INCORRECT

The active design authority, generated/global error guidance, agent guide, and
SSO status paragraph still generalize endpoint-specific OAuth wire behavior.
Browser authorization uses query plus redirect or local HTML outcomes; the four
form endpoints use OAuth JSON; revocation success is empty; and form refusals
can also be `500 server_error` or `503 temporarily_unavailable`. Reconcile only
the documentation and generator with the existing handlers and served OpenAPI;
do not introduce a common envelope, error identity, status, or compatibility
mechanism.

### FIND-TASK-005-7 — CONFIRMED — INCORRECT

The generic IdP procedure requires a confidential client although
`HumanClientAuth::Public` is shipped and valid with PKCE and no client secret.
Describe the existing confidential and public choices without changing the
typed contract, relying-party implementation, or sealing behavior.

### FIND-TASK-005-8 — CONFIRMED — INCORRECT

The Rust configuration example assigns nonexistent `ClientConfig::api_key`.
The shared public type exposes `ClientConfig::credential` and is re-exported
unchanged by the Rust SDK. Correct the example; do not add an alias or new API.

The validator rejected `STD-TASK-005-R2-3`. The immutable range contains an
extra EOF blank line in a prior review artifact, making its recorded
`git diff --check` result literally inaccurate, but the underlying condition
is whitespace-only and has no shipped or required documentation consequence.
It is non-blocking under the standing direction and is not a remediation
requirement.

## Verification and limits

Fresh review-time verification passed:

- `mise run docs:check`
- `mise run codegen:check`
- `mise run fmt:check`
- `mise run lints`

These are the narrow documentation, generation, Rust formatting, and Rust
lint lanes covering the reviewed remediation write set. They establish
rendering, generated parity, formatting, and lint cleanliness, but not the
semantic documentation discrepancies in the validated ledger. No journey,
language suite, live IdP, database, browser suite, or repository aggregate was
run or required.

## Verdict

**FIX_REQUIRED**

Four bounded documentation corrections remain. They require no specification
revision or new product, public API, architecture, security, compatibility,
concurrency, resource-ownership, or persistent-data decision.

Remediation task:
`changes/active/oidc-production-readiness/review/TASK-005-r2/TASK-005-R2-doc-contract-closure.md`
