# Repository Standards Review

Subject: `a51af030b6039eea4b2914f3ebf2c31925d08721..eb22b03f2bb766886d839bda23aafbd4ba130ab3`

Overall result: **FAIL**

## Material findings

### Important

- `STD-001` — [`crates/skald/skald-workflow/src/workflow.rs:811`](../../../../../crates/skald/skald-workflow/src/workflow.rs), [`workflow.rs:1246`](../../../../../crates/skald/skald-workflow/src/workflow.rs), [`workflow.rs:1473`](../../../../../crates/skald/skald-workflow/src/workflow.rs), [`workflow.rs:1692`](../../../../../crates/skald/skald-workflow/src/workflow.rs), and [`workflow_surface.rs:848`](../../../../../crates/skald/skald-workflow/src/workflow_surface.rs) add function-scoped `use` statements. `architecture/agent-rules.md:10` requires every import to live at the top of its module and permits test imports at the top of the `#[cfg(test)] mod tests` scope, not inside individual test functions. This hides the test module's dependency surface and is a direct mandatory-style violation. Move these imports into the respective test module import blocks and deduplicate them.

- `STD-002` — [`TASK-001-explicit-local-runtime.md:465`](../../tasks/TASK-001-explicit-local-runtime.md) records the scoped verification set but omits both required lanes for surfaces changed by this candidate: `mise run docs:check` for the three `docs/` changes and `mise run check:examples` (or every touched example task) for the twenty changed example files. `AGENTS.md:499-500` makes those lanes mandatory. The recorded green lanes therefore do not prove generated-doc drift, docs commands/links/build/accessibility, or example compilation. Run and record both required lanes before accepting the task.

- `STD-003` — [`TASK-001-explicit-local-runtime.md:462`](../../tasks/TASK-001-explicit-local-runtime.md) names four Rust tests but records only package/target shorthand (`-p skald-agent --test loop_responses` and `-p skald-spec --lib`). `AGENTS.md:459-471` requires every specifically named test in a task artifact or implementation report to include and run its exact `mise exec -- cargo nextest run --locked ... -E 'test(=...)'` command. The broad `test:skald` result is useful aggregate evidence but does not satisfy the exact-selector evidence rule. Record successful exact commands for all four named tests.

### Critical

None.

### Suggestions

None.

## Authority coverage

All selected authorities were read completely. No required authority was unavailable.

