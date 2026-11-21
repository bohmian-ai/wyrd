# TASK-002 R1 remediation review

Reviewed immutable remediation range `29a9d276f5e668df56f5f16e898a73f7aefc46a0..ff42598100b0edabad5cad2272a3c7735f2eef14`; cumulative base `300542877cd2a58cbe5f061d5075616e4cc4397b`. Approved working-tree SPEC-authorization-model revision 2 and original TASK-002 have the supplied SHA256 identities. Intentional maintainer packet edits are context, not candidate changes. HEAD remained unchanged.

This fresh remediation reviewer checked prior findings and affected UI/consumer paths. Oracle correctness and principal lifecycle/SQL security are separately assigned specialist boundaries; this report does not substitute for them. A separate validator must assess any retained blocker.

## Authority and navigation

Read the task-review and UI skills, root instructions, spec-driven-development, maintainer style, TESTING, relevant security posture retained-data/access-administration rules, original task, prior validated findings/verdict, and R1 task/evidence. The governing promises are REQ-006 (`spec.md:140–164`), REQ-008, AC-001 and TASK-002 cycle 1. Prior backend enforcement proof remains usable but does not replace connected consumers of the UI-created grant.

CodeGraph was consulted before source navigation. It returned the Access action owner; exact source/diff completed test navigation not returned by that query. Compact map:

- Access selected-account form (`settings/access/+page.svelte:180–192`) → BFF `suspend` (`+page.server.ts:183–190`) → existing session `call`/`act` (`:12–15`, `:54–64`) → existing Rust `POST /v1/principals/{id}/revoke` owner. The form sends principal id/kind/reason; it adds no browser credential or alternative authority.
- Existing load (`+page.server.ts:27–48`) reloads server account/access/key metadata after enhanced action completion. Suspended Service keys remain visible, while issuance is hidden (`+page.svelte:222–258`). The server specialist assesses the broader status-independent lookup needed by that load.
- Alice journey (`ui-pages.integration.test.ts:2651`) creates the grant in Access, accepts the invitation, signs in, redeems device login, and passes that actual saved login to TS, CLI, Python and MCP consumers (`:2724–2826`). It then opens Repository/Verify/member views as Alice (`:2828–2853`).
- New suspension journey (`:3028–3127`) drives the production BFF and real server for User and Service suspension, refusal and preserved state.

## Prior finding closure

| Existing reference | Source and supplied proof | Assessment |
| --- | --- | --- |
| FIND-TASK-002-1 / REQ-006, AC-001: connected grant and review membership | Alice's real CLI login is reused by TS (`:2797`), CLI (`:2804`), Python (`:2814`) and MCP (`:2820`). Admin reads two actual retained PR input rows, Alice reads exactly payments, its member/generation/head, count 1 and an empty offset page (`:2771–2825`). Browser opens payments' exact member, Verify lists payments only, hidden ledger member is indistinguishable from an invented id (`:2828–2853`). | Substantial closure: connected retained admission/review-set/member and input-data seam now proved. |
| FIND-TASK-002-1: independently refused referenced definition | Admin reads `Verifier/code-review@1.0.0`; Alice's member page, direct Card page and TS SDK refuse the definition (`:2835–2841`). | PASS. |
| FIND-TASK-002-1: connected review results and produced retained evidence | Every new SQL query targets `execution_inputs`; the expected execution explicitly remains `pending` (`:2793`, `:2834`). Implementation evidence expressly records no Verifier runs and empty results/evidence/events. No produced review-result or evidence value is read through Alice's created grant. | Remaining EVIDENCE_GAP proposed below; existing id retained. |
| FIND-TASK-002-2 / REQ-008: suspend User and Service from Access | Active-account form and existing BFF mutation added; no alternate persistence or lifecycle. Real journey asserts Service suspended, metadata/keys/bindings preserved, issuance control absent and subsequent key exchange refused; User suspended and existing browser redirected (`:3095–3121`). | PASS within supplied execution record and specialist confirmation. |
| FIND-TASK-002-2: refusal and presentation | Non-admin direct action returns failure 403 `WYRD_PERMISSION_403_DENIED_RBAC`, state unchanged (`:3058–3068`); unsafe reason displays safe `WYRD_SPEC_400_VALIDATION`, state unchanged (`:3070–3080`). Keyboard/focus, 390px dark overflow and desktop light submission checked (`:3081–3104`). | PASS. |

