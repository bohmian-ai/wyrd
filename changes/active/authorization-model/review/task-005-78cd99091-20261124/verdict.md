# TASK-005 — FIX_REQUIRED

The principal projections and hand-merged provider helpers have credible implementation and connected evidence. All twelve maintainer-approved mock differences are accepted. TASK-005 cannot pass yet: destructive company-SSO controls lack confirmation, modal/scroll accessibility is incomplete, required Secrets and provider-identity UI journeys are missing, and the merge removed served-contract assertions. Required qualification also remains incomplete.

## Immutable subject and authority

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Base: `f7e0d67b2e007a1262fed34d45e5ebd2629e9bee`.
- Candidate: `78cd99091c81e5a7d706ac6f54a8222e62705e0a`; HEAD remained this commit and tracked source was unchanged.
- Actual reviewed range: `git diff f7e0d67b2...78cd99091`; includes merge `8cc9b03d3` of PR 244.
- Approved authority: authorization-model spec revision 12, REQ-021 / AC-016, revision-12 persistence rows and Scope and delivery; original `tasks/TASK-005-admin-console.md`; finalized admin-console design, mock and twelve recorded maintainer rulings.
- Governing references: root and applicable nested AGENTS; architecture authority map, Wyrd self-profile paragraph, security posture browser/platform/tenant/access/stored-secret boundaries; spec-driven development, maintainer style, TESTING and wyrd-ui rules.

This is the first TASK-005 review of this candidate. There is no prior finding-closure or remediation-convergence claim. The cumulative range's unrelated Bifrost/benchmark changes retain their separate acceptance/qualification scope; their effects on TASK-005 dependencies and merge seams were assessed.

## Independent coverage assignments

| Reviewer / report | Assignment |
|---|---|
| behavior-rev / `task-review-behavior.md` | Full requirement/scenario acceptance; tenant and platform consumer workflows, failure paths and assertions |
| domain-rev / `task-review-domain.md` | Sensitive principal/provider/tenant/SQL/SDK/MCP/CLI boundary and hand-merged platform fixture helpers |
| ui-rev / `task-review-ui.md` | Mock rulings, shared presentation, provider marks, responsive/mode/keyboard accessibility and captures |
| repo-rev / `task-review-repo.md` | Broad cross-tier/gate rules, migration authorization, merge provenance, hot-path applicability, adopted dev/test seams |
| ponytail-rev / `findings-validation.md` | Fresh independent coverage and source validation of every proposal; final reconciliation |

All selected reports are present. Separate invariant/system reviewers were omitted because TASK-005 adds no complex shared runtime state or recovery subsystem; the domain reviewer covers auth transaction/session failure boundaries, primary covers workflow refusals, and repo reviewer covers the paused-clock/dev cleanup seam. A separate maintainer reviewer would duplicate these bounded owners; primary retains general maintainability coverage. The UI specialist is needed for the approved visual contract and accessibility boundary. The orchestrator did not act as a reviewer.

## Reconciled acceptance matrix

All rows reference the existing TASK-005 scenario numbers and REQ-021 / AC-016; they do not create new obligations. UI paths are relative to `crates/wyrd/wyrd-server/wyrd-ui`.

