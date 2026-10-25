# TASK-006 first review

Repository: /Users/stevenforrester/Documents/GitHub/schema-change-task-6
Base: 5fe833547c66916c8c4868afc8e07d985dcba7d7
Candidate: c95359fd608fa65d4f0422820751490f61c99202
Range: `git diff 5fe833547...c95359fd6` (all seven implementation commits, including f456eb702). Clean initial worktree. No .codegraph directory. No earlier TASK-006 verdict; no carried evidence or remediation.

Authority: approved revision 16 `changes/active/service-identity-and-telemetry/spec.md`; original `tasks/TASK-006-verification-trace-evidence.md`; supplement `supplements/table-storage-and-correlation.md` whole-trace decision. Root AGENTS, SDK Python/TS nested AGENTS, `changes/spec-driven-development.md`, `changes/implementation-execution.md`, `agent_docs/index.md`, rust/errors/maintainer-style guides, TESTING.md; applicable Verifier/query/security architecture. Read owning sections, including baseline architecture if changed authority needs comparison. Task footer supplies available verification and diagnoses; treat these as claims, not conclusions.

Navigation starting map (expand independently):
- vala-verify/src/blocks/engine.rs: VerificationEngine::run/trace/read, TraceReads, defer_unlanded; engine/tests.rs shared fetch proof; trace_source.rs errors/projection. TraceSource sibling implementations/callers matter.
- wyrd-server/src/verification/hosted/traces.rs: CallerTraces statement/fetch, ordinary caller query, SPAN_COLUMNS and overflow. hosted/blocks.rs constructs engine; components/verification/service.rs direct construction.
- verification/runner.rs execute/dispatch/settle -> wyrd-sql VerifierExecutions claim/retry/complete and RetryPolicy; accepted execution authority, deadline, final-attempt mode, fenced publication.
- server/tests/integration/pg_verification_runtime.rs I1/I2/I3 and modified helper; pg_hosted_artifacts constructor change.
- wyrd-testing/src/verification.rs, bifrost/canonical_signals.rs shared harness changes; tests/bifrost/server/verification_runtime.rs J1/J2/delegated history.
- MCP tests/bifrost/mcp/verification.rs public mirror; Python tests/integration/test_verifier_building_blocks.py and TS wyrd/tests/integration/verifier-building-blocks.test.ts public mirrors.
- fixtures/cards/verifier_building_blocks/{policy-desk,whole-trace}.yaml and fixtures/README.md; existing Rust building-block consumers.
- architecture/wyrd-design.md Verifier; architecture/bifrost-design.md query-service prose.

Reviewer selection: behavior-rev comprehensive acceptance/failure diagnosis/reuse audit; invariant-rev attempt-local sharing and durable queue accepted-state/publication invariants; system-rev changed retry/failure/restart behavior; repo-rev cross-tier Rust/Python/TS/fixture/architecture rules, changed-item rustdoc, required evidence gates. Separate maintainer specialist not selected: no substantial owner replacement, behavior reviewer covers reuse/maintainability. Separate domain specialist not selected: query/RBAC/tenant/SQL capability owners reused, no new policy or persistence schema; invariant and system reviewers trace their preserved boundaries. Always follow with fresh independent ponytail validation.

Review source immutable. Only write assigned reports. Do not run Cargo without orchestrator coordination. No implementation edits, staging, commits or pushes. Review directory contains reports only.

## Discovery comparison and validation routing

All four selected discovery reports are complete. Behavior and standards both propose missing observed RED evidence; consolidate as one proposal for independent validation. Behavior separately proposes absent independent diagnostician reports; standards proposes missing panic docs on three new test helpers. Invariant and system report no material findings within assigned scopes. These differences concern covered obligations rather than contradictory source claims. No followup discovery selected: independent source validation can resolve authority/materiality of each proposal, and the two newly observed consumer failures passed unchanged in focused serial reruns with the shared-harness execution rule/source paths recorded. A fresh ponytail-rev receives all reports and current verification; it will decide which proposals survive. No proposed finding is retained by the orchestrator without that validation.
