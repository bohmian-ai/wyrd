# TASK-001-008-R7 cumulative re-review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Cumulative candidate: `eb9b2f69cb883fa508ed168f21cb868451e61b82`
- Code candidate: `a9706766f4d4b270ca55daf9bd0d3a8f8716ffad`
- Approved specification:
  `changes/active/admin-principals/spec.md`, revision 12, SHA-256
  `1a2fd760de9012767499cf9ca41d41c21d502ffe6e13613ddf5cdd2328bcf856`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Prior verdict, validated ledger, and remediation:
  `changes/active/admin-principals/review/whole-branch-07/`

The candidate remained unchanged through both review waves.

## Verdict

**SPEC_REVISION_REQUIRED**

The revision-12 authentication architecture is implemented at its principal
runtime boundaries: all five tenant grants use one issuance owner, tenant JWTs
are verified locally from their signed `permissions`, platform authorization
revalidates current state through Postgres, and Bifrost authorizes resolved
table identities at admission. Nine independently validated defects remain.
After review, the user directed that the two rejected pre-existing delegation
defects also be corrected as credential-exchange security work. Approved
revision 12 expressly excludes delegation-chain changes, so revision 13 now
drafts the required authority-attenuation and durable-decision-audit semantics;
implementation must wait for explicit approval.

Remediation task:
`changes/active/admin-principals/review/whole-branch-08/TASK-001-008-R8-close-validated-findings.md`.
That task remains blocked until revision 13 is approved and incorporated.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| TASK-001–TASK-008 cumulative principal, credential, initialization, provisioning, administration, recovery, human identity, client, MCP, CLI, and OpenAPI outcomes | The final owners and recorded principal, platform, identity, CLI, MCP, SQL, OpenAPI, and journey lanes cover the original outcomes | **FAIL** — retained contract, security, data-upgrade, RLS, and scope-drift findings below |
| `R7-AUTH-1`: five grants share one current-state five-minute `permissions` JWT issuer | API key, OIDC login, refresh, workload assertion, and delegation converge on `TenantTokenIssuer`; focused issuance evidence is recorded | PASS |
| `R7-AUTH-2`: tenant verification is local and builds runtime authority from claims | Concrete synchronous `TokenVerifier` owns keys, issuer, audience/settings and has no database/cache dependency | PASS |
| `R7-AUTH-3`: stable Bifrost table identities are checked against exact/schema/global permissions | Oracle builds table permissions from resolved `TableUid`; HTTP scoped coverage exists | **FAIL** — `FIND-admin-principals-R8-6` requires the mandated scoped gRPC proof |
| `R7-AUTH-4`: credential revocation blocks renewal while existing JWTs expire naturally | Revoked credentials cannot exchange; sibling credentials work without epoch ordering; expiry journey is recorded | PASS |
| `R7-AUTH-5`: current tenant, principal, credential, and grants govern new issuance | Shared issuer performs current-state reads before minting | PASS |
| `R7-AUTH-6`: platform requests revalidate current state without a cache | Platform authorization remains `OperatorPool`-backed on every request | PASS |
| `R7-AUTH-7`: rejected epoch/checker/cache/request-time-resolution design is gone from active code and authority | Runtime machinery is deleted | **FAIL** — `FIND-admin-principals-R6-1` retains live stale contracts, an unreachable error, and an unused cache dependency |
| `R7-AUTH-8`: Bifrost authenticates at admission and does not reauthenticate admitted bounded work | Gate verifies before dispatch; the short-token stream journey proves admitted completion and next-admission refusal | PASS |
| `R7-AUTH-9`: Wyrd verification and external OIDC verification remain separate concrete owners | `TokenVerifier` is cryptographic and DB-free; `ExternalVerifier` remains issuance-side | PASS |
| `FIND-admin-principals-13`: invalid local transfer locators return documented problems | Assembled authenticated router coverage proves path/query failures use the canonical problem response | PASS |
| `FIND-admin-principals-R5-2`: revoke misses audit exactly one allowed no-effect decision | Real-Postgres coverage distinguishes logical miss from lookup failure | PASS |
| `FIND-admin-principals-R7-5`: each named test records its literal nonzero selector and result | The evidence table contains a template and `N tests run: N passed` | **FAIL** — exact evidence remains absent |
| Repository security and Rust source-shape rules | Shared client ownership and most Rust boundaries conform | **FAIL** — CLI secrets enter argv/debug and one import is function-scoped |
| Tenant isolation and retained audit durability | `TenantConn`, canonical audit staging, and publication remain the owners | **FAIL** — redundant tenant predicates and an unhandled retained-table/hash upgrade regression remain |
| Public route/schema/problem agreement | Local-transfer and MCP UUID corrections pass | **FAIL** — malformed administrative IDs bypass the published problem contract |
| Revision-12 non-goals and bounded scope | No replacement auth abstraction was added | **FAIL** — unrelated audit-publisher `NOWAIT` policy entered the candidate |
| Formatting and generated consistency | Recorded fmt, lints, codegen, docs, strict rustdoc attribution, and cumulative whitespace checks pass | PASS, subject to the evidence defect above |

