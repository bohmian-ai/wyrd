# Admin principals whole-branch review 07 — verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Cumulative base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `ddd80c7a7b723a2f39a729e5aebe6b6072d2b983`
- Latest remediation range: `4d185da9cc2805940786416d0392f4c73bf337c2..ddd80c7a7b723a2f39a729e5aebe6b6072d2b983`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 10,
  status `approved`, SHA-256
  `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Prior review and remediation:
  `changes/active/admin-principals/review/whole-branch-06/`

## Verdict

**FIX_REQUIRED**

The R6 candidate closes the epoch memoization, named no-effect branches, MCP
UUID mismatch, route omission, and qualified declaration inventory at their
original boundaries. Eight independently validated roots remain. All are
bounded implementation, repository-compliance, contract, documentation, or
evidence corrections under the approved specification; no specification
revision is required.

## Review results

| Required review | Result | Report |
|---|---|---|
| Task implementation | FAIL | `task-review.md` |
| Repository standards | FAIL | `standards-review.md` |
| Security/RBAC/auth/audit/tenancy | FAIL | `domain-review-security.md` |
| Persistent data/SQL/RLS/concurrency | FAIL | `domain-review-data.md` |
| HTTP/OpenAPI/MCP/client contracts | FAIL | `domain-review-contract.md` |
| Structured Ponytail validation | FIX_REQUIRED | `findings-validation.md` |

## Acceptance matrix

| Obligation | Candidate evidence | Verification evidence | Result |
|---|---|---|---|
| Uninterrupted overlapping credential rotation and next-request predecessor revocation | Direct epoch read is live, but credential revocation stores a sub-second epoch while successor JWT `iat` is whole-second | Existing journey exchanges the surviving credential but does not spend its returned token | FAIL — `FIND-admin-principals-R7-1` |
| Principal status gates every live token on the next request | The combined service-account read returns only an optional epoch; missing/deleted rows collapse to `None` | No production-wired warm-token proof follows Card/principal deletion | FAIL — `FIND-admin-principals-R7-2` |
| Local transfer routes serve the exact documented Wyrd problem contract | Routes are typed and co-registered, but Axum query/path extraction can reject before `WyrdErrorResponse` | OpenAPI metadata is tested; malformed runtime extraction is not | FAIL — `FIND-admin-principals-13` |
| Authorization epoch/admission is authoritative rather than cached | Runtime cache/listener/fan-out are gone and the read is one round trip | Trait and journey rustdoc still prescribe or claim the deleted cache | FAIL — `FIND-admin-principals-R6-1` |
| Every stable no-effect authorization result commits one decision; store failures fail closed | Most R6 branches commit correctly; principal revoke rolls back stable misses and swallows lookup errors | No focused miss/error proof covers this route | FAIL — `FIND-admin-principals-R5-2` |
| Library database access uses the repository's two sanctioned connection owners | Resolver still stores and accepts raw `PgPool` after material rewrite | Source-shape rule is directly violated | FAIL — `FIND-admin-principals-R7-3` |
| Tenant-scoped SQL relies on forced RLS rather than duplicate tenant predicates | New admission subqueries run on `TenantConn` but repeat `wyrd.current_tenant()` filters | Tenant-isolation lane is green but cannot waive the explicit rule | FAIL — `FIND-admin-principals-R7-4` |
| Every named Rust proof has an exact, nonzero selector record | Broad required lanes are recorded green | Only one named Rust test has an exact selector in the evidence | FAIL — `FIND-admin-principals-R7-5` |
| Typed MCP UUID inputs/outputs | Shared DTOs use `PrincipalId`/`Uuid`; duplicate parser is removed | Schema accept/reject and output validation are recorded | PASS — prior `FIND-admin-principals-R5-5` closed |
| Candidate-added Rust declaration types use top-level imports and bare names | R6 aliases every validated inventory site | Cumulative scan, format, and lint evidence are recorded | PASS — prior `FIND-admin-principals-R5-1` closed |
| Other original TASK-001 through TASK-008 obligations and non-goals | Complete cumulative source and diff inspection found no additional retained root | Recorded unit, integration, journey, SQL, Bifrost, storage, codegen, examples, and docs lanes | PASS subject to the eight failures above |

## Validated finding ledger

- `FIND-admin-principals-R7-1` — same-second overlap rotation can return a
  successor token that verification immediately refuses.
- `FIND-admin-principals-R7-2` — the authoritative service-account admission
  read does not distinguish an active principal from a missing, suspended, or
  deleted one.
- `FIND-admin-principals-13` — local upload/download extractor failures can
  bypass their published problem media and stable errors.
- `FIND-admin-principals-R6-1` — the shared revocation trait and journey prose
  still prescribe or claim the deleted epoch cache.
- `FIND-admin-principals-R5-2` — principal-revoke stable misses discard the
  decision, and lookup failures masquerade as not-found.
- `FIND-admin-principals-R7-3` — the materially rewritten resolver retains a
  raw application pool instead of `WyrdPostgres`.
- `FIND-admin-principals-R7-4` — the new `TenantConn` admission subqueries
  duplicate forced RLS with manual tenant predicates.
- `FIND-admin-principals-R7-5` — completion evidence omits mandated exact,
  nonzero selectors for named Rust tests.

The complete validated evidence, caller traces, proposal dispositions, minimal
corrections, and focused closure proofs are in `findings-validation.md`.

## Prior-finding closure

- `FIND-admin-principals-R5-1` and `FIND-admin-principals-R5-5` are closed.
- `FIND-admin-principals-R5-2` remains open only for principal revoke.
- `FIND-admin-principals-13` is closed for representation, registration,
  binary schemas, and proof guidance, but remains open for extractor failures.
- `FIND-admin-principals-R6-1` is closed for runtime cache/listener removal and
  authoritative reads, but remains open for contradictory rustdoc.
- Earlier whole-branch-06 closures remain closed.

## Verification limits

This review was static. The recorded broad lanes are credible for their stated
selections, but they do not exercise the eight retained gaps and do not include
all exact focused selector records required by the remediation packet. The
candidate and approved-spec checksum remained unchanged through both waves.
