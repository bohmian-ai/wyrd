# TASK-002 remediation round 1 immutable subject
Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-002
Original base: 41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0
Prior candidate: d342c54749a871dcacf5b0c7ec29e3b458f2d9e2
Candidate: 99cf2f18a8cadfe3c500cf7157934047112d156a
Review complete original base-to-candidate cumulative.diff, not only remediation.diff. Use remediation.diff to locate changes/closure. Source and HEAD must remain unchanged; only assigned reports may be written. No commits or packet edits.
Approved authority: changes/active/verifier-building-blocks/spec.md revision 6, frozen in spec-snapshot.md. Original task is original-task.md. Prior verdict/ledger in prior-verdict.md and prior-findings-validation.md. Required remediation outcome is remediation-task.md. Full task+recorded evidence frozen in remediation-task-snapshot.md; recorded verification commands in verification-evidence.md. Do not use implementation summary as proof. Parent packet remains untracked/user-supplied; no commit requested.

## Shared starting navigation map; independently expand/verify
- executor.rs: RunLedger<O>, Running, Prepared, walk, execute_plan and legacy gate/snapshot preparation. Inspect shared lifecycle ownership, generic/public re-export consumers, cancellation, deterministic ordering and old scenario semantics. Existing scenario.rs execute_scenario and orchestrator/scoring.rs ScenarioScoring remain consumers.
- blocks/engine.rs: VerificationEngine run/drive/step/requirement/judge, Block/Key and result adaptation. Follow both entry points into shared owner and verify earlier duplicate stage machinery is removed rather than wrapped.
- blocks/scope.rs: Scope reads original declarations and RunLedger<Done>; Cause/Missing/Settled/Done represent settled state, reference selection, source lineage. Compare old ContextSnapshot/TaskRegistry ownership and sibling paths.
- blocks/drift.rs: DriftChecks::judge/scored, per-requirement fitted state/per-slot samples, selected Custom null handling, score_drift and existing direct/scheduled scorer callers.
- tasks/assertion.rs: evaluate_single/evaluate_item_mode shared with legacy assertion; execute_assertion docs.
- tasks/trace.rs: project_spans and shared trace fixture/module. results.rs: capture_actual and legacy apply_context_capture.
- blocks/engine/tests.rs: new skip-owner/elapsed-deadline/Custom completeness regressions plus original engine tests; executor/assertion/scenario tests cover preserved sibling behavior. Verify test/diagnosis claims from bodies; do not restore old code during immutable review.
- blocks.rs/lib.rs reexports and Cargo.toml/Cargo.lock: original addition vala-drift, cumulative public surface and dependency costs. Generic RunLedger and boxed Done results need ordinary scope/caller review; no intended finding.
- unchanged prerequisite wyrd-spec VerifierSpec::plan/validation/reference/results and shared action/check seam are authority consumers, not privately editable.

## Prior finding hypotheses (must verify closure in current source)
FIND-TASK-002-1: duplicate execution/context ownership.
FIND-TASK-002-2: owning-step intentional skip hides failed required evidence.
FIND-TASK-002-3: immediately ready work can bypass expired absolute deadline.
FIND-TASK-002-4: Custom selected metric nulls discarded into healthy score.
FIND-TASK-002-5: touched-item rustdoc omissions (use prior validated item table; rejected fixed-fixture claims stay excluded absent new source evidence).
Preserve stable IDs; newly validated defects start at FIND-TASK-002-6. Do not rediscover the same defect caller by caller.

## Authority and workflow
Read .agents/skills/wyrd-task-review/SKILL.md and execute your assigned role fully. Read AGENTS.md, architecture/agent-rules.md, architecture/wyrd-design.md, architecture/wyrd-doctrine.mdx, architecture/references/languages/spec-driven-development.md and maintainer-style.md; select applicable complete references through architecture/references/README.md. Use CodeGraph before locating/understanding code (.codegraph exists). Approved spec explicitly replaces old authored Verifier kind shape; unrelated authority remains.
Domain auditors: concurrency (shared walker, task lifecycle, error/cancel/timeout/retry/limits); drift/evaluation (statistical adapter, completeness/readiness/provenance). No materially changed security/tenancy/persistence surface apparent; verify this.
Discovery reviewers must not read other current-round discovery reports or root conversations. No intended verdict. Audit full cumulative acceptance, not only five closure claims. Original TASK-002 authorizes isolated library lanes and excludes integration/journey suites. Root will run Cargo checks sequentially and provide review-verification.md/logs; do not overlap Cargo processes. Source inspection/review is independent and source immutable. You are not alone; never revert anyone's edits.