## Wave results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | FAIL | `TREV-WB08-1`–`TREV-WB08-3` |
| Repository standards | FAIL | `REPO-R8-1`, `REPO-R8-2` |
| Auth/security domain | FAIL | `AUTH-R8-1`–`AUTH-R8-3` |
| Data/audit domain | FAIL | `DATA-WB08-01`, `DATA-WB08-02` |
| Bifrost domain | FAIL | `BIFROST-R8-1` |
| Contract domain | FAIL | `CONTRACT-08-01`, `CONTRACT-08-02` |
| Ponytail validation | COMPLETE | Nine retained roots; `AUTH-R8-1` and `AUTH-R8-2` rejected as pre-existing, explicitly excluded delegation debt |

## Validated finding ledger

| Finding | Status and class | Violated obligation and consequence | Decision-complete correction and proof |
|---|---|---|---|
| `FIND-admin-principals-R6-1` | REVISED · VIOLATION | `REQ-012`, `REQ-012a`, `INV-013`, `AC-010`, `AC-013`, `R7-AUTH-7`; live source, docs, active specs, MCP/CLI text, OpenAPI errors, and a direct dependency retain the deleted auth model | Rewrite only live contracts to the five-minute permission snapshot, remove unreachable protected-route credential-revoked errors and unused direct server `moka`; audit residual terms and verify docs/OpenAPI/MCP/metadata |
| `FIND-admin-principals-R7-5` | CONFIRMED · MISSING | `VER-002`; the packet cannot prove each named selector selected a nonzero test | Record every literal command, selected count, result, and owning lane with no placeholder |
| `FIND-admin-principals-R8-1` | CONFIRMED · DRIFT | `VER-001`, `VER-005`; auth acceptance would also approve an unrelated audit concurrency policy | Remove only `67b4d0ba`'s production and journey changes; prove the cumulative diff no longer contains that repair |
| `FIND-admin-principals-R8-2` | CONFIRMED · VIOLATION | Secret-handling authority; new CLI secrets enter argv, shell history, and derived debug | Delete secret-valued options, reuse ambient tenant credentials and environment-only platform credentials, carry `SecretString`, and prove help/parser/debug/runtime behavior |
| `FIND-admin-principals-R8-3` | CONFIRMED · VIOLATION | Module-scope import rule | Move `sha2::Digest` to the existing test-module import block and rerun the checksum test |
| `FIND-admin-principals-R8-4` | CONFIRMED · REGRESSION | `REQ-037` and retained audit durability; predecessor tables reject the new fingerprint and old hashes become unreproducible | Evolve exactly the recognized predecessor audit schema by appending nullable `credential_id`; preserve the old hash preimage when it is absent; prove mixed-history upgrade/publication/hash verification |
| `FIND-admin-principals-R8-5` | CONFIRMED · VIOLATION | `REQ-031`, `AC-010`, RLS rules; four `TenantConn` queries duplicate the tenant boundary | Remove only redundant tenant predicates/binds and prove same-tenant behavior plus cross-tenant invisibility through RLS |
| `FIND-admin-principals-R8-6` | CONFIRMED · MISSING | R7 focused proof; scoped permission propagation is not exercised through real gRPC serving | Extend the existing bound-server journey with a scoped bearer, covered query, and pre-stream uncovered refusal; record one exact nonzero selector |
| `FIND-admin-principals-R8-7` | CONFIRMED · INCORRECT | `REQ-036`, `REQ-049`, `AC-014`, `AC-019`; malformed IDs are schema-valid yet return Axum plain text | Publish typed path IDs and map extraction rejection through the existing canonical validation problem; prove tenant/platform route schema and runtime agreement |

Full caller traces, exact locations, evidence, and correction boundaries are in
`findings-validation.md`.

## Prior-finding closure

- `FIND-admin-principals-13` and `FIND-admin-principals-R5-2` are closed.
- The rejected hybrid-design repairs `R7-1` through `R7-4` are closed by
  deleting that architecture, not by retaining its abstractions.
- Stable `FIND-admin-principals-R6-1` remains open in revised form because the
  runtime deletion did not reach every live contract and dependency.
- Stable `FIND-admin-principals-R7-5` remains open because the evidence still
  contains a template and placeholder rather than literal results.

## Verification limits

The reviewers inspected the complete cumulative diff and final source. The
task review did not rerun every long Docker/Postgres lane; it accepted the
recorded final-code-candidate results where they named a concrete lane. The
exact-selector artifact remains insufficient by definition. The standards
review independently reran the tenant-isolation, from-pools, client-tier,
clippy-allow, and whitespace checks using the host's available `python3`
substitution. Strict rustdoc proves documentation hygiene, not architectural
truth. Candidate `eb9b2f69` remained unchanged throughout review.
