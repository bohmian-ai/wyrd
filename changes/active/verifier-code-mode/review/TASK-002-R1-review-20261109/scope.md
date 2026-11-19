# TASK-002 first remediation review scope

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode.
Original base: e5af2ce5caef2d00303e1c980b12c8a9bca4af0e.
Prior candidate: ff78b51b61fea8f215a2139b17840044f65b17e4.
New cumulative candidate: bcf61769e9c7b9b6632ed93521a695ff72dc36f4.
Remediation diff: git diff ff78b51b61fea8f215a2139b17840044f65b17e4 bcf61769e9c7b9b6632ed93521a695ff72dc36f4.
Full context: git diff e5af2ce5caef2d00303e1c980b12c8a9bca4af0e...bcf61769e9c7b9b6632ed93521a695ff72dc36f4.
Ancestry is linear; no rebase or patch-correspondence question.
Approved spec revision 2: changes/active/verifier-code-mode/spec.md at prior/new candidate (identical). Working revision 3 is unrelated and excluded.
Original task: changes/active/verifier-code-mode/tasks/TASK-002-public-code-mode-workflow.md at original base; original obligations unchanged.
Remediation task: ../TASK-002-review-20261109/TASK-002-R1-correct-output-visibility.md at new candidate, including supplied execution evidence.
Prior verdict/validated ledger: ../TASK-002-review-20261109/{verdict,findings-validation}.md. Those reports remain locally available, uncommitted and unchanged by this review; source subject is pinned in Git.

## Scope and carried evidence

Review FIND-TASK-002-1 closure, the entire remediation diff, and directly affected source/reference/generated/fixture consumers. The diff changes only the agent-tools reference, generated llms-full.txt, a shared program comment and the remediation packet/evidence. No runtime, SDK, schema, manifest, dependency or authority changes entered. No .codegraph directory exists. Preserve all existing local changes and prior reports. Reports only in this new directory.

Carry forward source-backed acceptance rows from prior verdict and four discovery reports: same public workflows/catalog, permission denial/recovery, recipient/default/custom-data preservation through immediate/queued/retained paths, original pinned citations, served/schema/stub/generated contract parity, other operator guidance and non-goals/repository rules. Prior task-review-invariants.md traces full producer-to-consumer defaults/conversions/storage paths; domain-review-security.md traces accepted caller/permissions/record visibility; standards-review.md maps every original diff surface to applicable authority; task-review-behavior.md covers all original task criteria and fixture reuse. Prior findings-validation.md independently validated that coverage. Inspect whether the remediation or changed authority invalidates any row. Do not repeat full discovery or TASK-001 matrices absent specific material evidence.

## Navigation map

- docs/src/content/docs/reference/agent-tools-and-authorization.md Tools from Python: output/value/value_truncated and savings explanation.
- fixtures/cards/verifier_building_blocks/programs/nested-investigation.py: comment near final print; executable statements must remain identical. reduce-notes.py remains unchanged.
- docs/public/llms-full.txt: generated reference prose and embedded shared source; existing docs:generate/docs:check own generation.
- crates/wyrd/wyrd-server/src/verification/hosted/tools.rs Invocation::code and CodeRun descriptor: existing output construction and description.
- hosted/sandbox.rs Sandbox::exec and repr: last-expression/printed-output production and bounding.
- crates/vala/vala-verify/src/blocks/tools.rs Dispatch::call / RecordedTool::invoke: complete outer result bounding/delivery to model.
- Existing Rust SDK, Python, TypeScript and MCP story fixture consumers from prior scope; comment changes source text, not program behavior. Supplied remediation evidence reruns Rust SDK/MCP focused journeys.
- Applicable authority: root AGENTS.md; docs/AGENTS.md, CONTRIBUTING.md, PRODUCT.md; changes/spec-driven-development.md; agent_docs/maintainer-style.md and agent-surfaces.md; Wyrd design Agents/tools/workspaces and approved REQ-001/014. No authority change in remediation diff.

## Specialist applicability before delegation

| Contract/invariant | Role selection and rationale |
| --- | --- |
| Correct model-visible outer value/output explanation; source reference -> embedded fixture -> generated docs -> readers | One fresh ponytail-rev independently checks closure, changed claims/consumers, generation and applicable rules. Required remediation topology. |
| Record lifecycle/defaults/custom data, runtime state ownership | Carry forward prior invariant review and validated acceptance: no runtime/type/projection change. No new invariant specialist needed. |
| Multi-language/repository standards | Carry forward prior standards coverage; new write set is docs/comment/evidence only. Validator checks affected docs/fixture/generated rules; no broad cross-tier rule change requires repo-rev. |
| Accepted authority/security/tenant/result access | Carry forward prior security coverage: no dispatcher/caller/query/record/access change. Output documentation must correctly describe existing model boundary; validator traces this bounded dependency. No new security semantics requiring specialist. |
| Ownership/abstraction | No new abstraction or owner; prior maintainer omission remains valid. |
| System failure/recovery/concurrency/persistence | No executable or deployment change; prior system/additional-domain omissions remain valid. |
| Follow-up/source validation | None initially: no conflicting claims. Obtain separate focused validation if validator discovers a new blocker; do not let it validate its own proposal. |

The fresh validator receives prior reports and acceptance evidence without an intended verdict. It must independently validate carry-forward and role decisions, retain FIND-TASK-002-1 for closure, and produce findings-validation.md with explicitly empty outstanding ledger if appropriate. No convergence assessment is required for this first remediation review. Full journey suites remain reserved for final integrated change review.
