# Admin principals whole-branch review 08 — task implementation review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Cumulative base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate / reviewed HEAD: `eb9b2f69cb883fa508ed168f21cb868451e61b82`
- Code candidate: `a9706766`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 12,
  status `approved`, SHA-256
  `1a2fd760de9012767499cf9ca41d41c21d502ffe6e13613ddf5cdd2328bcf856`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Prior verdict and ledger:
  `changes/active/admin-principals/review/whole-branch-07/{verdict.md,findings-validation.md}`
- Remediation task:
  `changes/active/admin-principals/review/whole-branch-07/TASK-001-008-R7-close-validated-findings.md`

The complete cumulative diff and final source were reviewed. CodeGraph was used
first to trace the five issuance entries, local verification, Bifrost admission
and object authorization, local-transfer extraction, and principal-revoke audit
flow. The completion summary was not used as evidence.

## Overall result

**FAIL**

The replacement tenant-auth architecture is implemented at its principal code
boundaries: the five grants converge on `TenantTokenIssuer`, tenant requests are
verified locally from signed `permissions`, Bifrost authorizes resolved stable
table identities, and the two independent R7 fixes are present. Three bounded
task defects remain. Active source documentation still claims the deleted
NOTIFY/epoch design exists; the evidence packet again does not record the exact
commands and selected counts it expressly requires; and R7 bundled an unrelated
audit-publisher concurrency change that the approved verification scope says is
not this change's responsibility.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-001 / `REQ-001`–`REQ-011`, `REQ-039`: independent principals, five kinds, generic multi-credential ownership, secrecy, overlap rotation | Principal/credential contracts and stores remain separated; `TenantTokenIssuer::issue` loads current principal state without moving grants onto credentials | Principal unit/integration, SQL, platform and CLI journey evidence recorded; cumulative source trace | PASS |
| TASK-002 / `REQ-012`–`REQ-019`, `REQ-031`: one tenant issuance workflow, claim-only request context, typed plane separation, fail-closed permission checks | `issuance.rs:217-437`; `wyrd-auth-verify/src/lib.rs:208-352,550-588`; server extractors call the synchronous verifier | Auth verifier focused tests and principal/platform journeys recorded | PASS |
| TASK-003 / `REQ-020`–`REQ-024`, `AC-001`: explicit one-time initialization, no start-time secret, retry/concurrency safety | Final `boot/init.rs` and server subcommand boundary remain present | Platform journey evidence retained from cumulative candidate | PASS |
| TASK-004 / `REQ-025`–`REQ-028`, `AC-002`, `AC-007`, `AC-008`: provisioning and tenant lifecycle | Final platform provisioning/lifecycle owners and tenant issuance admission read | Platform journey runs and focused issuance status tests recorded | PASS |
| TASK-005 / `REQ-029`–`REQ-031`, `REQ-037`, `AC-004`, `AC-009`, `AC-012`: tenant administration, RLS, scoped principals, transactional decision audit | Principal routes use `TenantConn`; issuance resolves current grants; no-effect branches retain their deciding transaction | Principal integration, tenant-isolation, revoke-miss and scoped-Bifrost proofs recorded | PASS |
| TASK-006 / `REQ-006`–`REQ-010`, `REQ-032`–`REQ-033`, `AC-005`, `AC-006`: credential lifecycle and recovery | Revoked API keys no longer exchange; already-issued JWTs have no revocation lookup; sibling credentials are independent | Rotation, expiry and recovery journey evidence recorded | PASS |
| TASK-007 / `REQ-034`–`REQ-035`, `REQ-041`–`REQ-046`, `AC-011`, `AC-015`–`AC-017`: tenant/platform humans and platform current-state authorization | OIDC stays issuance-side; platform session owner revalidates current credential/principal/grants through `OperatorPool` | Identity and platform journeys recorded | PASS |
| TASK-008 / `REQ-036`, `REQ-047`–`REQ-049`, `AC-013`, `AC-014`, `AC-019`: shared clients, MCP/CLI and runtime OpenAPI | Shared client remains the HTTP owner; local transfers are co-registered typed routes with canonical rejection mapping | CLI/MCP and served-OpenAPI lanes recorded | PASS |
| `R7-AUTH-1`: all five tenant grants use one current-state issuance owner and one five-minute `permissions` JWT | API-key and delegation call `TenantTokenIssuer::issue` at `exchange_api_key.rs:178-189,235-238`; OIDC/refresh use `issue_human_session`; workload uses it at `jwt_bearer.rs:70-78`; `issuance.rs:270-371` owns state/grants/claims/audit | Named issuance, refresh, verifier and identity tests recorded | PASS |
| `R7-AUTH-2`: tenant requests verify locally, build principal from claims, and perform no auth-store read/cache/role resolution | `TokenVerifier` owns only keys, issuer and settings and synchronously verifies at `wyrd-auth-verify/src/lib.rs:211-352`; runtime principal receives the signed `PermissionSet` | Signature/issuer/audience/expiry/malformed-permission tests recorded | PASS |
| `R7-AUTH-3`: Bifrost authorizes exact/schema/global scope after stable-table resolution | `oracle/mod.rs:4697-4752` constructs each required permission from `TableUid` and checks `effective_permissions` | Restricted-table server journey and scoped-grant decode test recorded | PASS |
| `R7-AUTH-4`: revoked A cannot exchange, A's existing JWT remains bounded by expiry, B works immediately | API-key lookup filters revoked credentials; verifier contains no epoch comparison | Rotation/expiry platform journey tests recorded | PASS |
| `R7-AUTH-5`: current principal/tenant/grants govern new issuance only | `issuance.rs:277-317` performs current reads inside the issuing transaction | Suspended-principal and grant-change tests recorded | PASS |
| `R7-AUTH-6`: platform changes apply next request with no cache | Platform session/current-state owner remains database-backed | Platform grant-withdrawal, suspension and credential-revocation tests recorded | PASS |
| `R7-AUTH-7`: no active code or authoritative prose retains epoch/checker/cache/admission/request-time-resolution design | Runtime owners and dependencies were deleted | The packet's claimed zero-result audit is contradicted by active source at `principal_kind.rs:7-8,20-21` | **FAIL — `TREV-WB08-1`** |
| `R7-AUTH-8`: Bifrost verifies at admission and does not reauthenticate admitted bounded work | Gate authenticates metadata before dispatch at `gate/mod.rs:529-545`; no mid-stream verifier exists | `an_expired_bearer_finishes_its_admitted_stream_but_opens_no_other` uses a three-second token and zero skew | PASS |
| `R7-AUTH-9`: one concrete synchronous Wyrd verifier, separate issuance-side external OIDC verifier | `TokenVerifier` is concrete and DB-free; `ExternalVerifier` is a separate issuance dependency | Structural source trace and crate dependency inspection | PASS |
| `FIND-admin-principals-13`: invalid local transfer locators return documented Wyrd problems | Custom extraction maps failures into the canonical error owner | Assembled-router test covers missing/duplicate query and undecodable path at `pg_openapi_contract.rs:694-764` | PASS |
| `FIND-admin-principals-R5-2`: revoke miss commits one allow/no effect; lookup failure rolls back | Final admin/revoke flow distinguishes logical miss from store failure | Named real-Postgres tests recorded | PASS |
| `FIND-admin-principals-R7-5`: every named closure test records an exact command, nonzero selected count, and result | Evidence table names tests and gives one command template | No per-test exact command or selected count is recorded; the literal placeholder `N tests run: N passed` remains at task line 318 | **FAIL — `TREV-WB08-2`** |
| Revision-12 non-goals and `VER-003`–`VER-005`: no replacement auth abstraction, compatibility path, broad-gate obligation, or unrelated-failure repair | Auth replacement adds no checker/cache/factory, but commit `67b4d0ba` changes audit publication lock semantics and rewrites its journey | The packet itself labels this an out-of-task fix; `VER-005` says such failures are not this change's responsibility | **FAIL — `TREV-WB08-3`** |
| Formatting and generated-contract consistency | Candidate tree and generated schemas are committed | `git diff --check` is silent; fmt/lints/codegen/docs results are recorded | PASS, subject to the evidence defect above |

