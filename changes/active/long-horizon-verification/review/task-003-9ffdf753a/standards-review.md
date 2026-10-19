# TASK-003 repository standards review

Overall: **FAIL**. Repository-rule findings below need independent validation. The specification identity discrepancy is a separate authority limit, not a code finding.

## Immutable subject and method

- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate: `9ffdf753ab11ce9b638c906388d3690a6aede249`.
- Reviewed the complete `git diff ef3a57b07...9ffdf753a`, including all 32 paths, not an implementation summary. Used CodeGraph first for Outbox, Reservation, SubmissionSink, ClaimLoop, VerificationControl and ScribeSink, then expanded the changed owners, callers, bindings and tests directly.
- Read `AGENTS.md`, `architecture/agent-rules.md`, the reference router and all selected references completely. The table below identifies their scope. Read the full Wyrd design, doctrine, Bifrost design, security posture, operations deployment/release and reliability/recovery authorities, and `TESTING.md`.
- Checked owning manifests, workspace toolchain/dependency policy, Python package configuration, lockfile package declarations, testing exports and relevant `mise.toml` lanes. No dependency or Cargo-feature change appears in the range.
- Read the task's implementation evidence as claims. No raw verification logs or diagnostician reports were supplied. No other discovery report was consulted.

## Authority coverage

All rows also apply `AGENTS.md`, `architecture/agent-rules.md`, Wyrd design/doctrine, `languages/maintainer-style.md`, `languages/spec-driven-development.md`, `languages/implementation-execution.md` and the reference router. Paths below are repository-relative; abbreviated SDK names denote their existing `sdks/wyrd-sdk-*` directories.

| Changed surfaces (complete path coverage) | Additional applicable authorities | Source inspected / governing concern |
|---|---|---|
| `crates/shared/wyrd-runtime/src/outbox.rs` | `languages/rust-core.md`; `architecture/patterns.md`; `doctrine/architecture-constraints.md`; `languages/testing-workflows.md`; Bifrost design; security posture; operations reliability/recovery | Outbox/Reservation/Admission and writer receive, dispatch, finish, retry, release and shutdown ownership; existing MemorySink helpers and both changed tests; finite admission, non-blocking audit, accounting and rustdoc |
| `crates/wyrd/wyrd-server/src/components/verification/service.rs` | Rust core; patterns; constraints; `languages/errors.md`; `domain/evaluation.md`; `domain/telemetry-observations.md`; security posture | Full execute/enqueue/stage_direct bodies and their authorization, payload and tenant-resolution seams; typed pressure and request-bound reservations |
| `crates/wyrd/wyrd-server/src/scribe_outbox.rs` | Rust core; patterns; constraints; `domain/olap-serving.md`; `domain/arrow-analytical-interop.md`; telemetry observations; `domain/analytical-operations-reliability.md`; Bifrost design; security posture | ScribeWrite, bounded construction, span producer, write frames and ACK-to-submission handoff; unchanged tenant/attribution boundaries and shared audit destination |
| `crates/wyrd/wyrd-server/src/verification/{claims,mod,operators,runner,submissions}.rs` | Rust core; patterns; constraints; evaluation; analytical reliability; security posture; operations reliability/recovery | Concrete retained owners; active-task cap in shared claim loop and Operator caller; queue connection types; result reservation, execution and fenced settlement; SubmissionSink transaction |
| `crates/wyrd/wyrd-server/src/{config.rs,boot/mod.rs}` | Rust core; patterns; operations deployment/release and README; Bifrost design | ScribeRuntimeConfig default/validation and composition parameter closure; portable process-local configuration |
| `crates/wyrd-spec/src/error.rs`; `crates/wyrd/wyrd-server/src/http/error.rs`; `docs/src/content/docs/api/errors.md`; `sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts` | Errors; `languages/agent-harness.md`; `doctrine/positioning-and-vocabulary.md`; testing workflows | Derive-backed catalog variant, detail normalization, central Retry-After mapper and changed mapper test; generated projections |
| `crates/wyrd/wyrd-testing/src/server.rs` | Rust core; testing workflows; TESTING; patterns | Builder field/default/setter and bound-server configuration consumption; existing test-only owner reused |
| `crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs`; `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs`; `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs` | Rust core; testing workflows; TESTING; agent harness for MCP; security posture | New and materially changed test/helper bodies, their server/fixture setup, runtime limits, API consumers, pressure assertions and rustdoc |
| `sdks/wyrd-sdk-python/src/testing.rs`; `sdks/wyrd-sdk-python/python/wyrd/testing/__init__.pyi` | `languages/pyo3-boundaries.md`; `languages/python-api-and-stubs.md`; testing workflows; Rust core | __new__ option, retained field, __enter__ builder projection, testing feature and native registration/public export; authored stub parity |
| `sdks/wyrd-sdk-python/tests/integration/{conftest.py,support.py,test_verify_in_real_time.py}` | Python API/stubs; testing workflows; TESTING; security posture | Shared provider/assistant setup, explicit neighbor identity, public SDK and dev-only server use, top-level tests and typed error assertions |
| `sdks/wyrd-sdk-ts/native-testing/src/lib.rs`; `sdks/wyrd-sdk-ts/testing/native.d.cts` | `languages/typescript-guide.md`; Rust core; testing workflows | N-API option and async builder projection; declaration parity; separate testing package boundary |
| `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts`; `sdks/wyrd-sdk-ts/wyrd/tests/support/{assistant,server}.ts` | TypeScript guide; testing workflows; TESTING; security posture | Typed neighbor/client use, shared provider/server fixture scope, pressure/recovery assertions and helper signatures |
| `architecture/wyrd-design.md`; `docs/src/content/docs/build-and-verify/verify/verify-a-request-in-real-time.md`; `docs/src/content/docs/reference/verification-runtime-limits.md` | Positioning/vocabulary; errors; patterns; security posture; operations reliability/recovery | Staging-only receipt vocabulary and documented typed refusal; normative operational/audit contract consistency |
| `changes/active/long-horizon-verification/tasks/TASK-003-bounded-outbox-admission.md` | Spec-driven development; implementation execution; task-review skill | Added execution evidence, claimed commands, diagnostics and completion limits |

