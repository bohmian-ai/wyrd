# TASK-005 independent maintainer review

Result: **FAIL**

## Immutable subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Base (exclusive): `f2a726c90edbf8956c7b91c843379175ee20eac0`.
- Candidate: `99882cf824e70a601033aba4cfa5c5b27a8dfc7c`; HEAD was checked against this identity after inspection.
- Approved authority: `changes/active/verifier-building-blocks/spec.md` revision 8 and original `tasks/TASK-005-server-integration.md`, under AGENTS.md, agent rules, and the maintainer-style guide. The reference router's architecture patterns and Rust/Python/TypeScript guidance informed owner and declaration inspection.
- Reviewed the cumulative change, including deleted owners and retargeted tests, rather than the latest commit. Started with the shared navigation map and expanded through CodeGraph and repository caller searches. No peer reports were read. No production source was changed.

## Changed-surface coverage

| Changed surface | Owners, consumers, and maintainer concerns inspected | Assessment |
| --- | --- | --- |
| Verification control and projections | `components/verification/service.rs`, `routes.rs`, MCP verification/catalog wiring, router, OpenAPI, body-limit middleware; direct and enqueue callers and route/MCP contract tests | HTTP and MCP share `VerificationControl`; direct engine construction and queued construction converge on `HostedBlocks`. Typed requests and result projections remain discoverable. The local upload route's approval status belongs to the acceptance/security review, rather than a naming preference. |
| Hosted engine composition and trace reads | New `hosted/blocks.rs`, `hosted/traces.rs`, existing hosted registry/evidence/tools/workspace owners, Vala block interfaces and engine trace path, gateway invocation attribution | `HostedBlocks` retains real process-wide capacity; its per-execution executors carry accepted caller/evidence. It composes existing Vala/Skald owners rather than introducing a parallel graph executor. CallerTraces uses the ordinary query owner and shared span row decoder. |
| Queue and Scribe handoff | New `verifier_executions.rs`, migration, removed `verifier_runs.rs` and observation activation, `scribe_outbox.rs`, new `submissions.rs`, shared Outbox, Gate reservation, state/boot/shutdown wiring | Reuses the existing outbox and transactional queue pattern. SubmissionSink has real retained database/queue dependencies. One obsolete copied SQL lifecycle remains (MNT-002). |
| Runner, scheduling, fitting, and reactions | Runner claim/dispatch/result/settlement paths; cache, claims and leases; scheduler, drift, fitter, authority; operator worker/provider delivery; SQL binding/baseline/dispatch owners | Existing owners remain cohesive. The cache now retains the whole exact Verifier; result mapping remains a pure mapper; statistical scoring and delivery remain in their existing owners. |
| Canonical inputs/results and analytical contracts | `verification/results.rs`, execution-input/results table declarations and re-exports, drift feature table, retained Eval table declarations, Scribe destinations, Oracle executed-by context and query propagation | Schema bindings use named columns and owning table declarations. Durable identity and attribution paths can be followed without an alternate storage reader. No extra maintainer defect identified. |
| Existing scenario evaluation and CLI | Vala context/executor/store/scenario/scoring/compare/results/task exports and deleted judge/media modules; CLI Eval loader, record scoring, errors and output; corresponding integration/unit fixture changes | Removed judge implementation is cut over in callers; the remaining scenario executors and CLI load a bare Eval spec. Changed test wiring has undocumented items (MNT-001). |
| Shared client and three SDKs | `Observe` staging/enqueue paths, `StagedRun`, state projections and Cards fixture; Python observe/testing bindings and authored stubs; TypeScript NativeRun staging and generated declarations/card/error types; mirrored journey changes | Foreign-runtime staging remains in the shared client with thin binding projection. Python and TS declarations describe their native changes; no material signature/documentation contradiction found. |
| Test/runtime fixtures and edge proof | Server route/runtime/registration/OpenAPI/operator tests, SQL execution/baseline/binding tests, shared VerificationFixture/test server, Bifrost verification/observe tests, MCP tests and SDK support/negative/replay/trace/telemetry journeys | Outcome-focused real-server proof is present. Some changed Rust test symbols remain outside the mandatory rustdoc standard. The retained SQL trace-wait test exercises no production path. |
| Capacity and delivery tooling | Capacity fixture/load/evidence/report/step/main changes and deleted mock judge; build script, release workflow, three Dockerfiles, mise lanes, Cargo manifests/lockfile | Worker packaging follows the server's sibling-binary resolution. The benchmark now describes assertion-only direct/queued traffic consistently with its changed driver; whether losing previous workloads is permitted is an acceptance/regression question. |
| Documentation, examples, UI fixture | Runtime configuration/limits/deployment/MCP docs; Wyrd design/security trace terminology; card YAML/JSON fixtures and fixture README; UI mock Service binding | No structural UI change. Runtime settings and worker installation are explained at the owning operator surfaces. External example tests are supplied evidence only; their uncommitted built artifacts are not immutable review inputs. |

## Material proposed finding

### MNT-001 — Changed Rust items do not satisfy the required rustdoc gate

