# Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `a704a8890ef20efe65fee1e116f7d02288f8ec5c`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`,
  Revision 11
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation task:
  `changes/active/skald-workflow-runtime/review/TASK-001-r3/TASK-001-R2-close-round-three-runtime-gaps.md`
- Additional reviewed harness fix: `516d0fbcce4e3051348293d7c645cae91f3e1e42`

The candidate remained at the stated commit throughout this review. There is no
`.codegraph/` directory, so navigation used Git, `rg`, and direct source and
caller inspection.

Overall result: **FAIL**

## Review Findings

### Critical

None.

### Important

- **STD-R4-001 — The changed Skald architecture document still contradicts
  the repository's actual dependency and PyO3 boundaries.**
  `docs/architecture/skald.md:3-5` says Skald depends only on neutral/Skald
  crates and no `wyrd-*` crate, while the newly rewritten Workflow entry at
  `docs/architecture/skald.md:28-38` correctly says `skald-workflow` depends on
  `wyrd-spec`; `docs/architecture/skald.md:44-46` repeats that no Skald engine
  crate depends on `wyrd-spec`, and the dependency diagram at lines 48-58
  omits that live edge. The same changed document says only `skald-prompt` and
  `wyrd-cards` opt into Python and that `skald-agent`/`skald-workflow` are
  source-level PyO3-free (`docs/architecture/skald.md:77-87`), although
  `crates/skald/skald-agent/Cargo.toml:15-20,43`,
  `crates/skald/skald-workflow/Cargo.toml:15-20,47`, and
  `crates/skald/skald-agent/src/python.rs:10-11` retain the approved optional
  owner-crate Python boundary. `AGENTS.md` sections 2, 3, 7, and 8 and
  `architecture/references/doctrine/architecture-constraints.md` require the
  locked Wyrd foundation edges and retained optional Python features to be
  described accurately; changed permanent documentation is part of contract
  correctness. This is also the document selected to close
  `FIND-TASK-001-27`, so the contradiction is inside the candidate's
  remediation surface rather than unrelated documentation debt. A maintainer
  following it cannot tell whether the current manifests are sanctioned or
  architecture violations. Reconcile the introduction, dependency-direction
  prose/diagram, and PyO3 section with the existing rule: Skald remains
  independent of Vala and server-tier crates, while named foundation edges
  such as `wyrd-spec` are locked and existing owner crates may retain optional
  Python features enabled only by `wyrd-sdk-python`; new or materially moved
  wrappers belong in the SDK. Do not add another rule or compatibility
  explanation. Verify with `mise run docs:check` and direct comparison against
  the cited manifests and `check:client-tier`/`check:pyo3-scope` policy.

### Suggestions

None.

## Authority coverage

