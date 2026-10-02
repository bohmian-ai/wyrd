# Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 11 at `9a621a28a40b67e82e4ba119f7df577dde893f1e`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Prior review and remediation: `changes/active/skald-workflow-runtime/review/TASK-001-r2/`, `TASK-001-R1-close-validated-runtime-gaps.md`, and `TASK-001-R1-addendum-revision-11.md`

The candidate remained at the stated commit throughout this review. No
`.codegraph/` directory exists, so navigation used Git, `rg`, and direct source
inspection.

Overall result: **FAIL**

## Review Findings

### Critical

None.

### Important

- **STD-R3-001 — The replacement Agent spans do not conform to the OpenTelemetry GenAI attribute contract.** `crates/skald/skald-agent/src/loop_runtime.rs:303,602,617,795-801` records `gen_ai.response.finish_reasons` as one scalar string, although the GenAI semantic convention defines it as `string[]`, and maps Google and Vertex to the non-canonical provider values `google` and `vertex` rather than the applicable well-known values `gcp.gen_ai` and `gcp.vertex_ai`. `architecture/references/domain/telemetry-observations.md:44-49` requires standard GenAI attributes to follow the OpenTelemetry semantic conventions, and REQ-053 requires the replacement Agent spans to use those conventions where they exist. The current test at `crates/skald/skald-agent/tests/agent_timeout.rs:198-219` exercises only OpenAI and never asserts the finish-reason attribute, so the mismatch is not detected. Downstream collectors querying the standard typed fields will receive an invalid shape or split Google/Vertex traffic into non-standard provider buckets. Use the canonical provider values. Since the plain `tracing` bridge does not expose an OTLP string-array field here and REQ-053 does not require a finish-reason attribute, delete `gen_ai.response.finish_reasons` rather than emitting the wrong type; add the smallest focused assertion covering all built-in provider mappings and the resulting exported Agent attributes. Primary grounding: [OpenTelemetry GenAI attribute registry](https://opentelemetry.io/docs/specs/semconv/registry/attributes/gen-ai/).

- **STD-R3-002 — The rewritten Agent timeout/telemetry test leaves materially new Rust items undocumented.** `crates/skald/skald-agent/tests/agent_timeout.rs:248-379,417-443` introduces the replacement test fixtures `RecordingJournal`, `RecordingProvider`, `FixedTool`, `ControlledTool`, and `SlowTool`, including their fields, constructors, helpers, and trait methods, without rustdoc. This file replaces the removed observer timeout suite and materially changes the fixtures to prove result, journal, and tracing behavior, so they are within the touched surface rather than unrelated legacy code. `AGENTS.md:716-726` and `architecture/agent-rules.md:35` require substantive rustdoc for every new or materially modified Rust item, explicitly including private fields, methods, helpers, and tests; fallible functions also require `# Errors`, with async cancellation and side effects documented where relevant. The missing contract makes the timing controls, poison recovery, scripted response exhaustion, and tool blocking behavior implicit in implementation details, and is a hard pre-merge repository-rule failure even though lints pass. Document these existing fixtures in place, including fields and trait methods; add `# Errors` to fallible methods and describe the controlled tools' notification/cancellation behavior. Do not add an abstraction or split the test.

### Suggestions

None. Both findings are direct repository-authority violations, not optional
cleanup.

## Authority coverage

The complete 211-path cumulative diff was grouped by ownership surface. Every
changed language and layer is covered below.

| Changed surface | Applicable authority | Result and evidence |
|---|---|---|
| Approved spec, TASK-001, prior review/remediation, task-index updates | `AGENTS.md` §§14-16; spec-driven development and implementation evidence rules | **PASS.** Revision 11 is approved, TASK-001 names it, the R1 remediation and addendum are preserved, and the evidence maps prior findings and REQ-053 to source and commands. The explicit cumulative diff check passes. |
| Workspace/root manifests and `Cargo.lock` | Dependency-cost ownership; Cargo-feature policy; REQ-053 deletion | **PASS.** `skald-observer` and its edges are removed; no replacement crate or third-party dependency was added. `wyrd-telemetry` is an existing dev dependency used only for capture proof. |
| Architecture references and `docs/architecture/skald.md` | `wyrd-design`, doctrine, architecture constraints/patterns, Rust/PyO3/Python references, REQ-053 | **PASS.** Skald ownership now describes payload-free `tracing`; Python package and PyO3 inventories no longer claim the deleted Observer surface; unrelated `wyrd.observe` remains intact. |
| `wyrd-spec` Workflow contracts, validation, IDs, errors, and generated schemas | `AGENTS.md` §§2-4, 8-9; `wyrd-design`; Rust/errors/agent-harness references | **PASS.** Contracts remain typed, synchronous, IO-free, async-free, and PyO3-free; public errors use the derive-backed catalog; schema source and generated copies move together. |
| `wyrd-client` consumer adjustment | Client-tier ownership and dependency direction | **PASS.** The change adapts a consumer only and adds no client-owned durable behavior or forbidden data/server dependency. |
| `skald-agent` runtime and Agent tests | Skald ownership; Rust/async/documentation/testing rules; telemetry reference; REQ-053 | **FAIL.** Span payload safety, parentage, terminal error recording, and timeout behavior are otherwise covered, but the GenAI attribute contract is wrong (`STD-R3-001`) and the replacement external test has undocumented new items (`STD-R3-002`). |
| Deleted `skald-observer` crate and all direct Agent/Workflow hooks | Revision 11 REQ-053; ownership/dependency rules | **PASS.** The crate, workspace member, dependencies, scopes, callbacks, and compatibility surfaces are deleted without an alias. |
| `skald-spec` provider-native Responses wire contract | Skald provider-wire ownership; Rust documentation; schema/serde parity | **PASS.** Reasoning identity, typed summary parts, and encrypted state stay in the native Responses owner and are documented; round-trip evidence is recorded. |
| `skald-providers` endpoint policy and external client | Provider ownership; SSRF/secret rules; Rust errors | **PASS.** The shared resolve-screen-pin owner remains single, proxies/redirects stay disabled, refusal bodies are withheld at the external-gateway boundary, and native-provider error behavior is preserved. |
| `skald-workflow` plan, route, attempt, run, executor, authoring surface, and tests | Struct-centered Rust; sync/async boundaries; bounded ownership; external network safety; testing | **PASS.** Cohesive concrete owners hold planning, route state, scheduling, and run accounting; pure validation remains synchronous; task ownership is bounded; checked deadlines, retry limits, result ceilings, reserved bound-header rejection, and payload-free Workflow spans have direct tests. |
| `sdks/wyrd-sdk-python` Rust boundary, Python package, generated stubs, tests, and examples | PyO3 boundary; Python SDK ownership; Python API/stub rules; runtime-owned tests | **PASS.** New Workflow PyO3 code and registration live in the SDK, the owner crate retains only the orphan-rule conversion, public `TypedDict` results are precise, observer exports are removed, public imports/tests align, and recorded codegen/typecheck lanes pass. |
| TypeScript error-code projection | TypeScript/error catalog reference | **PASS.** The generated union follows the derive-backed Wyrd error catalog; no unapproved TypeScript Workflow runtime surface is added. |
| Wyrd gateway/server/testing/umbrella and Vala consumers | Application/server ownership; cross-tier direction; audit/security boundaries | **PASS.** These are direct consumer repairs only. No second runtime, public route, tenant source, policy decision, audit path, persistent state, or Vala-to-Skald reverse dependency is introduced. |
| Public docs, generated docs, changelog, and Rust/Python/YAML examples | Public contract parity; documentation/example gates; REQ-053 deletion | **PASS.** Observer-specific examples/docs are removed, remaining Workflow authoring and derived-step-ID guidance matches the current API, generated error/schema docs are aligned, and `docs:check` plus `check:examples` are recorded green. |
| `mise.toml` and repository checks/scripts | Gate integrity; adding/retiring-check rules | **PASS.** The deleted check clauses protected only the now-unreachable Observer error-map/crate boundary; live client-tier and PyO3 invariants remain. The example task now builds the actual example package, and Workflow typing is added to the existing typecheck lane. |

## Per-rule results

| Rule area | Result | Source evidence |
|---|---|---|
| Wyrd/Skald/Vala ownership and dependency direction | PASS | Runtime/provider behavior remains in Skald; pure contracts stay in `wyrd-spec`; Vala and Wyrd crates are consumers only. |
| Struct-centered Rust and abstraction discipline | PASS | `WorkflowExecutor`, `StepTask`, `RunLedger`, `ExecutionPlan`, route owners, and provider clients own cohesive state; no replacement observer trait, factory, or utility object was added. |
| Sync/async runtime discipline | PASS | Validation/planning/conversion helpers are synchronous; async functions await provider, tool, journal, session, timer, or task-set operations; no ad hoc runtime was added. |
| Rust import placement | PASS | Prior function-scoped test imports were hoisted; no changed function contains a `use` declaration. |
| Rust documentation | **FAIL** | `STD-R3-002`; production replacement items are documented, but the rewritten timeout/telemetry fixtures are not. |
| PyO3 placement, GIL/runtime behavior, and SDK aggregation | PASS | Workflow wrappers moved to `sdks/wyrd-sdk-python/src/workflow.rs`; blocking execution uses the shared runtime under `py.detach`; no `Bound` crosses an await. |
| Python exports, exact typing, stubs, and tests | PASS | Exact Workflow run dictionaries are exported; Observer/Otel exports and `_init` are removed; generated declarations and public-runtime tests align. |
| Stable public errors and language projections | PASS | Derive-backed `WyrdError` remains authoritative; Rust/Python/TypeScript/docs projections move together; external refusal details are sanitized. |
| Generated artifacts | PASS with recorded evidence | Schema, stub, and docs generators were updated; `codegen:check` is recorded green. No generated artifact is established as hand-edited. |
| Telemetry semantic conventions and payload safety | **FAIL** | `STD-R3-001`; payload exclusion, span ownership, error codes, and parent/child relationships otherwise pass source inspection and focused tests. |
| Secret handling and external network safety | PASS | Bound headers reject routing/framing/internal names, external refusal bodies cannot reflect secrets, DNS is screened and pinned, and TLS/no-proxy/no-redirect/body bounds remain. |
| Test location and runtime ownership | PASS | The external Agent test wires multiple Skald crates plus the production-shaped telemetry pipeline; Python lifetime behavior stays in Python tests. |
| Required verification and exact selectors | PASS | The task records all 15 exact named nextest selectors plus the scoped Rust, Python, codegen, boundary, docs, and example lanes. |
| Gate integrity and check retirement | PASS | No new `#[allow]`/`#[ignore]` circumvention was found; removed Observer-specific checks protect no reachable owner after deletion. |
| Public documentation accuracy | PASS | Derived step IDs, tracing behavior, and removed Observer APIs are accurately described; no compatibility alias or migration surface is introduced. |
| Tenant SQL, durable persistence, and canonical audit rules | N/A | This cumulative task adds no SQL transaction, tenant-persistent run state, authorization decision, or audit publisher. Existing gateway invocation behavior is only adapted as a consumer. |
| Bifrost storage/query architecture | N/A | No Bifrost ingest, query, storage, maintenance, or analytical reliability behavior changes. |

## Prior-finding closure

| Prior finding | Candidate evidence | Standards result |
|---|---|---|
| FIND-TASK-001-1 exact run budget | Exact JCS charging and `terminal_budget_reserve` | **CLOSED** |
| FIND-TASK-001-2 Responses reasoning replay | Typed reasoning item identity/summary plus exact Agent/spec tests | **CLOSED** |
| FIND-TASK-001-5 result ceiling before observation | Observer path deleted; `AttemptOutcome::from_agent` enforces the ceiling before span-only telemetry | **CLOSED** |
| FIND-TASK-001-6 callback liveness | Callback system deleted; synchronous `tracing` cannot run user callbacks or change outcomes | **CLOSED** |
| FIND-TASK-001-7 import placement | Imports are in module import blocks | **CLOSED** |
| FIND-TASK-001-8 docs/examples proof | Both required lanes are recorded green | **CLOSED** |
| FIND-TASK-001-9 exact named tests | Fifteen exact selectors are recorded | **CLOSED** |
| FIND-TASK-001-10 Python owner | Workflow PyO3 surface resides in the Python SDK | **CLOSED** |
| FIND-TASK-001-11 exact Python declarations | `WorkflowRunError`, `WorkflowStepResult`, and `WorkflowRunDict` are precise `TypedDict`s | **CLOSED** |
| FIND-TASK-001-12 Observer documentation | Observer hooks are deleted; no surviving hook requires documentation | **CLOSED** |
| FIND-TASK-001-13 derived step-ID guide | Guide and builder test describe the implemented transformation | **CLOSED** |
| FIND-TASK-001-14 pre-poll cancellation | Settlement leaves zero-attempt aborted work for `RunLedger::finish` to mark unstarted | **CLOSED** |
| FIND-TASK-001-15 checked deadlines | Authored and local deadlines use checked construction with focused proof | **CLOSED** |
| FIND-TASK-001-16 retry bound | Pure validation rejects only `u32::MAX` | **CLOSED** |
| FIND-TASK-001-17 reserved bound headers | Shared classifier rejects transport/routing/internal names at insertion | **CLOSED** |
| FIND-TASK-001-18 reflected credentials | External-gateway refusal body is replaced with a fixed safe diagnostic | **CLOSED** |
| FIND-TASK-001-19 cumulative diff check | Explicit base-to-candidate `git diff --check` exits zero | **CLOSED** |

REQ-053 itself is not standards-complete because the replacement Agent tracing
retains `STD-R3-001` and its replacement timeout proof retains `STD-R3-002`.

## Open Questions

None. Both corrections are bounded by existing authority and require no product,
public API, architecture, security, compatibility, concurrency, or persistent
data decision.

## Verification Notes

- Confirmed `HEAD` was `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`
  before and after source inspection.
- Ran
  `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`;
  it exited zero.
- Source inspection confirmed the prior security, PyO3 ownership, typing,
  generated-surface, Observer-deletion, and exact-selector corrections.
- The candidate records these green lanes: `mise run fmt`, `lints`,
  `py:format`, `py:lints`, `codegen:check`, `check:client-tier`,
  `check:pyo3-scope`, `check:unwrap-audit`, `test:skald`, `py:test:unit`,
  `py:typecheck`, `docs:check`, and `check:examples`, plus the 15 exact named
  nextest commands, error coverage, Wyrd/Vala consumer tests, and the explicit
  cumulative diff check. Those results are available evidence; they do not
  enforce semantic-convention value/type correctness or the repository's
  private-item rustdoc rule, which is why `STD-R3-001/002` remain.