| Authority | Applicability | Result and evidence |
|---|---|---|
| `AGENTS.md` | Repository ownership, Rust/PyO3/Python rules, contracts, async, testing, completion | FAIL because of `STD-001` through `STD-003`; other applicable rules passed below. |
| `architecture/agent-rules.md` | Mandatory Rust source organization and repository hygiene | FAIL: added function-scoped imports violate line 10. No added `#[allow]` or `#[ignore]` was found; `git diff --check` is clean. |
| `architecture/wyrd-design.md` | Skald/runtime ownership, client/server and contract direction | PASS: reusable local agent workflow behavior remains in Skald; wire contracts remain in `wyrd-spec`; gateway/server changes are consumers rather than duplicated runtime owners. |
| `architecture/wyrd-doctrine.mdx` | Verification vocabulary and public surface doctrine | PASS: the change keeps Workflow/Agent/Prompt/Card vocabulary and does not add a new Card kind or compatibility surface. |
| `architecture/wyrd-security-posture.md` | Outbound endpoint, secret, TLS, redirect, proxy, DNS/SSRF controls | PASS: endpoint screening is centralized in `skald-providers`; external clients disable redirects and proxies, retain TLS, and connect using screened resolver results. Secret values use `SecretString` and are not formatted into diagnostics. |
| `architecture/references/README.md` | Reference router | PASS: routed to doctrine, architecture, Rust, PyO3, Python/stubs, TypeScript, testing, errors, and agent-harness authorities. Bifrost was not applicable because this diff does not change Bifrost ingest/query/storage behavior. |
| `references/doctrine/{architecture-constraints,positioning-and-vocabulary}.md` | Boundary and naming constraints | PASS: no legacy vocabulary or language-specific durable behavior was introduced. |
| `references/architecture/patterns.md` | Dependency direction and ownership patterns | PASS: dependencies point from applications/SDKs into shared contracts and Skald owners; no reverse Vala dependency or broad platform trait was introduced. |
| `references/languages/{rust-core,implementation-execution,maintainer-style}.md` | Rust structure, async ownership, maintainability | FAIL only for `STD-001`; execution otherwise uses cohesive owners, synchronous validation, bounded async orchestration, and typed errors. |
| `references/languages/{pyo3-boundaries,python-api-and-stubs}.md` | Existing PyO3 migration boundary, exports, generated stubs, runtime-owned tests | PASS: the existing feature-gated Skald wrapper remains an approved migration boundary, Python-visible behavior is exported through `wyrd`, stubs were generator-updated, and interpreter behavior is tested in Python. Recorded evidence includes Python unit tests, typecheck, PyO3 scope, formatting, and lint lanes. |
| `references/languages/typescript-guide.md` | Generated TypeScript contract projection | PASS: `error-codes.ts` is updated with the error catalog projection; recorded `codegen:check` passed. |
| `references/languages/{errors,spec-driven-development}.md` | Stable public errors and generated schemas | PASS: public workflow errors use the derive-backed catalog, contract source lives in `wyrd-spec`, and schemas/goldens are generator outputs. Recorded `codegen:check` passed. |
| `references/languages/testing-workflows.md` | Test tier, placement, exact selectors, lane selection | FAIL: required docs/examples lanes and exact selectors are missing from recorded evidence (`STD-002`, `STD-003`). New multi-crate Agent loop coverage is properly placed in an external integration target; no test was disabled. |
| `references/languages/agent-harness.md` | Agent loop/tool execution tests | PASS: native OpenAI Responses history/tool-loop behavior has dedicated Rust integration coverage and workflow route coverage, subject to the exact-command evidence defect in `STD-003`. |
| Approved `spec.md` revision 9 and `TASK-001-explicit-local-runtime.md` | Authorized task surface and claimed evidence | Covered as context only; acceptance correctness belongs to the task and behavior reviewers. The repository review validated the implementation record rather than assuming its PASS cells. |

## Per-rule results

| Rule area | Result | Source evidence |
|---|---|---|
| Ownership and dependency direction | PASS | `skald-workflow` owns the local workflow executor and composition; `skald-providers` owns provider egress policy/client mechanics; `wyrd-spec` owns pure DTOs/errors/schema; gateway/server/SDK files consume those owners. Client-tier checks are recorded green. |
| Struct-centered Rust style | PASS | Stateful execution remains on `Workflow` and focused executor/binding owners; endpoint policy is a meaningful invariant-bearing type, not a zero-sized utility wrapper; no single-implementation trait was added merely for indirection. |
| Async/runtime discipline | PASS | Pure validation/planning remains synchronous. Execution awaits actual providers/tools, uses bounded scheduling and owned task draining, and does not create an ad hoc Tokio runtime. |
| Import placement | **FAIL** | Five changed test functions contain local imports; see `STD-001`. |
| Public errors and safe diagnostics | PASS | Stable errors remain derive-backed; remote failures retain bounded safe metadata; external secret values are represented as secrets and omitted from display/debug paths. |
| PyO3 and Python surface | PASS | Existing optional wrapper boundary is retained; native Rust behavior is wrapped rather than reimplemented; public package exports/stubs/tests change together. Recorded `py:test:unit`, `py:typecheck`, `check:pyo3-scope`, `py:format`, and `py:lints` pass. |
| Generated contracts | PASS | Contract source, schema generator, checked-in schemas/goldens, docs generator, Python stubs, and TypeScript error union move together. Recorded `codegen:check` passes. |
| Outbound HTTP/SSRF policy | PASS | One endpoint policy validates scheme/origin/address class. The custom resolver returns screened addresses used by the request client; redirects and proxy discovery are disabled, TLS verification remains enabled, and production rejects plain HTTP. |
| Test integrity and placement | PASS | No added `#[allow]`/`#[ignore]`; the cross-crate Responses loop test is correctly external; Rust-only behavior stays in Rust tests and Python lifetime behavior stays in Python tests. |
| Required verification selection | **FAIL** | The implementation record omits `docs:check`, `check:examples`, and exact commands for named Rust tests; see `STD-002` and `STD-003`. |
| Tenant/audit/persistence rules | N/A | No durable tenant-scoped write, authz decision, audit publication, or persistence ownership is added by this local runtime task. Existing gateway invocation consumers are adapted without adding another audit path. |

