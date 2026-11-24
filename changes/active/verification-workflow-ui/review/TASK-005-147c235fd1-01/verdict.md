# TASK-005 — FIX_REQUIRED

Behavioral verification: **FIX_REQUIRED**. Independent validation retained three
blocking findings. Historical TDD compliance: **VIOLATED**, separately; this
verdict neither erases that history nor demands retroactive RED evidence.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verification-workflow-ui-resume`.
- Base: `05fc05138a0a72d59a2fd9b96fae5e6e4ad6f285`.
- Candidate: `147c235fd19dd4a400da1515a908db0d5521477d`.
- Actual comparison: `git diff 05fc05138a...147c235fd1`; 12 commits, 41 paths.
- Approved contract: verification-workflow-ui revision 8,
  `changes/active/verification-workflow-ui/spec.md`, blob
  `e736e3463f8862b7ae0a34dcca9274e23c00dcfa`.
- Supporting contract: authorization-model revision 9, blob
  `55689bae96c62e78b1ea8d765a32629664b4e603`.
- Original task: `changes/active/verification-workflow-ui/tasks/TASK-005-pr-review-sets.md`,
  blob `9b0165d8e5de02ff2a272b6c70acc1bf81916e43`.

Governing references: root and applicable SQL/Python/TypeScript AGENTS;
`changes/spec-driven-development.md`; `agent_docs/index.md` and applicable Rust,
maintainer-style and agent-surface guidance; `TESTING.md`;
`architecture/wyrd-design.md` Repository, Verifier, client/recording and registry
owners; `architecture/wyrd-security-posture.md` current authority and retained-data
visibility; `wyrd-ui` skill and its architecture/testing references. No CodeGraph
index exists in this checkout.

Current maintainer decisions accept the listed AC-009 mock differences and the
bounded display-field/shared-mark changes. They are not reopened. The task's
`proposed` header is not treated as independent runtime acceptance; its approved
specification and explicit activation remain the behavioral authority.

## Independent review coverage

The orchestrator coordinated source identities and executable checks; it did not
serve as a discovery reviewer or validate its own findings.

| Reviewer | Concrete boundary and report |
|---|---|
| `behavior_rev` (behavior-rev) | Assigned requirements/scenarios, navigation/pins/history, shared marks, assertions, accepted visual scope and remaining local rules: [primary report](task-review-behavior.md). |
| `domain_rev` (domain-rev) | Retained-parent authorization and version visibility, query bounds/member completeness, definition-unavailable/deleted-Card reads and unchanged producer/consumer dependencies: [domain report](task-review-domain.md). |
| `repo_rev` (repo-rev) | GitHub display-field admission identity, retained propagation, Rust/wire rules and Python/TS projections: [repository report](task-review-repo.md). |
| `ponytail_rev` (fresh validation) | Independent coverage/proof assessment, specialist applicability, every proposed finding, focused reproductions and remediation scope: [validation](findings-validation.md). |

All selected reports were delivered. No separate system reviewer was needed:
there is no new recovery topology or failure propagation owner. No separate
maintainer reviewer was needed: there is no substantial owner replacement;
primary covered local maintainability. State/query lifecycle expertise belongs
to the domain reviewer and pin/navigation lifecycle to primary, so a separate
invariant reviewer would duplicate those assignments. The validator independently
confirmed this coverage. No selected expertise is missing.

## Reconciled acceptance matrix

The following assessments are supplied by the independent validator. They apply
only to TASK-005's assigned portions of each referenced requirement, preserving
the original task's exclusions and predecessor ownership.

| Assigned references | Implementation and proof | Result |
|---|---|---|
| REQ-001/003; scenarios 1/3/5; AC-001 | Shared execution population, Verify facets/groups, Repository Head/Policy; component/page assertions and fresh real-server grants/facets proof. Assigned PR contribution only; predecessor discovery retained. | PASS |
| REQ-002/012; scenarios 1/2/5; AC-007 | PR URL/generation selection, current/history/member navigation; missing pin and changed-PR assertions; real admissions across rerequest/new head, fresh focused library run; recorded Alice reload/Back proof. | PASS |
| REQ-011; scenarios 1/3/5 | Existing Repository routing/activity links and exact policy projection; page assertions and scoped real-server/browser evidence. | PASS |
| REQ-020 consumer obligations; scenarios 1/2/3 | Retained frozen membership, observation ordering, bounded explicit history/pins, partial-member reporting; pure bounds/refusal tests, fresh real-server membership/grants, unchanged prerequisite interrupted/delayed-record proof. | PASS |
| REQ-004/005; scenario 4; AC-002 | Existing fallback reads authorized progress, but repeated result requirements collide and preterminal requirement progress is discarded; independent executable falsification and producer-to-consumer source traces. FIND-TASK-005-1/-2. | FAIL |
| REQ-006; scenario 4 | Existing exact-producer publication reads, bounded preview and production BFF download; fresh subject-only real-server preview/download test. No separate publication-owner defect found. | PASS |
| REQ-014; scenarios 1/4/5 | Fresh Node/MCP/built-artifact/custom-host proof passes; shared marks pass component assertions. Shared fallback fails supported variants and exported-renderer contract gate fails. FIND-TASK-005-1/-2/-3. | FAIL |
| AC-008; scenarios 3/4/5 | Family/custom/exact/table-only visibility, grant changes and hidden/unknown paths are supported by source/assertions/runtime evidence. No-definition inspection remains incomplete under FIND-TASK-005-1/-2. | FAIL |
| AC-009; scenario 5 | Maintainer accepted recorded mock differences; recorded browser/theme/width/navigation evidence plus fresh shared-mark/component checks remain applicable. No additional visual ruling required. | PASS |
| Mandatory generated/component-contract gate | Fresh full codegen check and independent focused brand test fail on uncontracted StatusMark. FIND-TASK-005-3. | FAIL |

## Validated blockers and remediation

Each finding, its authority, exact location, supported trigger, evidence,
consequence and smallest correction is retained once in
[findings-validation.md](findings-validation.md):

- **FIND-TASK-005-1 — INCORRECT:** repeated requirement IDs collide in
  definition-unavailable navigation, preventing reliable rendering.
- **FIND-TASK-005-2 — MISSING:** settled requirement invocations disappear
  before a final result when the definition is unavailable; exact requirement
  selection falsely reports “Not judged yet.”
- **FIND-TASK-005-3 — VIOLATION:** exported StatusMark lacks its component
  contract, and the mandatory codegen gate fails.

Primary B1 and DOMAIN-1 were consolidated into FIND-TASK-005-1. DOMAIN-2 and
primary B2 became findings 2 and 3 respectively. No new validator finding needed
another discovery pass; no rejected proposal creates remediation work.

One cohesive correction task:
[TASK-005-R1](TASK-005-R1-recorded-inspection-and-component-contract.md).
Its scope is the existing shared inspection component and brand-contract owner;
it introduces no server workflow, API, SDK field or new authorization behavior.
The validator checked its correction and closure scope against the findings.

## Verification and limits

Current-candidate executions are recorded in [verification.md](verification.md):

- `mise run ui:check`: PASS, 113 tests; package and host each report zero
  Svelte errors/warnings; package/MCP artifact rebuilt.
- `mise run py:typecheck`: PASS.
- Focused real-server library selector
  `TASK-005|a reader of the subject but not the Verifier`: PASS, five selected
  tests, 17 unselected; real Postgres/server with fresh addon/testing/server
  prerequisites.
- `mise run codegen:check`: FAIL on the missing StatusMark contract.
- Independent isolated render assertions reproduced findings 1 and 2;
  independent focused brand check reproduced finding 3. These are diagnostic
  falsifications, not durable regression tests or successful product proof.
- Commit-range whitespace check: PASS.

Unchanged Rust/provider/SDK evidence carries forward only where source and
dependencies remain unchanged; the repository reviewer traced that relationship.
Recorded production-browser and visual evidence remains applicable to its actual
assertions, with the maintainer's AC-009 rulings and current shared-mark tests.
No full library integration suite, broad shipped-surface journey aggregate,
benchmark or new browser/visual capture was run during this review. Those limits
do not replace the focused proof or waive the validated blockers. Historical
TDD violations remain recorded, without prescribing impossible historical repair.

This is the first review of this TASK-005 candidate: no prior remediation closure
or repeated-round convergence assessment applies. HEAD and tracked source stayed
at the candidate; only this new review directory was written. No implementation,
commit, push, merge or deployment was performed. TASK-005 has not passed its
completion gate; return this verdict and R1 to the maintainer.