| Changed surface | Applicable authority | Result and evidence |
|---|---|---|
| Approved Revision 11, TASK-001, prior verdicts, R2 remediation, and implementation evidence | `AGENTS.md` sections 14-16; `languages/spec-driven-development.md`; `languages/testing-workflows.md` | **PASS.** The task and remediation retain immutable identities, mapped findings, focused commands, broader lanes, and the additional harness diagnosis. |
| Workspace manifests, lockfile, crate features, and dependency removals | `AGENTS.md` sections 2-4; `architecture/agent-rules.md`; `doctrine/architecture-constraints.md`; `architecture/patterns.md`; `languages/rust-core.md` | **PASS.** `skald-observer` is deleted without a replacement dependency; `futures-util` was already workspace-installed and is used at the Workflow owner; `anyhow` is removed from `skald-workflow`; feature changes remove only the deleted Observer edge. |
| `wyrd-spec` Workflow contracts, validation, IDs, public errors, schema generation, and generated JSON | `AGENTS.md` sections 2-4 and 8-9; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `doctrine/positioning-and-vocabulary.md`; `languages/errors.md`; `languages/agent-harness.md` | **PASS.** Contracts remain synchronous, IO-free, PyO3-free, typed, schema-derived, and catalog-backed; generated schema copies move with their sources and recorded `codegen:check` is green. |
| `skald-agent` runtime, request/session handling, tracing, and tests | Skald ownership; struct-centered Rust; async, error, documentation, testing, and telemetry rules | **PASS.** Stateful orchestration is on `Agent`; pure helpers remain synchronous; async methods await provider/tool/journal/session IO; required rustdoc was added; spans use canonical provider/model fields and omit payloads and the unsupported scalar finish-reason field; focused tests cover OpenAI, Gemini, and Vertex. |
| Deleted `skald-observer` crate, hooks, exports, tests, examples, and checks | Revision 11 REQ-053; dependency ownership; check-retirement rules | **PASS.** The owner and all reachable compatibility surfaces are removed. Observer-only check clauses protected no live owner; client-tier, PyO3, stable-error, and telemetry boundaries remain enforced elsewhere. |
| `skald-spec` provider-native Responses wire changes | Skald provider-wire ownership; Rust/API/documentation rules | **PASS.** Native reasoning identity and summary state stay in the provider wire owner with typed round-trip proof; no Wyrd or Python contract is introduced into `skald-spec`. |
| `skald-providers` endpoint policy and external-gateway client | Provider ownership; external URL safety; secret handling; Rust errors | **PASS.** The existing resolve-screen-pin transport remains the egress owner; redirects and proxies stay disabled; secret headers are sensitive; refusal/decode details are withheld; retained decoded success fields are checked at the client boundary with focused clean/reflection controls. |
| `skald-workflow` plan, route, attempt, executor, run ledger, authoring API, and tests | Struct-centered Rust; synchronous validation; bounded async ownership; stable errors; testing | **PASS.** Concrete owners retain shared state; the single bounded task set and deadline select remain explicit; attempt panic conversion occurs while the span owner is live; the fixed Agent deadline has direct paused-time proof; the library uses typed/standard errors rather than `anyhow`. |
| Python SDK Rust wrapper, Python package exports, generated stubs, tests, and examples | `AGENTS.md` sections 7-8; `languages/pyo3-boundaries.md`; `languages/python-api-and-stubs.md`; runtime-owned test rules | **PASS.** Materially moved Workflow PyO3 code lives and registers in `wyrd-sdk-python`; the owner crate retains only its orphan-rule conversion; public imports and precise `TypedDict` stubs align; no `Bound` crosses an await; local blocking execution uses the shared runtime under `py.detach`. |
| TypeScript generated error-code projection | `languages/typescript-guide.md`; `languages/errors.md` | **PASS.** The changed union projects the derive-backed catalog and adds no TypeScript-only runtime or error semantics. |
| Wyrd client, gateway, server, umbrella, Vala consumers, and gateway policy | Ownership/dependency direction; server/contract rules; audit and tenancy rules | **PASS.** Changes are consumer adaptations to typed contracts. No new route, durable owner, authorization decision, audit path, tenant source, SQL transaction, or Vala-to-Skald reverse dependency enters TASK-001. |
| `wyrd-testing` teardown fix in `516d0fbcc` | Test-harness ownership; root-cause/gate rules; Rust documentation; async lifecycle rules | **PASS.** The recorded trace diagnoses a real in-process harness lifecycle defect. `WyrdTestServer::shutdown` and `Drop` cancel the composed state token, and `WyrdTestServerInner::fixture` is declared after runtime/state owners so the forced database drop occurs last. The moved field and `Drop` behavior are documented; no production API, retry, sleep, ignored test, or weakened assertion was introduced. The full `test:wyrd` rerun is recorded green (2,282 passed, 161 skipped). |
| Repository scripts and `mise.toml` | Gate integrity; adding/retiring checks; canonical verification | **PASS.** Removed clauses name only the deleted Observer crate/error map; live boundary checks are not broadened. `check:examples` now builds the actual example package, and the existing Python typecheck lane includes the changed Workflow surface. |
| Public architecture/docs/changelog/generated docs and examples | Documentation correctness; public contract parity; docs/example gates | **FAIL.** Deleted Observer names survive only in historical/removal context and examples build, but the materially changed Skald architecture document contradicts current dependency and PyO3 boundaries (`STD-R4-001`). |

## Per-rule results

