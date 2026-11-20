# TASK-003 R1 verdict: FIX_REQUIRED

All five original findings are closed by the remediation. One new, independently validated contract defect remains: the new trusted-issuer replacement operation documents the KMS 503 code for a store-failure branch that actually returns the existing auth-store 503 code. This requires a narrow annotation and served-contract assertion correction; it does not require changing runtime behavior.

## Subject and authority

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Original cumulative base: `a5b1d54174227b26e2f3eb09e833a9172d32e2ab`.
- Prior candidate: `c93287b54bf944c8c8cc9323f81c12ba5174369c`.
- Reviewed remediation candidate: `d3db737e5fccacbe02421ea135fd5327b6335d79`.
- Evidence/workspace commit: `fa990be1989959de963d062ad8a47336b40c62da`, a direct child adding only seven prior review/evidence files. Production source, tests, public documentation, approved spec/original task and build configuration are identical to the candidate. Existing user instruction edits are preserved.
- Remediation diff: `git diff c93287b54 d3db737e5`; cumulative diff: `git diff a5b1d5417...d3db737e5`.
- Contract: SPEC-stored-secret-encryption revision 1; original TASK-003; prior validated findings and implemented `TASK-003-R1-complete-resource-actions-and-proof.md` in sibling `task-003-c93287b54/`.
- Current maintainer scope: HTTP and existing CLI owners, human/platform renewal HTTP only, no SDK/MCP/UI or migrations. Applicable authorities are AGENTS.md and SQL/docs directory rules, the spec-driven workflow, TESTING.md, maintainer/rust/agent-surface guides, architecture's open-client protocol and security stored-secret/permission/tenant/audit boundaries.

This is the first remediation review, not a restart of original discovery. The changed source and affected paths were inspected; prior source-backed acceptance evidence for unchanged owners remains applicable. The evidence-only child commit is explicitly accounted for rather than treated as an unexplained candidate change.

## Independent assignments

Fresh `r1_ponytail_rev` checks prior finding closure, the changed issuer HTTP/CLI/SQL owner, shared human audit staging and callers, new user/consumer assertions, OpenAPI and docs. Root orchestrates and executes focused verification, without acting as a reviewer. The prior primary/domain reports retain unchanged security/lifecycle coverage. No extra broad specialist is selected for those same correction boundaries; there is no new runtime subsystem, migration, hot-path behavior or abstraction requiring duplicate discovery.

The validator's one newly proposed issue was separately source-checked by fresh `r1_contract_check`, scoped only to the new issuer operation's 503 declaration, mapper and served-contract assertions (`focused-contract-validation.md`). No reviewer validates its own new finding. Required selected reports are present.

## Reconciled acceptance and closure matrix

