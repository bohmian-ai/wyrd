# TASK-002 independent findings validation

Recommendation: **FIX_REQUIRED**. Two source-backed blockers remain: one connected-journey evidence gap and the missing supported account-suspension action. No production filtering or credential leak is established. This is fresh validation of discovery proposals, not implementation.

## Subject, authority and coverage

Base `300542877cd2a58cbe5f061d5075616e4cc4397b`; candidate `29a9d276f5e668df56f5f16e898a73f7aefc46a0`; approved SPEC-authorization-model revision 2. The subject record identifies the supplied dirty packet context and hashes. HEAD, spec hash and task hash still match that record at validation. Read the task-review skill, supplied root rules, approved requirements/acceptance, cited agreed administration design, and all three discovery reports. CodeGraph preceded exact source tracing; its route/test queries returned unrelated indexed symbols, so direct current source completed those traces.

The behavior reviewer covers assigned promises, UI consumers and repository rules; domain covers the sensitive BFF/credential/tenancy/Card-concealment seam; system covers CPU/deadline/forwarding/parser/host lifecycle and changed server assertions. These concrete assignments cover the material boundaries. Separate invariant, repository and maintainer reviewers would duplicate the selected scopes; none is needed for another vote. Prior TASK-001 evidence remains applicable to unchanged backend enforcement, freshness, delegation, peer/source and language boundaries, but cannot prove an unexercised consumer of a newly created UI grant.

## Proposal dispositions

| Proposal | Disposition | Reason |
| --- | --- | --- |
| Behavior B-1 | **CONFIRMED**, FIND-TASK-002-1 | The complete Alice test admits PR deliveries but never consumes their retained review sets/member executions/evidence or filtered counts. Its downstream CLI/SDK checks list/get Cards only. Separate backend journeys and the older admin/tenant-wide UI journey do not consume this UI-created binding. |
| Behavior B-2 | **REVISED**, FIND-TASK-002-2 | Retain only the missing existing principal-suspension operation. REQ-008's account management is made concrete by the spec's explicitly referenced, maintainer-agreed administration design. Do not infer arbitrary profile editing or a new reversible resume API. |
| System SYS-1 | **REJECTED as a TASK-002 blocker** | The two failing capacity assertions are unchanged in this range and outside TASK-002's required focused UI and affected-boundary checks. The skill explicitly reserves broad integrated suites for change review. Recording their failure is not circumventing a task gate. They remain a real final-integration limit; a future merge is not passing evidence. No remediation for them is created by this task review. |

Domain discovery proposed no blocker. Its selected-session forwarding, tenant/Rust authorization, one-time no-store issuance, SQL projection and shared HTTP/MCP Card-error traces substantiate that assessment. System discovery found no concrete production defect; independently inspected lane dispatch, parse and forwarding source agree with the reported admission/deadline/lifetime behavior. There is no new validator-originated blocker requiring another discovery pass.

## Retained findings

### FIND-TASK-002-1 — EVIDENCE_GAP: UI-created access never proves the connected retained-review promise

**Requirement:** REQ-006 and AC-001; original TASK-002 cycle 1. The approved observation connects creation of Alice and payments access in the UI to the same Repository/review membership across UI, HTTP/CLI, SDK/MCP and SQL, with another Repository contributing no data/counts. The task explicitly calls for payments review sets, member executions and evidence through those created grants. Root journey rules and the review skill require real consumer outputs and assertions at that seam.

**Exact location:** `crates/wyrd/wyrd-server/wyrd-ui/src/lib/ui-pages.integration.test.ts:2650`, setup at 2674–2677, browser assertions at 2729–2743, downstream CLI/SDK at 2760–2768, and gain/removal at 2794–2813.

**Starting conditions and trigger:** the administrator invites local Alice with viewer at Repository/payments using Access; payments and ledger PR deliveries are admitted. Alice signs in and approves her real CLI code, then reads payments reviews and evidence using this identity while ledger remains ungranted.

**Expected proof:** visible payments review/member/evidence results and filtered list/page/count results actually belong to payments; ledger contributes none. A referenced definition outside the grant remains refused independently. The shipped downstream consumers use the created account/grants or its redeemed credentials.

