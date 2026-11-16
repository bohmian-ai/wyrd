# TASK-002 first acceptance review

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode
Base: e5af2ce5caef2d00303e1c980b12c8a9bca4af0e (parent of first TASK-002 implementation commit).
Candidate: ff78b51b61fea8f215a2139b17840044f65b17e4.
Diff: git diff e5af2ce5caef2d00303e1c980b12c8a9bca4af0e...ff78b51b61fea8f215a2139b17840044f65b17e4.
Approved authority: revision 2 of changes/active/verifier-code-mode/spec.md at candidate (and base; identical). Read using git show, since working spec has an unrelated revision 3 edit.
Original task: changes/active/verifier-code-mode/tasks/TASK-002-public-code-mode-workflow.md at base; its pre-Implementation-Evidence portion is unchanged at candidate.
Skill: .agents/skills/wyrd-task-review/SKILL.md.

First review of the full TASK-002 range, no carried-forward TASK-002 findings or acceptance evidence. TASK-001 is a dependency, not part of this diff; inspect its unchanged runtime owners where needed for TASK-002 obligations without re-auditing its entire fault matrix. Working spec revision, TASK-001 review directories, spec-revisions and TASK-003 are excluded. No .codegraph directory exists. Source stays immutable. Reports only in this directory. No build/test mutations to reviewed source.

## Shared navigation map (starting points, not coverage conclusions)

- Python: python/wyrd/verify/__init__.pyi ToolRecipient, ToolCallRecord, RequirementResult; src/verify.rs PyVerificationResult requirements/steps/typed and json_to_pyobject. tests/typecheck/verify_contracts.py and integration/test_verifier_building_blocks.py; integration/conftest.py provider phases.
- TypeScript: wyrd/src/card-types.ts generated wire contract; src/index.ts typedResult and RequirementResult; tests/types/verification-data.test-d.ts; tests/integration/verifier-building-blocks.test.ts provider/journey.
- MCP: crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs pg_tests code_mode_preserves_authority_and_records and existing execute/enqueue/readback helpers; server src/mcp/verification.rs typed handlers.
- Served contract: crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs verification_tool_records_match_served_contract; wyrd-spec result and ToolCallRecord declarations, schema defaults and error catalog.
- Shared fixture: fixtures/cards/verifier_building_blocks/programs/{nested-investigation,reduce-notes}.py; existing Cards, Rust SDK verifier_building_blocks.rs and each language's provider script load these programs.
- Docs: reference/agent-tools-and-authorization.md, reference/verification-runtime-limits.md, operate/configure-verification-runtime.md, build-and-verify/verify/run-a-verification-and-inspect-its-evidence.md; docs/public/llms-full.txt generated consumer.
- Runtime dependency: vala-verify Agent tool dispatch, accepted authority, code host call handling and result assembly; shared client verify Execution immediate/queued/retained paths. Inspect relevant owners from source as needed.

## Contract coverage and role selection

| Contract / invariant, owners and consumers | Role | Source-backed rationale |
| --- | --- | --- |
| Nested public execution, supported citations, provider-visible catalog and reduced output; shared programs -> real Python/TS/MCP journeys -> results/docs | behavior-rev | Required first comprehensive acceptance discovery. Covers all task criteria, non-goals, maintainer/Ponytail and credible failure paths. |
| Record recipient/defaults/custom-data preservation through immediate, queued, typed and retained paths; spec/server/client -> Python mappings, TS spread, MCP output/readers | invariant-rev | Multi-stage value lifecycle crosses shared serialization, typed transformation and durable readback. Task explicitly requires all paths despite unchanged runtime. Scope is these invariants, not TASK-001 full runtime matrix. |
| Python stubs/type fixtures, TypeScript tests/types, Rust SDK/MCP/schema tests, docs/generated consumer and shared fixtures | repo-rev | Broad cross-tier and multi-language rules require independent standards coverage, including changed Rust test/helper rustdoc and authored/generated parity. |
| Accepted authority, downstream denial/no protected data, scoped MCP writes, capture visibility; server dispatch/auth -> nested programs and public result readers/docs | domain-rev-security | Explicit task success/denial/recovery and safe record obligations make security a material acceptance boundary even with unchanged runtime. Read security posture and trace producers/consumers. |
| Ownership/abstraction | omit separate maintainer-rev | Diff adds tests, authored stubs, shared test programs and docs; no production orchestration, retained-state owner, abstraction or dependency introduced. Behavior reviewer covers local reuse and fixture/provider maintainability. |
| Process failure/recovery/deployment | omit separate system-rev | Diff changes no runtime executable, worker ownership, protocol, startup setting or deployed topology. Public docs must match existing worker wait/upgrade/host-failure behavior; behavior reviewer traces these bounded claims. No TASK-001 lifecycle re-review. |
| Persistent data isolation / concurrency | no additional domain role | No storage, transaction, SQL, coordination or concurrency implementation changes; invariant reviewer covers required retained evidence preservation and security reviewer covers tenant/authority boundary. |
| Final source/materiality/coverage validation | fresh ponytail-rev after discovery | Mandatory independent validation of every proposal and specialist applicability, including explicitly empty ledger if appropriate. |

Reports: task-review-behavior.md, task-review-invariants.md, standards-review.md, domain-review-security.md, findings-validation.md, verdict.md.

Verification evidence is recorded in the candidate task's Implementation Evidence section. Treat command outcomes as supplied execution evidence, not independently re-run results. Read original task separately from that evidence; do not use implementation completion prose as discovery instructions. Focused rechecks may be run via mise after inspecting owning tasks; serialize Cargo-backed work across the checkout. Full journey suites belong to final change review.