Unchanged prior acceptance rows for local login/device flow, runtime-family provisioning, scope/group/custom-role forms, GitHub separation, copy/no-send behavior, current-session gain/loss and docs/projections retain their prior evidence. The remediation does not reopen those implementations. No source-level UI authorization bypass, new workflow owner, migration, dependency or generated contract is added by this diff.

## Proposed retained finding for independent validation

**FIND-TASK-002-1 — EVIDENCE_GAP (partially closed).** Requirement: REQ-006 first sentence (`spec.md:142–143`) includes code-review results and retained evidence as well as review sets/members; AC-001 and original TASK-002 cycle 1 connect Alice's UI-created access to those actual retained outputs. R1 expressly asks to await materialized review outputs and read evidence, not merely delivery acceptance.

**Location:** `ui-pages.integration.test.ts:2771–2834`, especially `REVIEWS`/`MEMBERS` at `:2772–2773`, the explicit `pending` member at `:2793`, and the browser pending assertion at `:2834`; R1 acceptance/evidence acknowledges empty results/evidence/events.

**Starting state/trigger:** payments and ledger PR work is admitted, Access creates Alice with payments family viewer, and Alice signs in and redeems her CLI credential. She reads the promised Repository review results and evidence while ledger remains outside her grant.

**Expected proof:** Alice can consume at least one actual payments review result and its produced retained evidence under this grant, while equivalent ledger outputs contribute no data/counts/page or direct disclosure. Existing member/input/definition assertions remain useful and need not be duplicated.

**Actual proof:** the deployment never executes a Verifier. It materializes and reads two admission/input records, of which Alice sees one, and opens a pending execution. Inputs are genuine retained admission data; they do not exercise the result/evidence producers or disclosure of a completed review. An empty result/evidence store cannot prove positive access or exclusion for populated result/evidence rows. No runtime implementation defect is alleged.

**Consequence:** a required connected consumer seam for code-review results and produced evidence remains unproved. Separate TASK-001 accounts/fixtures establish backend behavior but cannot establish that this created grant reaches those outputs; that same distinction was the validated original finding.

**Smallest correction boundary:** extend the same Alice/PR journey and existing deployment/Verifier fixture setup just enough to produce real payments and ledger review results/evidence through existing owners, then inspect their actual values as Alice and assert ledger exclusion. Reuse the already connected identity/login and public consumers; preserve all passing admission/member/definition/suspension checks. Do not add production authorization machinery, broad suites, peer fault injection or backend refactoring absent an exposed defect. A focused production journey run under the existing R1 runner is the relevant proof. This is continuation of the original evidence correction, not a new product obligation or request to rewrite TDD history.

## Verification, performance and process history

Supplied R1 evidence records S1/S2 passing under the production BFF/repository-Postgres runner after `ts:build:testing`, and affected Rust/UI owner checks passing. Assertions above were inspected; this reviewer did not regenerate those runtime logs. Reviewed-range `git diff --check 29a9d276f ff4259810` passes. No Cargo or heavyweight journey was launched, honoring shared-build coordination.

Access adds only the existing administration call through the established BFF path, outside the defined hot paths. The Oracle specialist owns the exact query-path wait inventory and cast/source/peer assessment. Benchmark qualification was expressly excluded in the supplied prior review subject, R1 non-goals and approved working-tree spec; no benchmark or measured-performance PASS is claimed here. The prior 64/66 broad result remains a change-integration limitation, not converted to green by focused proof.

TDD is separate: original UI RED history remains unverified; S1 was written after exploratory probes, so its acknowledged test-first violation remains. Supplied O1 RED precedes the production Oracle fix; supplied S2 and K1 RED precede their behavior fixes. This reviewer did not reconstruct historical RED or independently witness it.

Skipped runtime reruns, screenshot comparisons, benchmarks, deployment and source edits. Residual confidence limits are the proposed connected result/evidence proof gap and the separately assigned specialist assessments.