No UI, migration, new schema layout, DataFusion planning, compaction or Iceberg publication implementation changed; those deeper mechanics do not acquire separate language/domain obligations in this standards pass. Operational runbooks contain no changed incident procedure to audit.

## Rule results

| Applicable rule | Result | Source evidence |
|---|---|---|
| Rust/server ownership, declarative foundational contracts, first-class client projection (`AGENTS` §§2–3, 7–9) | PASS | Generic admission stays in wyrd-runtime; durable queue/claim/settlement stays in server-owned orchestration and existing SQL owner. SDK production logic only gains a generated error code. Testing controls remain in existing testing boundaries. |
| Cohesive concrete retained owners (`AGENTS` §5; Rust core/patterns) | PASS | Admission owns counters, Reservation owns admitted unspent slots, OutboxWriter retains backlog/retry state, ClaimLoop retains process limits. No utility owner or parallel durable resource appears. |
| Actual SQL capabilities and caller-owned transactions (agent rules) | PASS | ClaimLoop and SubmissionSink retain WyrdPostgres and open tenant_conn locally; claim/resolve receive `&mut TenantConn<'_>`. VerifierRunner and OperatorWorker retain `OperatorPool` for due-tenant work. Service tenant resolution accepts TenantConn. No changed production function or field adds PgPool, naked connection or caller transaction. Existing raw pools observed in Scribe/boot test modules and integration assertion fixtures are test-only; the testing server's existing app_pool accessor is outside this diff. No check output was used as a substitute for this inspection. |
| Permission blocks; audit does not; one Scribe outbox (`AGENTS` §2; security/Bifrost authorities) | PASS for code path; FAIL for authority consistency | authorize_run still precedes producer reservation, audit uses unreserved stage, and overflow logs/counts loss without returning an audit error. No second audit writer. Normative prose still promises an unbounded queue (STD-003-02). |
| Public errors derive once and project through central boundaries (`AGENTS` §4; errors/agent harness) | PASS | VerificationStagingFull metadata is in WyrdError, normalized by existing detail path; HTTP mapper adds Retry-After through its existing branch; generated error docs and TypeScript union match. |
| PyO3 lifetime/runtime/placement and authored stub parity (`AGENTS` §§7–8) | PASS | New option is an extracted Option<usize>, copied into builder through existing shared runtime bridge; no Bound crosses await. Signature/default/field/stub names agree. Existing testing-only registration and public `wyrd.testing` export remain intact. |
| TypeScript types and N-API projection | PASS | Option uses optional number and native Option<u32>; test options carry readonly properties; no durable semantics moved into TypeScript and no new any boundary. Generated declaration source matches the new field. Generation execution remains a recorded claim. |
| Rust item intent/error/panic documentation (`AGENTS` §16) | FAIL | Missing new staging error conditions on execute/enqueue, and missing panic sections on changed constructors/tests (STD-003-01). This is the explicit rustdoc blocker, not a discretionary naming/style objection. |
| Portable runtime, IO-earned async, no new runtimes/dependencies (`AGENTS` §§2, 4, 6) | PASS | Atomic admission is synchronous; IO owners keep async. Existing runtime bridges and local server fixtures are reused; no target_os gate or new dependency/feature. |
| Test tiers and runtime ownership (`AGENTS` §11; TESTING) | PASS for source placement | New Rust/Python/TS journeys drive public clients against bound servers, MCP exercises a real tool path, and native lifetimes are tested in their language. Queue unit tests remain inline. Postgres tests remain in gated existing targets. This does not assert their executions passed. |
| No weakening checks to clear failures; diagnosis evidence (task-review skill) | FAIL evidence | Prior tests are adapted to explicit 70/200 slots and preserve their outcomes; source is consistent with the stated cap diagnosis. However independently inspectable diagnostician reports required by the review protocol are absent (STD-003-03). |
| Generated outputs source-owned (`AGENTS` §8; agent rules) | PASS source correspondence / execution unverified | Catalog and N-API Rust source carry each added generated field/code. Reported codegen and N-API checks lack raw logs; no evidence establishes hand editing. |
| Required verification (`AGENTS` §11) | LIMITED | Task records scoped tests, lints, codegen, docs and language checks as passing. Reviewer independently ran only git diff --check; no Cargo, pytest or TS execution was started. Recorded command summaries are not promoted to independently reproduced evidence. |
| Immutable review | PASS | HEAD checked before review and at report completion remains candidate. Only this report is written by this reviewer. |