| Rule area | Result | Evidence |
|---|---|---|
| Wyrd/Skald/Vala ownership and locked dependency direction | **FAIL (documentation only)** | Runtime source and manifests comply; `docs/architecture/skald.md` contradicts them (`STD-R4-001`). |
| Struct-centered Rust and abstraction discipline | PASS | `Agent`, `WorkflowExecutor`, `StepTask`, `ExecutionPlan`, `RunLedger`, route owners, and provider clients own cohesive behavior; no replacement Observer abstraction was added. |
| Sync/async and cancellation discipline | PASS | Pure validation/planning is synchronous; async functions await real IO/task/timer work; Workflow deadlines own cancellation; no ad hoc runtime was added. |
| Rust imports, errors, unwrap/expect policy, and lint suppressions | PASS | Imports are module-scoped; direct library `anyhow` is gone; changed production paths add no `unwrap`, unjustified `expect`, `#[allow]`, or `#[ignore]`. Recorded lint and unwrap lanes are green. |
| Rust documentation | PASS | R2 adds intent, error, timeout/cancellation, side-effect, fixture, field, and test rustdoc to the materially changed Agent/Workflow items; the harness field and `Drop` change are documented. |
| PyO3 placement, GIL behavior, registration, exports, stubs, and Python tests | **FAIL (documentation only)** | Code follows the approved boundary and recorded codegen/typecheck/tests are green; `docs/architecture/skald.md` states a different boundary (`STD-R4-001`). |
| Stable errors and Rust/Python/TypeScript/docs projection | PASS | Derive-backed `WyrdError` remains authoritative and generated projections align; external secret-reflection refusal uses the existing provider error boundary. |
| Generated artifacts | PASS with recorded evidence | Schema/stub/docs sources and generated outputs move together; `mise run codegen:check` is recorded green. |
| Telemetry conventions and payload safety | PASS | Canonical GenAI provider/model attributes and stable failure fields are covered; span/event payload marker checks remain; Observer callbacks are absent. |
| External network and secret safety | PASS | Shared endpoint policy owns screening and pinning; bound credentials are redacted and reflected successes fail closed at the external client. |
| Test taxonomy, location, runtime ownership, and exact selectors | PASS | Rust/Python lifetime ownership is respected; external tests earn their placement; named remediation tests have exact nextest selectors; scoped family, Python, codegen, docs, examples, and boundary lanes are recorded. |
| Gate integrity and check retirement | PASS | No gate was disabled or broadened to hide a live violation; Observer-only clauses were removed with their deleted owner. |
| Tenant SQL, audit durability, Bifrost storage/query behavior | N/A | No SQL tenancy, authorization verdict, audit publisher, Bifrost data contract, or persistent analytical behavior is changed by TASK-001 or the harness teardown repair. |

## Prior-finding closure

| Prior finding | Standards result |
|---|---|
| `FIND-TASK-001-20` fixed Agent deadline | **CLOSED.** The existing absolute deadline participates in the owner select with the required precedence and focused settlement proof. |
| `FIND-TASK-001-21` panic/span disagreement | **CLOSED.** Panic conversion now occurs inside `StepTask` while the attempt span is live, with failure/cancellation controls. |
| `FIND-TASK-001-22` successful credential reflection | **CLOSED.** The external client checks retained decoded success content and returns a fixed safe refusal. |
| `FIND-TASK-001-23` GenAI semantic fields | **CLOSED.** Canonical provider/model values are emitted and scalar finish reasons are absent with cross-provider capture proof. |
| `FIND-TASK-001-24` required rustdoc | **CLOSED.** The cited production and test items carry substantive intent/error/lifecycle documentation. |
| `FIND-TASK-001-25` Agent owner shape | **CLOSED.** Runtime orchestration is inherent on `Agent`. |
| `FIND-TASK-001-26` direct library `anyhow` | **CLOSED.** The resolver uses `std::io::Error`; the direct dependency is removed. |
| `FIND-TASK-001-27` permanent documentation | **NOT CLOSED.** Deleted names and absent examples were removed, but the changed Skald architecture document remains internally inconsistent (`STD-R4-001`). |

## Open Questions

None. The finding is resolved by existing repository authority and current
manifests; it requires no product, public API, architecture, security,
compatibility, concurrency, or persistent-data decision.

## Verification Notes

- Confirmed `HEAD` was
  `a704a8890ef20efe65fee1e116f7d02288f8ec5c` before writing this report.
- Ran
  `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721 a704a8890ef20efe65fee1e116f7d02288f8ec5c`;
  it exited zero.
- Reviewed the complete 223-path cumulative diff by owning surface, then the
  R2 remediation delta and `516d0fbcc` lifecycle fix in detail.
- The candidate records green results for the exact remediation tests,
  `mise run fmt`, `mise run lints`, `mise run test:skald`,
  `mise run test:shared`, `mise run test:wyrd`, `mise run py:test:unit`,
  `mise run py:typecheck`, `mise run codegen:check`,
  `mise run check:client-tier`, `mise run check:pyo3-scope`,
  `mise run check:unwrap-audit`, `mise run docs:check`, and
  `mise run check:examples`. These establish executable and generated-surface
  coverage but do not detect the semantic contradiction in the architecture
  prose.