| Scenario / promise | Implementation and actual proof | Result |
|---|---|---|
| 1. Self-profile authority first | `architecture/wyrd-design.md:1040`, spec profile rustdoc; first-parent history places approved reconciliation before implementation | PASS |
| 2. Store/refresh provider names and consume them | Claims/GitHub parsing → auth callbacks → SQL refresh; SQL nullable/refresh/RLS and real provider callback tests cover production. Rendered provider identity chain is unproved; task's cited Alice fixture is password-based | FAIL — FIND-TASK-005-5 |
| 3. Safe read projections across surfaces | `wyrd-spec` types → SQL directory/shared role holders → principal handlers → shared client / Python / TS / MCP / CLI. SQL/server/client assertions cover scopes, filters, counts, descriptions, null human methods for Services and tenant exclusion. Served OpenAPI assertions disappeared during merge | FAIL — FIND-TASK-005-6; other inspected projection behavior PASS |
| 4. Settings/account identity | Shared Shell, admin section list, account route/load; password account and ordinary-User navigation assertions pass per receipts. Provider-name priority/refresh/email in rendered consumers lacks connected proof | FAIL — FIND-TASK-005-5 |
| 5. People, Service accounts, Roles | Existing access actions and actual invitation/grant/group/suspension/key/runtime-service/Role/refusal journeys; server counts and Role filter consumed. Shared modal keyboard containment is missing | FAIL — FIND-TASK-005-2; traced owner workflows PASS |
| 6. Company SSO/policy | Sign-in actions delegate to existing identity owner; production required-policy tests cover fresh proof, refusal, revisions and replicas. Deactivate/Remove submit before confirmation | FAIL — FIND-TASK-005-1 and shared modal FIND-TASK-005-2 |
| 7. Secrets and Integrations | Four-owner Secrets composition and action dispatch exist; mocked loader plus captures do not prove mutations/refusals/cancel. Actual GitHub Integrations journey remains connected | FAIL — FIND-TASK-005-4; scrolling FIND-TASK-005-3 |
| 8. Platform shared shell/authority | Production console navigation, mode, plane separation, fresh-proof workspace/provider/mail/signup workflows; hand-merged signing/held-token helpers preserve verified claims and real callback assertions. New dense tables miss required region accessibility | FAIL — FIND-TASK-005-3; traced platform authority/helper boundary PASS |
| 9. Provider marks | Production Mark SVG matches approved artwork/color behavior; brand exception and existing POST wiring retained; real Google Join/Continue journey. Historical capture coverage of every auth-page variant is limited | PASS for inspected artwork/wiring/interaction; visual qualification limit recorded below |
| 10. Mock states/modes/keyboard | Twelve explicit deviations accepted; capture helper covers named admin/account/platform states at 1280/390 and both modes; real People/Services responsive tests. Shared modals and four other table regions violate required keyboard behavior | FAIL — FIND-TASK-005-2, FIND-TASK-005-3 |
| Mandatory qualification | Post-merge runtime/codegen/UI/docs receipts inspected against assertions; review freshly passed format, Python checks, TS format/lint. Recorded aggregate remains incomplete and Clippy/TS type/N-API receipts absent | FAIL — FIND-TASK-005-7 |

Stable findings, full source evidence, starting conditions, consequences and smallest corrections are recorded once in [findings-validation.md](findings-validation.md). Remediation is [TASK-005-R1-complete-console-safety-and-proof.md](TASK-005-R1-complete-console-safety-and-proof.md), covering FIND-TASK-005-1 through FIND-TASK-005-7 under the unchanged approved behavior.

## Verification and limits

The repository reviewer freshly ran `fmt:check`, `py:format:check`, `py:lints`, `py:typecheck`, `ts:format:check`, `ts:lints`, and working-tree `git diff --check`; all passed. No production formatter was run in write mode. Its first batched mise invocations failed on task-argument setup and were corrected with separate invocations; no source failure is inferred from those attempts.

Post-merge codegen, OpenAPI/platform, principals, identity UI, UI, docs and development-lifecycle passes are task/maintainer receipts, not fresh independent browser/database executions. Their actual assertions were inspected, including the missing OpenAPI test despite a green lane receipt. The recorded `test:wyrd` result is 3032/3034; the capacity-clock repair is source-consistent, but no clean final aggregate is supplied and the verification-route 504 remains open. This review does not attribute it to the redesign. Outstanding qualification is FIND-TASK-005-7, not a diagnosed production defect.

Historical mock/production captures were inspected by the UI reviewer and corroborate structure. They lack an immutable candidate manifest and do not establish every sign-in/signup/invitation variant; this is not a fresh comprehensive visual-parity certification. All twelve recorded differences remain approved. Focused visual checks must accompany repaired overlays and table wrappers.

TASK-005 adds/moves **no wait on a defined hot path**. Admin directory/Role/profile reads and provider callback updates are authentication/administration work; no verification/direct-verify/gateway/Oracle/Scribe wait was added by this task. No TASK-005 benchmark is required or claimed. Separate merged Bifrost performance acceptance is not granted here. TASK-004's paused live-provider smoke remains excluded.

Behavioral verification: **FIX_REQUIRED**. TDD compliance: **NOT_APPLICABLE**, explicitly waived for all TASK-005 entries by approved revision 12. No historical RED repair is required.

Only reports and remediation were written. No implementation, merge, commit, push, deploy, broad suite rerun or benchmark was performed. The missing connected proof and open qualification remain acceptance risks until closed.
