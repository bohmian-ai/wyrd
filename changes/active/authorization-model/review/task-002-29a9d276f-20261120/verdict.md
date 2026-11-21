# TASK-002: FIX_REQUIRED

The UI implementation passes its recorded focused checks, but acceptance has two independently validated gaps: the created Alice grant is not consumed for the promised retained-review journey, and Access cannot invoke the existing account suspension operation. This review establishes no production filtering or credential leak.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `300542877cd2a58cbe5f061d5075616e4cc4397b`, accepted TASK-001 R2.
- Candidate: `29a9d276f5e668df56f5f16e898a73f7aefc46a0`.
- Actual range: `git diff 300542877...29a9d276f`; descendant range without rebase.
- Approved contract: SPEC-authorization-model revision 2; original `tasks/TASK-002-access-ui.md`.
- Supplied dirty spec/task/design context was preserved; hashes and navigation are in [subject.md](subject.md). Implementation source and HEAD remained unchanged.

Governing references: root and affected nested AGENTS, `changes/spec-driven-development.md`, `agent_docs/maintainer-style.md`, TESTING, UI skill, architecture authority map, security posture browser-session/authorization/access-administration/retained-data rules, and Bifrost Oracle/resource rules. The spec explicitly references the agreed administration design. Reports do not create additional product authority.

## Independent assignments

Fresh behavior-rev covered assigned requirements, UI workflows, assertions, documentation, projections and unassigned repository rules in [task-review-behavior.md](task-review-behavior.md). Fresh domain-rev covered BFF authority, credentials, tenancy, Card concealment and local/provider authentication in [task-review-domain.md](task-review-domain.md). Fresh system-rev covered Oracle parsing, CPU/deadline/lifetime, forwarding, parser pin, query equivalence, integration host lifecycle and changed Bifrost assertions in [task-review-system.md](task-review-system.md).

Fresh ponytail-rev independently checked all proposals and coverage in [findings-validation.md](findings-validation.md). Separate invariant/repository/maintainer reviewers were omitted because those boundaries were covered by the selected concrete assignments; no substantial uncovered owner abstraction required another specialist. The orchestrator assembled this verdict and did not discover or validate findings. No new validator-originated blocker required another validation pass.

## Reconciled acceptance matrix

| Existing requirement/scenario | Implementation and proof | Result |
| --- | --- | --- |
| REQ-002 / AC-001 local invitation/browser/CLI | Alice production journey 2680–2704, 2724–2768: saved binding, password setup and redeemed real CLI login; provider pass recorded at candidate | PASS |
| REQ-003 / AC-002 runtime family and releases | Service journey 2943–3065: one key/two pulled releases, retained attribution, wrong-family/ungranted refusal, rotation/revoke and reverse SDK presentation | PASS; unchanged backend exact-Verifier semantics carried from TASK-001 |
| REQ-006 / AC-001 connected retained reviews | Alice admits PR deliveries but checks Cards only; no retained review/member/evidence/count consumer of her UI-created grant | FAIL: FIND-TASK-002-1 |
| REQ-007 / AC-001 GitHub isolation | Alice GitHub/Access refusal plus existing real configuration/inventory/direct-operation assertions and shared server boundary | PASS |
| REQ-008 scoped forms/roles/groups/keys | Saved binding, missing/inapplicable/malformed scope refusal, built-in/custom roles, inherited sources and credential lifecycle assertions | PASS for these promises |
| REQ-008 account management | Status display without action/control for existing principal suspension owner | FAIL: FIND-TASK-002-2 |
| REQ-009 / AC-003 safe copy/current sessions | Clipboard/no-request/unknown-hidden parity, same browser/CLI/SDK gain and loss; TASK-001 backend freshness and ceilings carried forward | PASS |
| UI failure/presentation/docs/projections | Real denial/unavailable/recovery and keyboard/mode/viewport assertions, supplied owner checks, fresh units 14/14 | PASS within recorded limits |
| Oracle/parser/runtime invariants | Independent source traces, parse/lane/stack regressions, supplied runs and discovery result equivalence | PASS functionally; benchmark excluded by maintainer |

Implementation/assertion locations and execution provenance for each row are in the independent reports and validator matrix. Prior TASK-001 acceptance remains supporting evidence for unchanged enforcement, freshness, delegation, peers and language contracts. It cannot establish the missing consumer of this task's created output.

## Findings and handoff

The two retained findings are recorded once, with full requirement, trigger, source, consequence and correction evidence, in [findings-validation.md](findings-validation.md):

- **FIND-TASK-002-1 — EVIDENCE_GAP:** extend the existing Alice journey to actual payments reviews/member executions/evidence and filtered counts, using the created identity through required consumers. Production changes are not required absent a newly exposed defect.
- **FIND-TASK-002-2 — MISSING:** expose the existing Rust principal suspension/revocation operation through selected-account Access actions. This does not authorize a new resume API, storage lifecycle or unrelated profile editing.

One cohesive remediation packet is [TASK-002-R1-complete-access-management-and-connected-proof.md](TASK-002-R1-complete-access-management-and-connected-proof.md). There is no prior TASK-002 remediation round or convergence concern. Review returns this packet; it does not launch implementation or another review.

SYS-1, concerning two unchanged broad capacity assertions, was rejected as a TASK-002 blocker because the selected task gate uses focused journeys; broad integrated acceptance belongs to change review. The supplied server result remains **64/66**, and the other branch's correction is neither merged nor counted as passing proof.

## Verification and process history

Fresh focused access/upstream units: **14/14**. Reviewed-range `git diff --check`: exit 0. Supplied UI pages **19/19** and library **17/17** were at 6a11468a8; subsequent candidate changes preserve the UI implementation correspondence. Provider login is recorded passing at 29a9d276f; parser regressions **3/3**. Owner format/lint/codegen/docs/UI checks are supplied results, not all rerun here. Broad passes cannot establish assertions absent from the connected journey.

Added/moved hot-path waits: **PostgreSQL none; Scribe none; object storage none; Vault none**. New CPU-lane admission and parse completion share the captured deadline; the worker retains its lease if the waiter times out and releases it before IO. Existing sanctioned permission/catalog waits and nonblocking audit staging are unchanged. Full clean Oracle benchmark qualification and historical comparison were explicitly excluded by the maintainer; no measured-performance acceptance is claimed.

Behavioral verification: **FIX_REQUIRED** for the two validated findings. TDD compliance: original three RED records **unverified**, and acknowledged later tests written after fixes **violated** test-first discipline. No historical RED reconstruction or production churn is required to repair history.

Skipped broad reruns, screenshots, deployment and benchmark qualification. Shared UI output can still be disrupted by another rebuilding process; no supported-flow regression justified new build-lock machinery. Existing dirty packet edits were preserved; nothing was implemented, committed, merged, pushed or deployed.