- Classification: **VIOLATION**.
- Governing authority: AGENTS.md §16 requires rustdoc on every new/materially modified Rust item, explicitly including private methods, test helpers and test functions; fallible functions require `# Errors`, and remaining panic conditions require `# Panics`. The task-review skill explicitly keeps missing rustdoc blocking. Maintainer Style, “Documentation,” carries the same requirement.
- Exact changed locations:
  - `crates/shared/wyrd-runtime/src/outbox.rs:107`: newly introduced `Debug::fmt` has no method rustdoc (the preceding prose documents the impl, not this method).
  - `crates/vala/vala-eval/src/tasks/assertion.rs:297`: materially rewritten `assertion_stage::executors` now delegates to the replacement assertion executor bundle, with no rustdoc.
  - `crates/vala/vala-eval/src/tasks/agent.rs:148` and `tasks/trace.rs:279`: executor-bundle helpers changed to the post-cutover set, with no rustdoc.
  - `crates/vala/vala-eval/src/executor.rs:939`: the materially rewritten full-spec cross-stage/aggregation test has no rustdoc at all. Its module documentation and inline Arrange comment do not document the test item.
  - `crates/wyrd/wyrd-server/src/verification/engines.rs:102`: new `tests::requirement` calls `TaskId::new(id).expect(...)` and decodes its check with `expect`, but its one-line rustdoc omits the panic contract.
  - `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:2197`: new `tests::request` constructs a CardUid with `expect`, but omits `# Panics`; its prose also calls the subject fixed although each invocation creates a fresh UID.
- Source evidence and callers: these bodies are part of the cumulative diff. The executor factories feed their owning assertion/agent/trace test drivers; the full-spec test exercises stage dependency propagation and aggregation. `engines::tests::requirement` feeds `failure_context_counts_requirements_and_the_one_drift_check`; SQL `tests::request` feeds `keyed_execution_id_is_stable_and_content_scoped`. The new Outbox formatter is reached by derived Debug on staged writes carrying an outbox.
- Concrete maintenance cost: the new outbox formatting contract and changed test-fixture/executor responsibilities are omitted from generated/private Rust documentation, and panic assumptions are hidden at the fixture boundary. This fails an explicit completion requirement independent of a green build; it is not a request for unrelated documentation cleanup.
- Smallest testable correction: add substantive rustdoc directly to these changed items, documenting their workflow responsibility and relevant error/panic conditions. Correct the SQL helper's fresh-subject wording. Keep implementation, assertions, timeout behavior, and all unaffected old helpers unchanged. Batch these edits with any bounded implementation remediation; no style-only round or new test is needed.
- Closure proof: inspect the item-level docs against each full body and run `mise run fmt`. A full gate is unnecessary for documentation-only closure; compilation does not by itself prove this rule.

## Deletion-only proposed finding (no production caller)

### MNT-002 — The copied trace-wait queue API retains a retired second lifecycle

- Classification: **DRIFT**, specifically deletion of unearned code, not an observed runtime failure.
- Governing authority: AGENTS.md §15 requires the smallest complete implementation and forbids machinery without an earned caller; the task's consumer cutover and INV-008 exclude replaced workflows. Maintainer Style places a workflow with its actual owner, and the review skill specifically requires existing workflow/caller comparison and allows a finding whose correction deletes unearned code.
- Exact changed locations: `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:375` (`AWAIT_TRACE_SQL`), `:929` (`TraceWaitOutcome`), and `:1644` (`VerifierExecutionQueue::await_trace`). Its sole consumers are the SQL integration test at `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs:1834` and that file's import.
- Existing alternative and producer-to-consumer evidence: the old queue/module and server Eval runner are deleted. The new `HostedBlocks::engine` injects CallerTraces into the shared VerificationEngine for direct and queued paths. `VerificationEngine::trace` at `crates/vala/vala-eval/src/blocks/engine.rs:420` fetches once and converts any source error to an inconclusive judgment at lines 433–435. Runner dispatch consumes this engine outcome and has no trace-wait transition; `RuntimeLimits.trace_poll` and `trace_deadline` were removed. Repository searches of `await_trace` and `TraceWaitOutcome` find only the method/type, their internal uses, and the SQL test. There is no caller to extend or missing deployed behavior this retained API provides.
- Concrete maintenance cost: the SQL owner publicly exposes and tests a retired trace-arrival deadline/requeue state machine, refunding attempts and timing out the execution, while the integrated engine follows a different trace failure contract. A maintainer can mistake this green test for proof of the shipped trace workflow and must preserve an unused query/API during future queue changes.
- Smallest correction: delete only that SQL constant, enum, method, corresponding dedicated test, and now-unused test import. Preserve ordinary retry, release/backpressure, lease fencing, status, and the shared engine's trace semantics. Do not wire the API into the runner, add a second poller, or change trace readiness behavior to justify it.
- Focused closure proof: repository search shows no remaining references; compile/check the SQL crate through `mise exec -- cargo check --locked -p wyrd-sql`. Existing engine trace behavior and current runner paths must remain unchanged. No new test is needed for deleting an API whose only caller is its own test.
- Calibration: this is a deletion-only proposal. It does not assert that a dormant path changes current user behavior. Independent validation should retain it only under the skill's explicit unearned-code criterion; otherwise omit it, rather than prescribing new behavior.

## Non-blocking notes

No personal-preference refactors are proposed. Existing nearby patterns support the composition choices: `VerificationControl` remains the transport-independent control owner, `Outbox<S>` remains the batching/retry owner, and `ClaimLoop`/queue/fitter/Operator owners retain their respective lifecycles. The new HostedBlocks and SubmissionSink types own actual state; zero-sized utility owners were not introduced in those paths.

## Verification limits

This was a static independent maintainer review. I did not rerun the broad gate, external builds, live model providers, or the intermittent Oracle test. Supplied evidence records a green gate on `03712af5e` and external local builds, with later HEAD recording evidence. Those results do not prove the rustdoc omissions acceptable or the test-only trace-wait API used by the product.

No source edits, merges, pushes, or deployments were performed. The source candidate remained `99882cf824e70a601033aba4cfa5c5b27a8dfc7c` throughout this pass.
