# TASK-002 Review Verdict — R5

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `b57d43d501c136591125b98fe78352e657b093b6`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Prior remediation tasks: TASK-002 R1 through R4
- Reviewed range: complete cumulative base-to-candidate diff, 86 files, 8,646 insertions and 1,839 deletions

The candidate remained `b57d43d501c136591125b98fe78352e657b093b6`
through both review waves. Review artifacts are outside that immutable subject.
Lead-directed reuse and test commits recorded separately in the evidence tables
were treated as authorized work rather than scope drift and were still checked
for regression.

## Verdict

**FIX_REQUIRED**

The cumulative tenant-login implementation satisfies its executable behavior,
security, tenancy, persistence, concurrency, contract, journey, and non-goal
obligations. All twelve prior findings are closed. One bounded repository-rule
violation remains: the public `refresh_by_hash` helper documents the retired
consume-before-lookup order even though R4 now requires lookup, family lock,
then active/stale classification. The correction is rustdoc-only and requires
no specification revision.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-006 / INV-001: pre-login routing cannot become tenant or provider authority | Opaque one-use state, narrow owner lookup, `TenantConn`, and header/wrong-tenant refusal evidence | PASS |
| REQ-007 / INV-004: PKCE, nonce, redirect, screened IO, signature, key, algorithm, issuer, audience, time, claims, and replay fail closed | Shared verifier now requires `exp`/`iss`/`aud`, validates present `nbf`, and applies OIDC-only numeric non-future `iat`; focused verifier and refusal proofs are green | PASS |
| REQ-008 / INV-002: tenant User identity is exact `(issuer, sub)` and roles come only from current tenant mappings | Exact-`sub` validation, tenant identity/role owners, same-email distinct-User proof, and human-login journey | PASS |
| REQ-013 / INV-003: machine, tenant-human, and platform authority remain separate | Workload verification remains on the generic verifier path; machine-independence and platform evidence remain green | PASS |
| REQ-014–016: provider replacement, tenant isolation, renewal cutoff, provenance, and five-minute access snapshots | Active connection/revision checks, forced RLS, successor provenance, provider-switch/cutoff/migration proofs | PASS |
| Packet renewal contract: serialize each refresh family before classification and preserve replay containment | Family-first PostgreSQL transaction lock, fixed family-before-connection order, route-owned commit, and deterministic ancestor-replay overlap proof | PASS |
| REQ-017: role changes, issuance, refusal, and replay containment use canonical transactional audit | Existing canonical append and rollback/containment evidence | PASS |
| Packet begin/callback/completion contract and authorization-code retirement | Typed begin/state flow, consume-before-provider-IO, sealed one-use completion, fixed callback output, removed legacy grant/client/CLI route, schema/OpenAPI proof | PASS |
| Required real-server journeys and negative boundaries | Four required identity journeys and full 27-test identity lane are recorded green; focused seam proofs close R4 gaps | PASS |
| Prior findings `FIND-TASK-002-1` through `FIND-TASK-002-12` | Independently revalidated on current source by both waves | PASS |
| Lead-directed reuse and test commits | Recorded separately, authorized, and checked for regression | PASS |
| Accurate substantive rustdoc for materially modified Rust items | `refresh_by_hash` still states the pre-R4 consume-then-lookup order (`FIND-TASK-002-13`) | **FAIL** |
| Non-goals and scope | No compatibility route, alternate audit path, new dependency, TASK-003 BFF flow, TASK-004 CLI persistence, or new identity model entered the candidate | PASS |

The complete requirement matrix is preserved in `task-review.md`; authority and
domain coverage are preserved in the other Wave 1 reports.

## Wave 1 results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | PASS | None |
| Repository standards | PASS | None |
| Security/OIDC/RBAC domain | PASS | None |
| Tenancy/persistence/concurrency domain | PASS | None |

Wave 2 confirmed the task, security, and tenancy/data empty ledgers. It
overturned the standards empty ledger after tracing both production callers of
`refresh_by_hash` and retained one new documentation finding.

## Validated finding ledger

| Finding | Status | Classification | Violated obligation | Decision-complete correction and closure proof |
|---|---|---|---|---|
| `FIND-TASK-002-13` | CONFIRMED; newly demonstrated in Wave 2 | VIOLATION | Mandatory accurate, substantive rustdoc for materially modified Rust items | Keep the helper and executable order unchanged. Update only `refresh_by_hash` rustdoc to describe lookup of any lifecycle state, family identity lookup before family lock/classification on rotation, and lookup use by revocation. Inspect both production callers; run `mise run fmt`, `mise run lints`, and `git diff --check`. |

Full caller traces, reachability evidence, rejected broader alternatives, and
the minimum correction boundary are in `findings-validation.md`.

## Prior-finding closure

| Prior finding | Result |
|---|---|
| `FIND-TASK-002-1` — exact human `sub` identity | CLOSED |
| `FIND-TASK-002-2` — OIDC `azp` validation | CLOSED |
| `FIND-TASK-002-3` — canonical role-change audit | CLOSED |
| `FIND-TASK-002-4` — advertised asymmetric algorithm enforcement | CLOSED |
| `FIND-TASK-002-5` — RLS-only login-state transitions | CLOSED |
| `FIND-TASK-002-6` — owner-bound state lookup | CLOSED |
| `FIND-TASK-002-7` — redacted PKCE verifier | CLOSED |
| `FIND-TASK-002-8` — required Rust-item documentation inventory | CLOSED |
| `FIND-TASK-002-9` — module-top imports | CLOSED |
| `FIND-TASK-002-10` — accurate algorithm-helper rustdoc | CLOSED |
| `FIND-TASK-002-11` — mandatory OIDC binding/time claims | CLOSED |
| `FIND-TASK-002-12` — refresh-family replay/rotation serialization | CLOSED |

`FIND-TASK-002-13` is distinct from prior finding 8: it concerns a helper
contract made stale by the later R4 ordering change.

## Verification limits

- Reviewers inspected the complete cumulative diff, applicable authorities,
  all prior remediation, relevant complete bodies and callers, migrations,
  contracts, generated artifacts, and tests. The repository has no
  `.codegraph/` index, so Git, `rg`, and direct source inspection were used.
- Fresh review evidence includes the focused ID-token verifier test and
  cumulative `git diff --check`. Reviewers did not rerun costly Docker,
  Postgres, Keycloak/Dex, or broad workspace lanes.
- Recorded candidate evidence is green for the focused deterministic refresh
  overlap proof, four required identity journeys, the full identity lane,
  principals unit/integration, SQL, tenant isolation, codegen/docs, format,
  lints, and diff hygiene.
- No reviewer exceeded the 20-minute cutoff and no required review report is
  missing.
- TASK-003 BFF completion, TASK-004 CLI handoff persistence, and live
  Okta/Entra qualification remain intentional non-goals.

## Remediation

Implement `TASK-002-R5-refresh-lookup-rustdoc-correction.md` through
`$wyrd-implement`, then reassess the complete original base-to-new-candidate
range.