## Proposed findings

### `TREV-WB08-1` — VIOLATION — active source still documents the deleted revocation epoch and NOTIFY channel

- **Violated obligation:** `REQ-012a`, `INV-013`, `AC-010`, R7's “Delete the
  rejected tenant-auth design” section, and `R7-AUTH-7`.
- **Exact location:**
  `crates/wyrd-spec/src/auth/principal_kind.rs:7-8,20-21`.
- **Evidence and reachability:** the canonical wire type's live module and item
  rustdoc say `PrincipalKindTag` is encoded in a “revocation NOTIFY channel” and
  “revocation-epoch lookups.” Both runtime mechanisms were deleted. A candidate
  search also finds conflicting epoch prose in active change specifications,
  including `changes/active/tenant-oidc-federation/spec.md:128,283,302` and
  `changes/active/object-scoped-rbac/spec.md:103`; the R7 evidence instead says
  all residual hits are historical review packets. Old migrations and historical
  review records are legitimate history, but this source documentation is not.
- **Observable consequence:** maintainers are told that a public contract still
  participates in mechanisms that no longer exist, and active dependent change
  authority continues to prescribe removed owners, so subsequent work can
  recreate or attempt to extend the rejected hybrid design.
- **Required testable correction:** remove the NOTIFY/epoch claims from the
  canonical type docs and reconcile active authoritative dependent specs with
  revision 12 (update or explicitly supersede their obsolete requirements).
  Keep historical review records and immutable migrations unchanged. Re-run the
  deleted-symbol/prose audit and record every residual as active defect,
  immutable history, or an unrelated platform concept.

