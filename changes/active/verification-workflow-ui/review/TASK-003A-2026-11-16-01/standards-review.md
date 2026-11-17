# Repository standards review

Result: **FAIL**

Subject: `/home/thorrester/Documents/GitHub/wyrd`, base `43c77a219fd10554e8c8663e980788b70f1a6296`, candidate `c233b75456fad4f45c772ec242a9d115b4d35c6b`. First review of the complete cumulative diff; no carried-forward acceptance. Reviewed independently without other discovery reports or implementation summary. Source was not edited and no Cargo commands were run by this reviewer.

## Authority coverage

| Changed surface | Applicable authorities and coverage |
| --- | --- |
| `wyrd-auth/src/revoke.rs`, server `auth/revoke.rs` | Root AGENTS Code/Testing/Locked decisions; `agent_docs/{rust,errors,maintainer-style}.md`; security posture Principal and credential lifecycle, Authorization and policy, Postgres tenant binding, Audit integrity and privacy; original task/spec revision 4 REQ-015/AC-010 and status-event design. Inspected transactional result and post-commit staging owner. |
| Server `components/github/{pull_requests,service}.rs` | Same root/Rust/error/style authorities; security posture GitHub App association and tenant binding; wyrd-design Trigger/queued execution authority. Inspected cancellation identity propagation and every changed commit/staging branch. |
| Server `components/verification/service/inspection.rs` | Root Code/server handlers; Rust/errors/style; wyrd-design Verifier execution control and recovery; security posture execution-read/permission attribution. Inspected changed cancel workflow and existing status readback. |
| Server `scribe_outbox.rs` | Root Rust ownership, dependency and audit rules; Rust/errors/style; bifrost-design System boundary, Table and row identity, execution-events definition, Resource and failure invariants; security posture managed attribution and Audit integrity/privacy. Inspected new variant, row constructor, rows/lane/weight/group activity integration, staging helper and inline test alongside existing frame machinery. |
| Server `verification/{runner,completions,mod,submissions}.rs`, boot and test_support | Root ownership, async, typed IDs, rustdoc, dependency and test-seam rules; Rust/errors/style; wyrd-design queued preparation, checkpoint acknowledgement, group completion and recovery; bifrost-design process ownership. Inspected changed methods, fields and test-support cfg boundaries, shared claim loop caller and completion fallback. |
| SQL `queries/verifier_executions.rs`, `verifier_executions/pull_requests.rs` | Root plus **`crates/wyrd/wyrd-sql/AGENTS.md`** in full; Rust/errors/style; security posture Postgres tenant binding and coordination clock; task/design transaction ownership and RETURNING constraints. Inspected actual signatures/fields, SQL UPDATE changes, typed result decoding, admission propagation and PR ending paths. |
| SQL `tests/integration/pg_verifier_executions.rs` | Root Testing/rustdoc; SQL nested AGENTS Tests; Rust/style; TESTING.md. Inspected all cumulative test/helper changes, including stored cancellation helper and final helper-doc placement commit. |
| Server integration `pg_card_declaration_route`, `pg_deferred_evidence`, `pg_github_pull_requests`, `pg_verification_groups`, `pg_verification_runtime` | Root Testing/rustdoc; Rust/style; TESTING.md; spec-driven-development TDD and task-review test-integrity rule. Inspected mechanical result adaptations, new assertions/helpers/fallback test, and four resequenced acknowledgement tests. |
| `wyrd-testing/src/verification.rs`, `tests/bifrost/server/verification_runtime.rs` | Root Testing, server/client ownership and rustdoc; Rust/style; TESTING.md journey conventions; approved task/design public Bifrost query proof. Inspected wrapper adaptation, new journey fixture/helpers and four new tests, plus extraction of batch_texts. |
| Task Markdown/evidence | `changes/{spec-driven-development,implementation-execution}.md`, task-review and implement skills; approved spec revision 4 and finalized design supplement. Reviewed recorded proof, diagnosis, TDD disclosure, exclusions and hot-path applicability. |

No other nested AGENTS applies to the changed paths. No Python, TypeScript, UI, manifest, migration, generated artifact, new wire contract or deployment file changes occur in this range; SDK-specific and docs-site-specific rules do not apply. CodeGraph was consulted before source navigation, then specific omissions and the actual immutable Git diff were inspected directly.

## Rule results