**Actual proof:** the browser reads the Repositories index and visits ledger's Card page. CLI lists Repository Cards; the TypeScript SDK lists/gets Cards. Later grant/removal checks also exercise Card access. The PR setup asserts only delivery HTTP 202, not materialized retained output. No payments review view or retained query is consumed. The earlier UI review journey at 2391 onward signs in a separate administrator and separately bootstrapped tenant-wide viewer. TASK-001 language/MCP evidence uses other fixture accounts/grants. Therefore passing 19/19 does not prove this explicitly required seam.

The supplied task honestly records the Python/MCP limitation and cites TASK-001. That is usable backend proof and remains preserved; the current user message reports the limitation, rather than explicitly revising AC-001 or waiving its connected-output requirement. The missing retained-review assertions are broader than that language limitation and independently sufficient for this finding.

**Consequence:** required connected behavior remains unproved; no filtering implementation defect is alleged.

**Smallest testable correction:** extend the existing Alice production journey/PR fixtures to await and consume real retained payments reviews, members and evidence, assert ledger exclusion before pagination/counting, and check referenced-definition refusal. Reuse existing public downstream consumers with this created identity/credential for the required surface seam; do not duplicate unchanged backend fault-injection or invent authorization machinery. Existing TASK-001 tests remain supporting proof. Run the focused Alice selector in `src/lib/ui-pages.integration.test.ts` under the existing production BFF/repository-Postgres runner, plus only focused existing consumer selectors needed to exercise this output. New selectors are implementation-owned. No production change is required unless the strengthened proof finds a defect.

### FIND-TASK-002-2 — MISSING: Access cannot suspend an existing managed account

**Requirement:** REQ-008, “Tenant admins create/manage accounts and scoped assignments,” and its explicitly referenced agreed design, `design/notes.md:410–414`: Users suspend and Service accounts suspend through Access using existing owners. Security posture Access administration assigns that UI the existing Rust-owned account/credential workflows. This is an existing supported stop-account operation, not a generic inference that every backend method needs a new control.

**Exact locations:** Access `src/routes/t/[tenantKey]/settings/access/+page.server.ts:108–189` (complete action table), and `+page.svelte:163–175` (selected account status). The status is displayed but no account-suspension control or BFF action exists. Remaining actions manage invitation/create, bindings/roles/groups and individual Service keys.

**Starting conditions and trigger:** an administrator selects an active User or Service in Access and needs to stop that account's credential use while keeping its identity and grants, rather than removing grants or individually rotating keys.

**Expected behavior:** the UI invokes the existing Rust principal-revocation/suspension contract and presents its actual success/refusal and resulting state. Account suspension is distinct from removing one binding or retiring one credential.

**Actual behavior/evidence:** there is no route action to invoke it. Existing `crates/shared/wyrd-client/src/principals/handle.rs:286` exposes `revoke_principal`; `wyrd-server/src/auth/revoke.rs:75` implements `POST /v1/principals/{id}/revoke`, validates the typed principal kind/reason, authorizes/audits, and invokes the existing durable owner in a tenant transaction. The account UI cannot drive that supported operation.

**Consequence:** an administrator must leave the approved Access workflow to stop an account. This is missing UI behavior within settled account management, rather than missing backend enforcement.

**Smallest testable correction:** add the selected-account action/control for the existing principal revoke contract, forwarding kind/reason and honoring server failures. Reuse the current BFF session/action/problem owner and existing Rust lifecycle exactly, preserving account identity/grants, tenant isolation, current authorization propagation and existing cancellation/credential behavior. Use its established user-facing suspension/revocation semantics; do not invent reversible suspension, resume, new status/storage, or unrelated account-field editing. No migration or architecture revision is indicated by this bounded correction. If an implementation attempts different lifecycle semantics, that is a separate contract decision, not authorized by this finding.

**Closure:** extend the existing focused Access production journey with an active User and Service; suspend each through the UI, inspect the server's resulting account state and show subsequent credential use refused through the real existing consumer. Assert a non-admin direct BFF mutation cannot perform it, and a refused mutation reports the Rust problem and preserves state. Use the same `ui-pages.integration.test.ts` focused runner below; component-only assertions cannot establish suspension. Preserve last-admin protections already proved at the owner. New test selectors are implementation-owned.

