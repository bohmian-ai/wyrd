# TASK-003 verdict: FIX_REQUIRED

The candidate does not yet satisfy every assigned requirement. Five independently validated findings remain; the seven focused server tests and three CLI tests passed, but the affected Operator served-contract test fails and required renewal journeys are incomplete.

## Immutable subject and authority

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Base: `a5b1d54174227b26e2f3eb09e833a9172d32e2ab`.
- Candidate: `c93287b54bf944c8c8cc9323f81c12ba5174369c`.
- Cumulative diff: `git diff a5b1d5417...c93287b54`; actual range contains 10 commits.
- Contract: SPEC-stored-secret-encryption revision 1 at candidate; original `tasks/TASK-003-resource-rotate-and-revoke.md`, assigned REQ-002/006/007/008/009/012.
- Current maintainer scope is authoritative: HTTP and existing CLI owners only; human/platform renewal HTTP only; no SDK/MCP/UI exposure. No ambiguity requires a spec revision to implement the retained corrections.
- Authorities: current AGENTS.md and SQL/docs nested rules; `changes/spec-driven-development.md`; TESTING.md; maintainer/rust/agent-surfaces guides; architecture authority map and security posture stored-secret, permission, tenant-isolation, audit sections.
- User modifications to contributor instruction files are disclosed in `subject.md` and preserved. Root's final check confirms HEAD is still the candidate and `git diff --quiet c93287b54 -- crates sdks docs architecture` plus the spec/task directories exits 0. Only existing instruction edits and the new review directory remain. Review writes only reports/remediation; no implementation, commit, push or deployment.

## Independent coverage

`behavior_rev` reviewed requirements, owner/consumer flow, tests/assertions, surfaces, docs and contributor rules (`task-review-behavior.md`). `domain_rev` reviewed security/tenancy, persistent CAS, lifecycle races, outage/redaction/transaction and refresh boundaries (`task-review-domain.md`). Fresh `ponytail_rev` independently validated all proposals, reconciled coverage and wrote this verdict (`findings-validation.md`). Root orchestrated and executed serial focused checks (`verification.md`).

The domain specialist covers invariant/system aspects of the same five-owner CAS/refresh boundary; separate invariant/system votes would duplicate it. No broad new cross-tier subsystem or substantial abstraction warrants a separate repo/maintainer specialist; primary coverage retains these rules. No selected report is missing. This is the first TASK-003 review; no prior findings or remediation rounds are carried forward.

## Reconciled acceptance matrix

| Assigned requirements / scenario | Implementation and assertion/execution evidence | Result |
| --- | --- | --- |
| REQ-002/006, scenario 1: fresh DEK, unchanged value and identity | Five owners delegate to shared `WyrdKms::renew`; Operator delivery `pg_operator_delivery.rs:1063`, Gateway snapshot `pg_administration_tests.rs:577`, human `admin/identity.rs:859`, issuer `admin/routes.rs:1500`, platform confidential sign-in `platform_admin_e2e.rs:1976`. Seven focused server cases passed; actual consuming journeys still incomplete under FIND-TASK-003-2. | PASS owner seam; FAIL full journey proof |
| REQ-006/008, scenario 2: replacement preserving references without old decrypt | Existing Operator/Gateway/human/platform owners reseal supplied replacement; Operator corrupt-envelope route assertions `pg_operator_connection_routes.rs:365`; existing auth candidate/activation and session effects retained. Trusted issuer cannot preserve bound references through documented actions: FIND-TASK-003-1. | FAIL |
| REQ-007/009, scenario 3: revoke not undone, refresh/restart semantics | Five owner CAS predicates and unchanged refresh/runtime owners; Operator disabled/race route, Gateway revoke race, human deactivation race, issuer/platform deletion races. Focused runs and predecessor restart/replica regression proof identified in task and discovery reports. | PASS bounded lifecycle proof |
| REQ-008/009, scenario 4: permissions, commit safety, redaction, failures | Real TenantConn or platform authority, KMS outside transaction, exact-envelope CAS, typed redacted views; owner outage/race tests pass. New human allowed-decision loss on write acquisition/lock failure: FIND-TASK-003-5. | FAIL audit invariant; PASS secret-state boundary |
| REQ-006/008, scenario 5: shipped HTTP/CLI | Operator/platform real-router journeys and Operator/Gateway CLI journeys pass. Human/trusted renewals lack composed journeys; issuer CLI wiremock proves transport only; session/Gateway consuming observations missing: FIND-TASK-003-2. | FAIL proof |
| HTTP/OpenAPI affected contract | Two served security/problem checks pass; exact Operator operation-set test fails on sixth route: FIND-TASK-003-3. | FAIL |
| REQ-012 documentation | Resource actions and upstream distinctions present; inaccurate per-owner errors and unchanged-view claim: FIND-TASK-003-4. | FAIL |
| Scope and performance invariants | Cumulative source excludes migrations, SDK/MCP/UI, dependencies and runtime hot-path changes. Administrative KMS calls occur outside DB transactions; existing consuming paths/refresh remain. No benchmark assigned or requested. | PASS |

## Retained findings and remediation

The authoritative finding details and validation dispositions occur once in `findings-validation.md`:

- FIND-TASK-003-1: missing trusted-issuer replacement preserving bound references.
- FIND-TASK-003-2: incomplete composed renewal and consumer/session proof.
- FIND-TASK-003-3: failing served Operator contract check.
- FIND-TASK-003-4: inaccurate renewal response/effect reference.
- FIND-TASK-003-5: missing allowed audit decision on human persistence failure.

One cohesive remediation task: `TASK-003-R1-complete-resource-actions-and-proof.md`. No implementation is launched by review. Acceptance remains withheld until these settled-contract findings close.

## Verification and process

See `verification.md` for exact commands, selected tests, run IDs, setup and results: seven server tests PASS; three CLI tests PASS (issuer uses wiremock); served OpenAPI two PASS/one FAIL at the exact-set assertion. The failure followed successful boot and GET, not setup failure. Managed Postgres migration setup and `git diff --check` passed. Full lint/codegen/docs/dependency checks remain implementation-reported, not rerun claims.

Owner Transit fixtures are mocked HTTP endpoints, not live Vault processes. Prior TASK-001 records real Vault provider execution; no repeated live-provider run was needed for this bounded review. No full aggregate, live-cloud lane or benchmark was run. There is no runtime hot-path change requiring additional wait/performance qualification.

**Behavioral verification: FIX_REQUIRED. TDD compliance: VIOLATED.** TASK-003 records compile-only RED and production-before-executed-RED cases. Later GREEN evidence cannot establish test-first history; preserve the violation without demanding retroactive RED or needless production churn.