| Rule | Result and source evidence |
| --- | --- |
| SQL capability types / caller-owned transactions | PASS. Changed production methods take `&mut TenantConn<'_>`; existing cross-tenant runner state uses `OperatorPool`. New CancelledRow/EndedRow/Admitted fields contain identities/results, not pools. SQL methods do not commit; server callers stage after their own successful `conn.commit().await?`. No added PgPool/PgConnection production signature or field. |
| RLS, SQL clocks, column mapping | PASS. No added manual tenant predicates or transaction control; existing coordination SQL remains statement_timestamp-based. RELEASE/CANCEL/PR UPDATEs gain RETURNING; named FromRow fields decode returned cancellation columns. Utc occurrence timestamps are domain event times, permitted by SQL AGENTS, rather than lease deadlines. |
| Tier imports, durable behavior ownership and dependencies | PASS. New SQL result types stay in the existing Wyrd server tier, auth returns cancellation identities to the commit owner, server projects status through its existing Scribe outbox. No SDK durable behavior, new dependency, feature, alternate queue or flusher. |
| Existing Scribe admission, attribution, batching and retry owners | PASS for standards. New Status rows use existing ExecutionEvents destination, per-row Attribution, ordinary Rows projection, execution lane hash and StagedRow byte accounting. Existing frame/retry/loss owners remain responsible. Constructor builds once; retries retain row/event/time. Checkpoint schema and recovery owner are unchanged. Behavioral/concurrency acceptance belongs to the other selected reviewers. |
| Rustdoc on every changed Rust item, including test helpers and cfg paths | **FAIL**, STD-001. New/changed item documentation otherwise exists throughout changed production/test surfaces, but two panic-capable changed items omit required # Panics sections. |
| Error/security/platform constraints | PASS within changed scope. No new public error code or raw public error mapper, secret-bearing payload, OS branch, runtime construction, unsafe block, permission bypass or tenant rebind. Existing fallible queue/owner methods preserve propagation. |
| Test tiers and seams | PASS. SQL/server integration binaries use real Postgres/server ownership. Public Bifrost journeys use real client/server reads; script/crash seams live under test-support. Newly ignored journey tests follow TESTING.md's serialized opt-in journey convention and execute with run-ignored=all; this does not disable an existing gate. Existing acknowledgement assertions remain present after resequencing. |
| Failure diagnosis / independent diagnostician record | **FAIL**, STD-002. Source-backed diagnosis is recorded, but its claimed independent diagnostician report is absent from the recorded packet/history. |
| TDD execution | Historical VIOLATION explicitly retained, not a new code finding: task lines 284 onward disclose late tests and PR production drafted before observed RED. Mutation checks cannot supply historical RED. No retrospective test-writing correction can change execution order. |
| Gates and evidence | Format/tenant checks passed in the orchestrator's review run (reported with temporary cache workaround). Implementation task records prior lints and targeted checks; owner 126/126 result precedes final test-only commits, whose affected tests were rerun separately. Exact-candidate owner retry was blocked before tests by Docker-socket permissions. These are evidence limits, not a claimed fresh owner pass. Unit outbox review check was still compiling at report time. |
| Codegen/SDK gates | N/A: no generator, generated declarations, wire shape, Python or TypeScript export changes. Existing table/status contract is consumed unchanged. |
| Hot path / benchmark applicability | No mandatory benchmark triggered by this diff. Changes run in queued claim/settlement/submission/PR background writers and post-commit cancellation/revocation staging; no engine transition waits or new synchronous persistence added. Existing shared Scribe matching arms account for the new payload; append/query/direct verify/gateway execution machinery is unchanged. The task explicitly says background runner writes alone are not hot-path changes. No latency claim is inferred from this static classification. |

## Findings

### STD-001 — Missing required panic contracts on changed Rust items

Classification: VIOLATION; explicit mandatory rustdoc gate.

Rule: root AGENTS Code requires `# Panics` when an item can panic and says missing rustdoc blocks merge; `agent_docs/rust.md` Rustdoc expressly includes private helpers and test helpers regardless of visibility.

Locations and proof:

- `crates/wyrd/wyrd-server/src/verification/completions.rs:187`, `Flusher::run`: the changed body now invokes `crash.check_completion_flush()` at line 195 under test-support. That method (`verification/mod.rs:300`) intentionally asserts when armed and documents its panic. The run method's prose mentions task panic but has **no # Panics section** for this newly introduced executable panic path.
- `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs:4348`, new `stored_cancellations`: its loop awaits `status(...)` at line 4355. The status helper at line 612 uses two expect calls and explicitly documents that it panics when the read fails or the execution is not visible. The new helper documents its purpose but omits **# Panics**.

Consequence/materiality: these changed items fail an explicit merge-blocking repository gate, including the test-only fault path. This is not a request for optional prose or unrelated pre-existing documentation cleanup.

Smallest correction: add accurate # Panics sections at these two owners describing the existing injected-flush panic and delegated read/missing-execution panic respectively. No source logic, new helper or test is required. Verify the cumulative changed-item rustdoc coverage statically and run appropriate format/diff checks; preserve the behavioral tests.

### STD-002 — Independent diagnosis claimed without its required report

Classification: VIOLATION of required execution evidence; separate from behavior and rustdoc.

Rule: `.agents/skills/wyrd-implement/SKILL.md`, Diagnose unexpected failures (lines 104–110), requires a fresh diagnostician before test/timing edits and says to record its report beside the diagnosis. Task-review's Independent task implementation reviews explicitly treats test/timeout/synchronization modifications clearing failure without diagnosis **and diagnostician report** as VIOLATION.

Location: task `TASK-003A-execution-status-events.md:258–282` records a timeout-case failure, attributes it to the earlier running-status write intercepting the next-events hold, and claims an independent diagnostician agreed. Commit `8cbbf7efc` changes the two pg_deferred_evidence stalls and the pg_verification_runtime terminal/park test synchronization. No diagnostician report is linked or reproduced in the task, and no diagnostic artifact exists in the relevant active packet; the cumulative committed range contains no such report.

Source/consequence: the source supports the described mechanism: the new running status and checkpoint share the same execution lane and destination, so a next-events fault can now intercept the earlier row. The edits retain assertions and use deterministic holds/drains. The deficiency is that the mandated independent evidence cannot be inspected; claimed historical agreement is not an independently recorded report.

Smallest correction boundary: recover and attach the actual original diagnostician report if available. If unavailable, explicitly retain that historical execution-evidence violation and obtain a bounded fresh independent review of the four changed synchronization paths with the original failing command/evidence and cumulative diff, recording its present-tense findings and focused rerun results. Do not label this as proof that the earlier mandatory consultation occurred; do not fabricate historical evidence, revert source-supported tests or add sleeps. Final validator/orchestrator must distinguish restoration of current credible proof from unrepairable historical ordering.

## Limits

This specialist performed static standards review, not acceptance discovery or runtime verification. No source edits, commits, Cargo commands or broad benchmarks were performed. Fresh exact-candidate owner proof remains constrained by Docker access; accepted eventual status-row loss on process death is an explicit spec limit, not a new risk.