## Proposed source-local findings

### STD-003-01 — Changed Rust contracts omit required error/panic documentation

- Classification: **VIOLATION**; blocking under the explicit rustdoc gate.
- Rule: `AGENTS.md` §16 and agent rules require substantive rustdoc for every materially changed item, a `# Errors` section naming error conditions, and `# Panics` whenever a panic remains possible, including tests and private helpers.
- Locations: `crates/wyrd/wyrd-server/src/components/verification/service.rs:453` and `:612` (execute/enqueue Errors sections); `crates/wyrd/wyrd-server/src/scribe_outbox.rs:652` and `:663` (outbox/bounded_outbox); `crates/shared/wyrd-runtime/src/outbox.rs:1069` and `:1129` (new bounded tests); `crates/wyrd/wyrd-server/src/http/error.rs:410` (materially extended mapper test).
- Evidence: execute and enqueue now propagate VerificationStagingFull from reserve, but their error sections omit it and its before-staging boundary. Both Scribe constructors delegate to Outbox::new, whose documented runtime precondition panics outside Tokio, without stating that precondition themselves. The new/changed tests contain assert/expect failure paths but lack Panics sections.
- Consequence: the changed invocation contract does not document its newly expected retryable failure, and the mandatory repository rustdoc gate is incomplete despite compilation/lint claims.
- Required testable correction: update the existing touched-item rustdoc to describe the staging refusal and runtime/assertion panic conditions. Recheck the materially changed items together; no new API or runtime logic is needed. Source inspection plus the existing Rust format/lint lane proves this correction.