## Complete changed-file map (111/111)

Every path in the immutable diff was assigned to its governing rule set. `T` = task/evidence; `D` = dependency/ownership; `R` = Rust/style/async; `C` = contract/errors/codegen; `S` = security/egress; `P` = PyO3/Python/stubs; `Y` = TypeScript projection; `V` = tests/verification; `X` = docs/examples.

| # | Changed path | Rules |
|---:|---|---|
| 1 | `Cargo.lock` | D, V |
| 2 | `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md` | T, V |
| 3 | `crates/shared/wyrd-client/src/state.rs` | D, R, C |
| 4 | `crates/skald/skald-agent/src/loop_runtime.rs` | D, R, V |
| 5 | `crates/skald/skald-agent/src/request_builder.rs` | D, R, C |
| 6 | `crates/skald/skald-agent/src/session.rs` | D, R, C |
| 7 | `crates/skald/skald-agent/tests/loop_responses.rs` | V, R |
| 8 | `crates/skald/skald-observer/src/composite.rs` | D, R |
| 9 | `crates/skald/skald-observer/src/observer.rs` | D, R |
| 10 | `crates/skald/skald-observer/src/python.rs` | P, R |
| 11 | `crates/skald/skald-providers/Cargo.toml` | D, S, V |
| 12 | `crates/skald/skald-providers/src/clients/external.rs` | S, R |
| 13 | `crates/skald/skald-providers/src/clients/mod.rs` | S, D |
| 14 | `crates/skald/skald-providers/src/endpoint.rs` | S, R, V |
| 15 | `crates/skald/skald-providers/src/error.rs` | C, S, R |
| 16 | `crates/skald/skald-providers/src/lib.rs` | D, R |
| 17 | `crates/skald/skald-runtime/src/error.rs` | C, R |
| 18 | `crates/skald/skald-spec/src/lib.rs` | C, D |
| 19 | `crates/skald/skald-spec/src/message.rs` | C, R, V |
| 20 | `crates/skald/skald-spec/src/request.rs` | C, R, V |
| 21 | `crates/skald/skald-workflow/Cargo.toml` | D, P, V |
| 22 | `crates/skald/skald-workflow/README.md` | X, C |
| 23 | `crates/skald/skald-workflow/src/attempt.rs` | R, C, V |
| 24 | `crates/skald/skald-workflow/src/context.rs` | R, C, V |
| 25 | `crates/skald/skald-workflow/src/def.rs` | R, C, V |
| 26 | `crates/skald/skald-workflow/src/error.rs` | C, R |
| 27 | `crates/skald/skald-workflow/src/handoff.rs` | R, C |
| 28 | `crates/skald/skald-workflow/src/lib.rs` | D, R, P |
| 29 | `crates/skald/skald-workflow/src/output.rs` | R, C, V |
| 30 | `crates/skald/skald-workflow/src/plan.rs` | R, C, V |
| 31 | `crates/skald/skald-workflow/src/python.rs` | P, R, C |
| 32 | `crates/skald/skald-workflow/src/route.rs` | R, S, C |
| 33 | `crates/skald/skald-workflow/src/run.rs` | R, C, V |
| 34 | `crates/skald/skald-workflow/src/schedule.rs` | R, V |
| 35 | `crates/skald/skald-workflow/src/task.rs` | R, C |
| 36 | `crates/skald/skald-workflow/src/tasklist.rs` | R, C |
| 37 | `crates/skald/skald-workflow/src/test_support.rs` | V, R |
| 38 | `crates/skald/skald-workflow/src/workflow.rs` | R, S, V |
| 39 | `crates/skald/skald-workflow/src/workflow_surface.rs` | R, C, V |
| 40 | `crates/skald/skald-workflow/tests/dag.rs` | V |
| 41 | `crates/skald/skald-workflow/tests/execute_task.rs` | V |
| 42 | `crates/skald/skald-workflow/tests/handoff.rs` | V |
| 43 | `crates/skald/skald-workflow/tests/parameter_injection.rs` | V |
| 44 | `crates/skald/skald-workflow/tests/retries.rs` | V |
| 45 | `crates/skald/skald-workflow/tests/skeleton.rs` | V |
| 46 | `crates/skald/skald-workflow/tests/workflow_observers.rs` | V |
| 47 | `crates/skald/skald-workflow/tests/workflow_run.rs` | V |
| 48 | `crates/skald/skald-workflow/tests/workflow_surface.rs` | V |
| 49 | `crates/wyrd-spec/examples/gen_schemas.rs` | C, V |
| 50 | `crates/wyrd-spec/schemas/card.json` | C |
| 51 | `crates/wyrd-spec/schemas/create_workflow_run_request.json` | C |
| 52 | `crates/wyrd-spec/schemas/get_card_response.json` | C |
| 53 | `crates/wyrd-spec/schemas/workflow_run.json` | C |
| 54 | `crates/wyrd-spec/schemas/workflow_spec.json` | C |
| 55 | `crates/wyrd-spec/src/card/workflow.rs` | C, R, V |
| 56 | `crates/wyrd-spec/src/error.rs` | C, R |
| 57 | `crates/wyrd-spec/src/gateway/policy.rs` | C, S, R |
| 58 | `crates/wyrd-spec/src/ids.rs` | C, R |
| 59 | `crates/wyrd-spec/src/reference.rs` | C, R |
| 60 | `crates/wyrd-spec/src/refs/mod.rs` | C, R |
| 61 | `crates/wyrd-spec/tests/schemas/card.json` | C, V |
| 62 | `crates/wyrd-spec/tests/schemas/create_workflow_run_request.json` | C, V |
| 63 | `crates/wyrd-spec/tests/schemas/get_card_response.json` | C, V |
| 64 | `crates/wyrd-spec/tests/schemas/workflow_run.json` | C, V |
| 65 | `crates/wyrd-spec/tests/schemas/workflow_spec.json` | C, V |
| 66 | `crates/wyrd/wyrd-gateway/src/adapter/http.rs` | D, S, R |
| 67 | `crates/wyrd/wyrd-gateway/src/adapter/tests.rs` | S, V |
| 68 | `crates/wyrd/wyrd-gateway/src/lib.rs` | D, S |
| 69 | `crates/wyrd/wyrd-server/Cargo.toml` | D, V |
| 70 | `crates/wyrd/wyrd-server/src/boot/mod.rs` | D, R, S |
| 71 | `crates/wyrd/wyrd-server/src/components/gateway/pg_invocation_tests.rs` | S, V |
| 72 | `crates/wyrd/wyrd-testing/Cargo.toml` | D, V |
| 73 | `crates/wyrd/wyrd-testing/src/server.rs` | D, V, S |
| 74 | `crates/wyrd/wyrd/src/agent.rs` | D, R, C |
| 75 | `docs/scripts/generate_card_docs.py` | X, C, V |
| 76 | `docs/src/content/docs/api/errors.md` | X, C, V |
| 77 | `docs/src/content/docs/how-to/build-a-workflow.svx` | X, C, V |
| 78 | `examples/python/workflow_anthropic.py` | X, P, V |
| 79 | `examples/python/workflow_gateway.py` | X, P, S, V |
| 80 | `examples/python/workflow_gemini.py` | X, P, V |
| 81 | `examples/python/workflow_openai.py` | X, P, V |
| 82 | `examples/rust/common/mod.rs` | X, R, V |
| 83 | `examples/rust/workflow_anthropic.rs` | X, R, V |
| 84 | `examples/rust/workflow_from_builder.rs` | X, R, V |
| 85 | `examples/rust/workflow_gateway.rs` | X, R, S, V |
| 86 | `examples/rust/workflow_gemini.rs` | X, R, V |
| 87 | `examples/rust/workflow_openai.rs` | X, R, V |
| 88 | `examples/rust/workflow_parallel.rs` | X, R, V |
| 89 | `examples/rust/workflow_structured_output.rs` | X, R, V |
| 90 | `examples/rust/workflow_with_observer.rs` | X, R, V |
| 91 | `examples/rust/workflows/research.yaml` | X, C, V |
| 92 | `sdks/wyrd-sdk-python/examples/from_builder.py` | X, P, V |
| 93 | `sdks/wyrd-sdk-python/examples/parallel.py` | X, P, V |
| 94 | `sdks/wyrd-sdk-python/examples/structured_pipeline.py` | X, P, V |
| 95 | `sdks/wyrd-sdk-python/examples/tracing_jaeger.py` | X, P, V |
| 96 | `sdks/wyrd-sdk-python/examples/with_observer.py` | X, P, V |
| 97 | `sdks/wyrd-sdk-python/examples/workflows/research.yaml` | X, P, C, V |
| 98 | `sdks/wyrd-sdk-python/python/examples/workflow_structured_output.py` | X, P, V |
| 99 | `sdks/wyrd-sdk-python/python/wyrd/__init__.py` | P, C |
| 100 | `sdks/wyrd-sdk-python/python/wyrd/__init__.pyi` | P, C |
| 101 | `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.py` | P, C |
| 102 | `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi` | P, C |
| 103 | `sdks/wyrd-sdk-python/python/wyrd/observer.py` | P, C |
| 104 | `sdks/wyrd-sdk-python/python/wyrd/observer.pyi` | P, C |
| 105 | `sdks/wyrd-sdk-python/python/wyrd/stubs/agent.pyi` | P, C |
| 106 | `sdks/wyrd-sdk-python/python/wyrd/stubs/observer.pyi` | P, C |
| 107 | `sdks/wyrd-sdk-python/python/wyrd/stubs/package.pyi` | P, C |
| 108 | `sdks/wyrd-sdk-python/tests/unit/runtime/workflow/test_workflow_observers.py` | P, V |
| 109 | `sdks/wyrd-sdk-python/tests/unit/runtime/workflow/test_workflow_parameter_injection.py` | P, V |
| 110 | `sdks/wyrd-sdk-python/tests/unit/runtime/workflow/test_workflow_save_load.py` | P, V |
| 111 | `sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts` | Y, C |

## Open questions

None. The failures have direct repository-rule corrections and do not require a product decision.

## Verification notes

- Independently inspected the complete 111-file diff and ran `git diff --check` against the immutable commits; it produced no errors.
- Verified the candidate remained `eb22b03f2bb766886d839bda23aafbd4ba130ab3` before writing this report.
- Treated the implementation record at task lines 453-465 as claimed available evidence, not assumed truth. Its recorded green lanes are useful but incomplete under `AGENTS.md` for the changed docs/examples and specifically named tests.
- No source file, generated artifact, test, or implementation record was edited by this reviewer.