## Reconciled acceptance matrix

| Existing requirement/scenario | Implementation and strongest proof | Result |
| --- | --- | --- |
| REQ-002 / AC-001 local invitation, local browser and real CLI approval | Alice 2680–2704 and 2724–2768, saved explicit binding and redeemed CLI configuration; existing Rust local/provider owners preserved; supplied provider pass at candidate | PASS |
| REQ-003 / AC-002 runtime family and one credential over releases | Service UI journey 2943–3065: separate restriction/bindings, two real pulled states, retained workload/subject/principal/actor rows, wrong-family and authoring refusal, key rotation/revoke and reverse SDK presentation | PASS; unchanged exact-Verifier/backend semantics retain TASK-001 proof |
| REQ-006 / AC-001 connected retained review data and counts | Alice PR admission plus Card-only assertions; prior backend proof does not use this created grant | FAIL — FIND-TASK-002-1 |
| REQ-007 / AC-001 configuration isolation | Alice denied GitHub/Access; existing real GitHub journey checks configuration/inventory/direct operations, preserved server boundary | PASS |
| REQ-008 / scoped grants, source, role/group and key administration | Saved-binding assertions, explicit/malformed/inapplicable scopes, built-ins, custom roles, inherited group preservation, key one-time/rotation/revoke | PASS for these subclaims |
| REQ-008 / existing managed-account suspension | Status presentation only; no BFF action/control for existing revoke owner | FAIL — FIND-TASK-002-2 |
| REQ-009 / AC-003 copy and current session | Clipboard exact contents/no outbound request, hidden/unknown parity, same-session browser/CLI/SDK grant/removal; backend freshness/ceilings retain TASK-001 coverage | PASS |
| UI failure/presentation and docs/projections | Recorded real denial/unavailability/recovery and keyboard/mode/viewport checks; source/projection/doc review and supplied owner checks; fresh access/upstream units 14/14 | PASS within inspected assertions/execution limits |
| Oracle/parser/runtime and shared-owner invariants | Independent system traces, parse/lane/stack regression assertions and supplied runs, query equivalence; no added external wait | PASS functionally; benchmark explicitly excluded |

## Execution, waits and limits

No tests were rerun by this validator. The behavior review freshly ran `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/access.test.ts src/lib/server/upstream.test.ts`: 14/14. Recorded production UI pages 19/19 and library 17/17 were at 6a11468a8; later candidate changes are Bifrost fixture work, preserving UI implementation correspondence. Provider result is recorded at 29a9d276f. These supplied runs and inspected assertions support the covered rows; neither test names nor aggregate pass counts repair FIND-TASK-002-1.

Existing focused production runner: `mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-pages.integration.test.ts -t '<focused selector>'`, using the task's existing TS/testing/UI production-build prerequisites. Re-run affected owner checks (`ui:check`; codegen only if contract sources change) as required. Full integrated suites belong to change review.

The two isolated-capacity assertions remain red in this immutable candidate: the supplied broad server result is 64/66. Their existing correction on 5290a7c14 has not merged and supplies no candidate execution proof. Rejecting SYS-1 as a task-scope blocker does not make that integrated result green or permit final acceptance to skip a mandatory integrated gate.

System wait inventory carries forward: **no added/moved PostgreSQL, Scribe, object-storage or Vault wait**. New parse CPU-lane admission/completion is bounded by the existing captured deadline; its worker owns the lease until completion even if the waiter times out, and returns it before catalog/storage IO. Warm permission/row checks and outbox staging are unchanged. The explicit current maintainer decision excludes full clean Oracle benchmark qualification; no benchmark, historical comparison, or measured-performance PASS is claimed.

Original UI-cycle RED history remains **unverified**. Later tests written after production fixes are recorded **TDD violations**; behavioral acceptance cannot erase them. Do not require reconstructed historical RED or production churn. Task frontmatter mismatch is administrative and is not retained as a behavioral finding. The disclosed shared build-directory risk from an unrelated concurrent process has no demonstrated newly broken supported flow and creates no locking/rebuild redesign requirement.

Skipped broad reruns, screenshots, deployment and benchmark qualification. Production source and packet edits were preserved; the two retained findings bound the required remediation.
