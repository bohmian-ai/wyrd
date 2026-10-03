# TASK-005 review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `e3a47a05d931c010f4c70c75edea2d23c447108b`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`

The candidate remained at the stated commit throughout discovery, validation,
and verdict preparation. The complete base-to-candidate range was reviewed.
The repository has no `.codegraph/` directory, so reviewers used the complete
Git diff, `rg`, and direct source inspection.

## Reconciled acceptance matrix

| Obligation | Implementation and review evidence | Result |
| --- | --- | --- |
| REQ-001 / AC-001: OIDC remains optional; existing credentials, machine identity, and recovery remain usable | Architecture, UI, and self-hosting docs preserve the OIDC-off paths; no executable behavior changed | PASS |
| REQ-005: only stored provider and workload-issuer client secrets require sealing | The server boot rustdoc and most operator guidance match the rewrap owner, but the workload configuration guide denies supported secret-bearing issuers | FAIL — `FIND-TASK-005-2` |
| REQ-018: operator and IdP responsibilities, setup, callback, connection lifecycle, role mapping, CLI login, saved-login selection, independent machine identity, and recovery are documented accurately | The primary SSO guide covers the required surface, but workload issuer scope, platform exchange routing, BFF cookie custody, and activation outage behavior remain inaccurate | FAIL — `FIND-TASK-005-2`, `FIND-TASK-005-3`, `FIND-TASK-005-5`, `FIND-TASK-005-6` |
| REQ-021: OAuth endpoints use their standard request, success, and error contracts | The endpoints and form encoding are documented, but the docs overgeneralize Wyrd error identities and RFC 6749 section 5.1 success bodies | FAIL — `FIND-TASK-005-1`, `FIND-TASK-005-4` |
| INV-001 / INV-004: tenant and connection selection remain server-bound and fail closed | Callback, issuer, state, PKCE, nonce, and failure guidance preserve the shipped trust boundary | PASS |
| INV-003 / INV-005: platform, tenant-user, and workload planes remain distinct; clients project server authority | The implementation remains distinct, but one operator sentence directs the platform grant to the tenant route | FAIL — `FIND-TASK-005-3` |
| INV-006 and task non-goals: no hosted signup, social login, commercial stub, SAML, SCIM, certified-provider list, or provider-specific implementation | Complete diff and source inspection found none | PASS |
| AC-002 / AC-003: self-hosted and hosted OIDC setup, callback, role, and tenant-isolation evidence is mapped | Required owner journeys are listed from owning-task evidence and were not rerun | PASS |
| AC-004 and FIND-TASK-004-8: Rust, Python, and TypeScript describe RFC 8628 saved login, newest-login default, and selected-tenant refusal | Shared-client source docs, PyO3 source/stubs, TypeScript source/native/generated declarations, CLI reference, and client configuration agree; codegen and N-API drift checks passed | PASS |
| AC-005: machine identity remains independent from human SSO | Architecture and public docs preserve API-key and JWT-bearer paths; no executable path changed | PASS |
| AC-006 / AC-007: provider replacement, test sign-in, failure, recovery, rotation, and logout behavior are documented accurately | Most behavior is correct, including best-effort revocation and access-token validity through expiry; activation liveness and BFF cookie statements are not | FAIL — `FIND-TASK-005-5`, `FIND-TASK-005-6` |
| AC-008: provider-agnostic OIDC with short common-provider examples and no certification list | Generic setup and five short examples are present; no provider branch was added | PASS |
| AC-009: architecture, docs, CLI, UI, SDKs, and generated declarations agree | Mechanical parity checks pass, but six semantic documentation contradictions remain | FAIL — `FIND-TASK-005-1` through `FIND-TASK-005-6` |
| Required deletion and closed decisions | No login handoff, private BFF channel, browser-session rows, sealed completion, or session-sealing key model remains. RFC 8693 API-key exchange, ingress device-page rate limiting, best-effort RFC 7009 revocation, origin-normalized client URLs, and access-token validity through expiry remain unchanged | PASS |

## Independent reviews

| Review | Result | Proposed findings |
| --- | --- | --- |
| Behavior | FAIL | `BHV-001`, `BHV-002` |
| Invariants | FAIL | `INV-REV-001`, `INV-REV-002` |
| Repository standards | FAIL | `STD-TASK-005-1` |
| Maintainer | FAIL | `MAINT-001` |
| System resilience | FAIL | `SYS-001` |
| OAuth/OIDC security domain | FAIL | `SEC-001`, `SEC-002`, `SEC-003` |
| Structured Ponytail validation | Six retained findings | `FIND-TASK-005-1` through `FIND-TASK-005-6` |

The discovery claims were additive or overlapping rather than materially
conflicting, and no common source remained unexplored, so no focused follow-up
review was needed. The fresh Ponytail reviewer independently validated every
proposal, consolidated duplicates, and moved generated-file corrections to
their source owners. No proposal was rejected.

## Validated finding ledger

| Finding | Status | Classification | Required outcome |
| --- | --- | --- | --- |
| `FIND-TASK-005-1` | REVISED from `BHV-001`, `SEC-003` | INCORRECT | Scope Problem Details and Wyrd catalog-code guidance away from the four OAuth form endpoints; accurately qualify logging; update generators before generated pages and preserve the RFC error body without inventing another identity |
| `FIND-TASK-005-2` | REVISED from `BHV-002`, `INV-REV-001` | INCORRECT | Describe trusted issuers as workload-only, route human setup to tenant OIDC connections, and state when secret-bearing workload issuers require the existing sealing key |
| `FIND-TASK-005-3` | CONFIRMED from `INV-REV-002` | INCORRECT | Qualify `/auth/token` as tenant-plane issuance and name `/auth/platform/token` as the existing platform exchange route |
| `FIND-TASK-005-4` | CONFIRMED from `STD-TASK-005-1`, `SEC-002` | INCORRECT | Document token success, device authorization success, and revocation success under their distinct RFC-owned response shapes |
| `FIND-TASK-005-5` | CONFIRMED from `MAINT-001`, `SEC-001` | INCORRECT | Distinguish the stateless Wyrd API server from the encrypted HttpOnly cookie owned by the BFF |
| `FIND-TASK-005-6` | CONFIRMED from `SYS-001` | INCORRECT | State that activation checks the current test stamp and recovery authority but does not re-probe provider liveness during the 15-minute window |

The source traces, observable consequences, correction boundaries, and focused
closure proofs are authoritative in `findings-validation.md` and are packaged
for implementation in `TASK-005-R1-doc-contract-accuracy.md`.

## Verification and limits

The orchestrator reran only the lanes covering this write set, all successfully:

- `mise run docs:check`
- `mise run codegen:check`
- `mise run ts:napi:check`
- `mise run fmt:check`
- `mise run lints`
- `mise run py:format:check`
- `mise run py:lints`
- `mise run py:typecheck`
- `git diff --check 134f605367e65b41f1977d6c70ac8ca8b277a69e..e3a47a05d931c010f4c70c75edea2d23c447108b`

These checks establish rendering, generation, declaration, formatting, lint,
and typing consistency. They do not prove prose semantics, which is why the
source-backed findings remain. Per task and human direction, no full journey
suite or repository aggregate was run or required.

There are no prior TASK-005 findings to reassess. The carried
`FIND-TASK-004-8` obligation is closed by the shared-client, Python, and
TypeScript source and generated declarations plus the passing codegen and
N-API checks.

## Verdict

**FIX_REQUIRED**

The six findings are bounded documentation and source-documentation defects.
They require no new product, public API, architecture, security, compatibility,
cross-service, concurrency, resource-ownership, or persistent-data decision.

