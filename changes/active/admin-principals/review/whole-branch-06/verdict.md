# Admin principals whole-branch review 06 — verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `2c0408b683f7a548cec6dd08b35698d761d33b31`
- Product-code candidate: `5ecc8a4e5a3a76390bc32f00353e558ea6a685d2`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 10,
  status `approved`, SHA-256
  `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Prior review/remediation: `changes/active/admin-principals/review/whole-branch-05/`

The complete base-to-candidate range was reviewed and the candidate remained
fixed throughout both waves.

## Verdict

**FIX_REQUIRED**

Five bounded implementation findings remain. No specification revision is
required. The corrections are packaged in
`TASK-001-008-R6-close-validated-findings.md` in this directory.

## Wave results

| Review | Result | Report |
|---|---|---|
| Task implementation | FAIL | `task-review.md` |
| Repository standards | FAIL | `standards-review.md` |
| Security/RBAC/audit | FAIL | `domain-review-security.md` |
| Tenancy/data/durability | PASS | `domain-review-data.md` |
| Public contract/transport | FAIL | `domain-review-contract.md` |
| Structured Ponytail validation | FIX_REQUIRED | `findings-validation.md` |

Wave 2 independently traced and dispositioned every Wave 1 proposal, rejected
unapproved alternatives, consolidated duplicate roots, and retained only the
five findings below.

## Acceptance matrix

| Obligation | Strongest evidence | Result |
|---|---|---|
| Principal/credential model, five kinds, plane and tenant isolation, fixed-cost refusal and credential secrecy | Cumulative contracts, SQL, auth owners, unit/integration/journey evidence | PASS |
| Initialization, provisioning, admission, recovery, rotation and revocation workflows | Server/SQL owners and platform/principal/CLI journeys | PASS except immediate cached revocation below |
| Authorization changes stop predecessor authority on the next request | Resolver, verifier, boot and epoch-writer caller trace | FAIL — `FIND-admin-principals-R6-1` |
| Federated platform pin/session/audit atomicity and stored-kind attribution | Platform session owner and focused R5 proof | PASS |
| Every evaluated authorization decision is durably audited, including stable no-effect results | Administrative route caller trace and focused R5 proof | FAIL — `FIND-admin-principals-R5-2` |
| Tenant RLS, operator boundary, immutable migrations, audit staging/hash/publication and strict current Bifrost schema | Data-domain source trace and independent focused migration/audit proofs | PASS |
| First-party HTTP authentication and bounded renewal have one shared owner | Shared transport, principal revoke and MCP transport trace | PASS |
| Runtime OpenAPI exactly describes every served Wyrd method/path/error | Storage route/URL producer/client/CLI trace and runtime contract suite | FAIL — `FIND-admin-principals-13` |
| MCP publishes and consumes one typed input/output contract | Shared DTO, catalog and dispatch trace | FAIL — `FIND-admin-principals-R5-5` |
| Candidate-added Rust declarations follow import/bare-name rules | Cumulative declaration scan | FAIL — `FIND-admin-principals-R5-1` |
| Architecture/docs/generated artifacts/operator journey and explicit non-goals | Current authorities, docs/codegen and cumulative diff | PASS except stale OpenAPI proof guidance folded into finding 13 |

## Validated finding ledger

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-admin-principals-13` | REVISED | VIOLATION | Make local upload/download representable, typed and co-registered in runtime OpenAPI, extend exact proof, and update stale proof guidance. |
| `FIND-admin-principals-R5-1` | CONFIRMED | VIOLATION | Remove the remaining candidate-added qualified type paths from declarations. |
| `FIND-admin-principals-R5-2` | REVISED | INCORRECT | Commit existing decisions for the additionally traced stable no-effect outcomes without weakening store-failure rollback. |
| `FIND-admin-principals-R5-5` | REVISED | VIOLATION | Use shared typed UUID identifiers so advertised MCP schemas match dispatch validation. |
| `FIND-admin-principals-R6-1` | REVISED | INCORRECT | Delete stale epoch memoization and read the authorization epoch directly on every verification request. |

Full locations, reachability, consequences, decision-complete corrections and
focused closure proofs are recorded in `findings-validation.md`.

## Prior-finding closure

R5 closes `FIND-admin-principals-R4-3`, `FIND-admin-principals-R4-5`,
`FIND-admin-principals-R4-9`, `FIND-admin-principals-R5-3`, and
`FIND-admin-principals-R5-4`. Findings 13, R5-1, R5-2 and R5-5 remain open only
in the revised forms above. Earlier closures, the full waiver of
`FIND-TASK-001-10`, and the withdrawn historical application/Iceberg
compatibility proposal are unchanged.

## Verification limits

The R5 packet records the required focused and broader lanes as passing, and
the data reviewer independently reran migration and no-effect proofs. Those
results are credible for their selections, but none proves documented local
transfer operations, current nonzero OpenAPI guidance, invalid UUID rejection
by advertised MCP schemas, production-constructor epoch reads after cache
warming, the newly traced no-effect branches, or a cumulative declaration scan
covering the residual sites.