| Requirements / original scenario | Source, assertion and execution evidence | Result |
| --- | --- | --- |
| REQ-002/006, scenario 1: renewal preserves value/identity | Original five-owner facade/CAS evidence retained. New `identity_e2e.rs:5636` exercises HTTP human renewal, existing access/refresh and fresh confidential login; `compatible.rs:937` dispatches after renewal and asserts the same upstream key and no warm-call unwrap. Executions and limits are bound in `verification.md`. | PASS; FIND-TASK-003-2 closed |
| REQ-006/008, scenario 2: replacement preserves references without old decrypt | New typed PUT/CLI path; `admin/routes.rs:645`, owner seal at `pg_resolvers.rs:900`, exact-byte TenantConn CAS at `trusted_issuers.rs:222`; owner test at `routes.rs:1794` observes new resolver secret, retained binding, corrupt-old-envelope success, refusals, outage and losing deletion race. Real CLI journey preserves workload exchange. Other owners unchanged. | PASS; FIND-TASK-003-1 closed |
| REQ-007/009, scenario 3: revocation/refresh and race safety | Original five-owner/restart/replica evidence retained. New issuer replacement cannot insert a deleted row and loses the deletion race; existing auth resolver and Gateway refresh owners remain unchanged. | PASS |
| REQ-008/009, scenario 4: tenant authority, safe commit, redaction, audit | New issuer flow authorizes/stages before fallible work; reads commit before KMS; CAS is tenant-bound and leaves bindings untouched. `HumanConnections::begin_locked` at `connections.rs:851` stages once before acquisition/lock; `admin/identity.rs:864` injects second-pool failure, observes unchanged bytes and the previously missing allowance. | PASS; FIND-TASK-003-5 closed |
| REQ-006/008, scenario 5: HTTP/CLI and real consumers | Existing Operator/platform and CLI evidence retained. New real-server identity journey reaches the missing human/issuer routes and CLI verbs, with actual authentication/session/binding observations; Gateway dispatch observes the credential after renewal. No SDK/MCP exposure added. | PASS; FIND-TASK-003-2 closed |
| Affected served Operator contract | `pg_openapi_contract.rs:2111` keeps exact-set equality for six operations, asserts bodyless renewal, typed redacted response and UUID locator. Prior failing five-operation expectation corrected. | PASS; FIND-TASK-003-3 closed |
| REQ-012 resource action/effect reference | `reference/stored-secret-encryption.md:117` and generated projection describe per-owner 404/409 behavior, disabled renewal, retained semantic fields and changed mutation metadata; new reference-preserving issuer replacement included. | PASS; FIND-TASK-003-4 closed |
| New issuer replacement error contract; REQ-008 and agent surfaces | `routes.rs:635` lists only the KMS/server code while store acquisition/read/CAS/commit map to the auth-store code at `:1206`; focused independent validation confirms the omission. | FAIL; FIND-TASK-003-6 |
| Scope and runtime performance | No migrations, dependencies, SDK/MCP/UI, session policy or runtime hot-path changes. New KMS preparation is outside DB transactions; shared audit stage adds no IO. | PASS |

## Finding and next bounded task

`findings-validation.md` retains FIND-TASK-003-6 once with its requirement, supported trigger, exact mapping, consequence and smallest correction. Original FIND-TASK-003-1 through -5 are closed and must not be reopened to implement this correction.

Remediation: `TASK-003-R2-correct-issuer-replacement-error-contract.md`. Add the actual store code beside the KMS code in the new PUT's response annotation and assert both against the served document. Keep runtime error mapping and every corrected lifecycle/security behavior unchanged. This is a fix-introduced surface-contract defect, not a failure of the five R1 corrections or an expanding product requirement. One remediation round has been reviewed; no two-round convergence assessment is triggered.

## Verification and limits

`verification.md` records exact setup-aware commands and completed fresh executions: **5/5 focused tests PASS**. The two library tests cover issuer replacement/bindings/corruption/races and the human audit persistence-failure branch (run `8f0f2834-bffa-494a-a9b4-6e25d31f197e`); the exact six-operation served contract passes (`4a13ee3b-b65f-4633-be49-43adb0aded0c`); Gateway dispatch after renewal passes (`49166cba-0a92-42fb-bf23-a4af8cae7234`); and real Keycloak/Dex human sessions and issuer CLI renewal/replacement pass (`9b26c16d-79fe-4565-85ab-77d4336bd4b6`). Managed migration setup, CLI binary/test build, provider lifecycle and final wrapper cleanup completed successfully. Full lint/codegen/docs/dependency checks remain implementation-reported, not new rerun claims. The validator checked actual assertions and consumers, not only test names. The new contract defect is statically established and independently confirmed; no live outage is needed to show its annotation differs from its mapper.

The root's final immutability check confirms HEAD remains `fa990be1989959de963d062ad8a47336b40c62da`, candidate production/tests/docs/architecture/config/spec/task equality is clean, and `git diff --check` passes. Only the preserved user instruction edits and new review directory differ in the workspace.

The Keycloak workload exchange does not submit the issuer's client secret. Its journey establishes binding continuity and actual served CLI behavior; the resolver owner test establishes the precise replacement secret. Controlled Transit fixtures are mocked HTTP endpoints with real Postgres, and prior real-provider facade proof is carried forward. No new live-provider acceptance claim, full aggregate or benchmark is made. Historical process records remain in the prior review and do not add an acceptance blocker.

Review changes only reports/remediation. It does not implement, commit, push, merge or deploy.
