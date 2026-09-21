# Admin principals whole-branch review 05 — verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `c9e1092bbdb4df3781eb91b0eb33150e00df7623`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 10,
  status `approved`, SHA-256
  `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Prior review and remediation:
  `changes/active/admin-principals/review/whole-branch-04/`

The review covered the complete base-to-candidate range. The candidate stayed
fixed throughout both review waves.

## Verdict

**FIX_REQUIRED**

Nine bounded implementation findings remain. They require no specification
revision and are packaged in
`TASK-001-008-R5-close-validated-findings.md` in this directory.

## Wave results

| Review | Result | Report |
|---|---|---|
| Task implementation | FAIL | `task-review.md` |
| Repository standards | FAIL | `standards-review.md` |
| Security/RBAC/audit | FAIL | `domain-review-security.md` |
| Tenancy/data/durability | FAIL | `domain-review-data.md` |
| Public contract/transport | FAIL | `domain-review-contract.md` |
| Structured Ponytail validation | FIX_REQUIRED | `findings-validation.md` |

All required reports are present. Wave 2 independently traced and dispositioned
all thirteen Wave 1 proposals, consolidated duplicates, and retained only the
nine roots below.

## Acceptance matrix

| Obligation | Strongest evidence | Result |
|---|---|---|
| Principal/credential model, five closed kinds, plane-valid tenancy, credential secrecy and rotation | Cumulative source, schema, SQL, unit, integration and journey evidence | PASS |
| Typed platform/tenant authentication and authorization, verifier-store uncertainty fails closed | Auth owners, extractors, verifier tests and protected-route evidence | PASS |
| Initialization disclosure/rollback/retry and provisioning/admission/recovery | Platform owners and two clean platform-journey runs | PASS |
| Changed human roles invalidate old authority while admitting the successor | Callback/epoch/verifier trace and existing identity journey | FAIL — `FIND-admin-principals-R4-3` |
| Platform federated grant pin, issuance and canonical audit are one correctly attributed transaction | Platform login/session/identity trace | FAIL — `FIND-admin-principals-R4-5` |
| Every permission decision is durably audited, including stable no-effect outcomes | Platform and tenant administrative route trace | FAIL — `FIND-admin-principals-R5-2` |
| Canonical staging, hash chain, sole publisher, strict current Bifrost schema and replay fence | SQL/Bifrost source and recorded integration lanes | PASS |
| Ordered immutable checksum-verified SQL migrations | Base-to-candidate migration checksum inspection | FAIL — `FIND-admin-principals-R5-3` |
| All first-party callers use shared HTTP authentication and bounded renewal | Shared transport, principal handle and MCP adapter trace | FAIL — `FIND-admin-principals-R4-9`, `FIND-admin-principals-R5-4` |
| Runtime OpenAPI is the exact served method/path/error contract with no parallel catalog | Router/OpenAPI/test trace | FAIL — `FIND-admin-principals-13` |
| MCP projects shared typed input/output contracts | Catalog, dispatcher, DTO and journey trace | FAIL — `FIND-admin-principals-R5-5` |
| Candidate-added declarations comply with repository import/type-shape rules | Base-to-candidate declaration scan | FAIL — `FIND-admin-principals-R5-1` |
| Architecture/docs/LLM indexes describe the five-kind, two-plane, split-renewal model | Updated authorities and recorded docs check | PASS |
| Operator journey uses the returned tenant credential through the shipped CLI | CLI and platform journey evidence | PASS |
| Cumulative Rustdoc, generated artifacts and whitespace hygiene | Diff audit, strict rustdoc, codegen and `git diff --check` evidence | PASS |
| Explicit non-goals remain excluded | Complete diff inspection | PASS |

## Validated finding ledger

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-admin-principals-R4-3` | REVISED | INCORRECT | Order the role-change epoch and successor `iat` so equal-second old tokens are refused and the successor is admitted. |
| `FIND-admin-principals-R4-5` | REVISED | INCORRECT | Pin/resolve the platform federated identity, issue the session, and append the correctly attributed audit row in one transaction. |
| `FIND-admin-principals-R4-9` | CONFIRMED | VIOLATION | Remove Wyrd header and 401 renewal policy from MCP and reuse the existing shared transport/auth owner. |
| `FIND-admin-principals-13` | REVISED | VIOLATION | Co-register served operations and OpenAPI, delete duplicate route parsing/catalog knowledge, and declare reachable `/auth/token` failures. |
| `FIND-admin-principals-R5-1` | CONFIRMED | VIOLATION | Replace candidate-added qualified declaration type paths with module imports and bare names. |
| `FIND-admin-principals-R5-2` | REVISED | INCORRECT | Commit existing authorization decisions before stable logical no-effect responses while preserving rollback on store failure. |
| `FIND-admin-principals-R5-3` | CONFIRMED | VIOLATION | Restore the old Vala migration byte-for-byte and add the nullable column in one forward migration. |
| `FIND-admin-principals-R5-4` | CONFIRMED | INCORRECT | Route tenant credential revocation through the existing shared renewal/replay path. |
| `FIND-admin-principals-R5-5` | REVISED | VIOLATION | Publish and consume shared typed MCP input/output schemas, deleting handwritten duplicates. |

The evidence, reachability, exact locations, consequences, corrections, and
focused closure proofs are authoritative in `findings-validation.md`.

## Prior-finding closure

R4-1, R4-2, R4-4, R4-6 through R4-8, R4-10 through R4-13, and R3-6 are
closed for their named boundaries. R4-3, R4-5, R4-9, and stable
`FIND-admin-principals-13` remain open only in the revised forms above.
`FIND-TASK-001-10` remains waived. The withdrawn historical application and
Iceberg compatibility finding remains withdrawn; R5-3 concerns only the live
SQLx migration checksum invariant.

## Verification limits

The R4 packet records the required format, lint, boundary, shared, principal,
platform, identity, CLI, MCP, SQL, Bifrost, codegen, docs, rustdoc, exact-test,
and diff-hygiene lanes as passing. Reviewers did not rerun the long
environment-owning suites. Those results are credible for their selections but
do not exercise the nine retained gaps: same-second epoch equality, unpinned
grant failure, stored platform-human kind, stable no-effect audit commits,
base-checksum migration upgrade, tenant-revoke renewal, method-level OpenAPI
closure and omitted token errors, MCP schema publication, or the declaration
shape scan.