### `TREV-WB08-2` — MISSING — the required exact focused-test evidence is still not recorded

- **Violated obligation:** `VER-002`, `FIND-admin-principals-R7-5`, and the R7
  task's focused-evidence and evidence-table requirements.
- **Exact location:**
  `changes/active/admin-principals/review/whole-branch-07/TASK-001-008-R7-close-validated-findings.md:299-318`.
- **Evidence:** the packet supplies one parameterized command template and a
  list of test names, then asserts every selector ran as the placeholder
  “`N tests run: N passed`.” It records neither each exact executable command
  nor each selected count. This is the same proof boundary R7-5 required; lane
  names and an unverifiable aggregate assertion do not close it.
- **Observable consequence:** review cannot tell whether the listed expressions
  selected the named tests, selected more than one similarly named test, or were
  actually the commands run on the final candidate.
- **Required testable correction:** append the literal final-candidate command,
  selected count, result, and owning lane for every specifically named closure
  test. Reuse the existing tests and runners; add no test or harness.

### `TREV-WB08-3` — DRIFT — R7 changes audit-publisher contention outside the approved auth scope

- **Violated obligation:** approved specification `VER-001`, `VER-005`, the R7
  task's bounded auth outcome, and the review requirement that no unrelated
  change enter the candidate.
- **Exact location:** commit `67b4d0ba`, specifically
  `crates/vala/vala-sql/src/queries/audit_staging.rs:199-249` and
  `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs`.
- **Evidence:** the commit changes the canonical audit publisher from blocking
  `FOR UPDATE` to `FOR UPDATE NOWAIT` and rewrites 142 lines of its crash/replay
  journey. The implementation packet itself calls this an “Out-of-task fix.”
  Audit-publisher cross-tenant contention is not a principal, credential,
  authentication, authorization-plane, initialization, provisioning,
  administrative-surface, or client-consolidation behavior in `VER-001`, and
  `VER-005` expressly says failures outside those surfaces are not this change's
  responsibility to diagnose or fix.
- **Observable consequence:** accepting this task would also approve a separate
  durability/concurrency policy change that was never reviewed under its owning
  audit-publisher requirements; it also makes the auth remediation materially
  larger than needed.
- **Required testable correction:** remove the `67b4d0ba` production and test
  changes from this candidate and handle that audit-publisher defect as its own
  change. Record an out-of-scope lane failure if it recurs; it does not block
  this specification under `VER-005`.

## Prior-finding closure

- Stable `FIND-admin-principals-13` and `FIND-admin-principals-R5-2` are closed
  at their validated runtime boundaries.
- The rejected corrections behind `R7-1`, `R7-2`, `R6-1`, `R7-3`, and `R7-4`
  are closed by deleting the hybrid runtime design, not by preserving its
  abstractions.
- `FIND-admin-principals-R7-5` remains open as `TREV-WB08-2`.
- `R7-AUTH-7` remains open independently as `TREV-WB08-1`.

## Verification limits

This review was static. It did not rerun the long Docker/Postgres suites. The
recorded broad lanes are credible for the behavior they name, but the evidence
artifact does not preserve the exact focused commands or counts required to
independently validate them. Strict rustdoc proves link/lint hygiene, not that
the prose describes the implemented architecture. `git diff --check` over the
cumulative range was silent.