### STD-003-02 — Approved bounded audit behavior leaves contradictory normative security and operations contracts

- Classification: **VIOLATION**.
- Rule: Wyrd design introduction requires replacing superseded architecture explicitly in the same cohesive change; reference-router authority requires all applicable security/operations authorities to agree. Spec-driven development requires lasting contracts to be reflected in their owners.
- Changed location: `crates/shared/wyrd-runtime/src/outbox.rs:220` and `architecture/wyrd-design.md:1607`; contradictory authorities: `architecture/wyrd-security-posture.md:469` and `architecture/operations/reliability-and-recovery.md:73`.
- Evidence: new stage rejects an item at the unreserved share, counts it lost, and returns. Security posture still states “The outbox queue is unbounded.” Reliability/recovery retains an explicit exception saying the Scribe outbox is unbounded because holding a write is preferred to dropping it. The candidate updates only the submission paragraph of Wyrd design, leaving those audit/overload contracts unchanged.
- Consequence: operators and security reviewers are told outage pressure retains audit/capture writes in memory, while the process now intentionally discards excess writes. This is a material data-loss/operating-contract discrepancy, not cosmetic wording.
- Required testable correction: synchronize these existing normative paragraphs to the approved finite process staging and counted non-blocking overflow loss behavior, retaining identical retry slices and the acknowledged/unacknowledged crash boundary. Do not restore an unbounded queue or add another writer. Static consistency inspection is sufficient for this prose correction.

### STD-003-03 — Failure-driven test adaptations lack the required inspectable diagnostician reports

- Classification: **VIOLATION** of review evidence discipline; distinct from a demonstrated runtime defect.
- Rule: task-review skill says a test/assertion/timeout/sleep/skip change made to clear a failure “without a recorded diagnosis and diagnostician report” is a VIOLATION.
- Location: `changes/active/long-horizon-verification/tasks/TASK-003-bounded-outbox-admission.md:235–239`; affected changes include `crates/wyrd/wyrd-server/tests/integration/pg_verification_runtime.rs:2865` and `sdks/wyrd-sdk-ts/wyrd/tests/support/server.ts:157`.
- Evidence: the appended diagnoses describe observed failures and assert independent confirmation; no report path, report artifact or raw diagnostic trace is supplied. Source supports the 64-slot/held-run incompatibility and shows the fixture scope correction, but cannot establish the claimed diagnostic review. The orchestrator confirmed no further reports were supplied.
- Consequence: independent validation cannot audit whether failure-driven test changes preserved the intended proof using the required evidence, even if their final shape is plausible.
- Required correction: supply the existing diagnostician reports/traces tied to these adaptations, or explicitly obtain the missing diagnosis evidence before accepting the modifications. Preserve current test assertions; this finding does not justify undoing correct slot setup or requiring a new production mechanism. Validator should adjudicate this evidence-only issue separately from code behavior.

## Authority and verification limits

The subject requests approved specification revision 13, while the available spec is revision 14/draft. The text records approval of revision 13 and says revision 14 concerns pre-release table schemas; no table-schema change appears in TASK-003. This reviewer did useful standards inspection under the unchanged task obligations and did not approve revision 14. Final acceptance remains subject to the orchestrator resolving that identity discrepancy.

Checks actually run: CodeGraph exploration; complete cumulative diff and source/authority reads; `git diff --check ef3a57b07...9ffdf753a` (PASS); `git rev-parse HEAD` (candidate). No runtime test, build, lint, code generation or docs build was rerun. Reported implementation checks remain unverified claims.

Skipped runtime reruns to keep this audit read-only and avoid overlapping shared Cargo lanes. Main review risks are the unresolved spec identity and missing diagnostic artifacts; source findings above remain independently actionable.
