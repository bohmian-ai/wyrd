# Repository standards review

Result: **FAIL**

Immutable subject: base `0739bbf38af7cc05bbfefb09850490c55432ec2d`, candidate `aaa152dd4243d57752b73d3a4e9c140bccc7d171`; first cumulative review using `git diff <base>...<candidate>`. Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.

This report covers mandatory repository standards, not task acceptance, performance, or preferred implementation shape. The two uncommitted changes in telemetry and capacity are outside the candidate and confer no acceptance credit. No source edits or Cargo commands were made by this reviewer; the orchestrator owns executable checks. CodeGraph was consulted before source searches. Commands ran through mise.

## Authority coverage

Root `AGENTS.md` and `agent_docs/index.md` apply throughout. The review scope identifies no nested AGENTS in the changed directories. The approved specification revision 5 and `changes/spec-driven-development.md` establish the internal benchmark scope and distinguish release-process evidence from new product APIs.

| Changed surface | Applicable authority | Static rule assessment |
|---|---|---|
| `Cargo.lock`, `wyrd-bench/Cargo.toml`, `wyrd-testing/Cargo.toml` | Root dependencies/build rules; `agent_docs/rust.md` Build and features | PASS: TOML moves from the retired shared SLO owner to benchmark owner; existing workspace dependency reused; Tokio signal supports interruption. No wildcard, per-crate profile, new client-tier heavy dependency or Python feature. |
| `wyrd-bench/src/{lib,slo}.rs`, `benches/thresholds.toml`; testing `src/load/*`, `src/lib.rs`, retired `bin/bifrost_forge_capacity/*`, retired per-engine report/envelope files | Root reuse/removal, vocabulary, tests; maintainer guide; revision-5 retirement | PASS for standards: obsolete exports/tasks and Linux resource-envelope machinery removed together. Replacement workload completeness belongs to behavior review. |
| `wyrd-telemetry/src/lib.rs` | Root structured tracing and rustdoc; `agent_docs/rust.md`; `wyrd-design.md` Trace context | FAIL for documentation below. Helper uses established tracing pipeline, fixed span name and bounded labels; no payload/SQL/tenant/credential field added. Candidate u64 export correctness belongs to invariant review. |
| Bifrost `gate/mod.rs`, `oracle/{exec,mod}.rs` | `architecture/bifrost-design.md` System boundary, Resource and failure invariants, Telemetry/Measurement meanings; root Rust/re-exports/testing | FAIL for changed-test rustdoc below. Row counter uses bounded `class`; stats remain server-owned and optional. Added timing does not remove permission/audit/durability work. |
| Gateway `engine.rs`; server `components/gateway/telemetry.rs`, `verification/telemetry.rs` | Root Rust/docs; `agent_docs/rust.md`; security Audit integrity/privacy | PASS static: existing timing owners retained; duration fields and closed labels; pure interval union helper and inline test. Existing permission/audit paths and SDK boundaries unchanged by these diffs. |
| Server `grpc/otlp.rs`, `query/routes.rs` | Root handlers; `agent_docs/{rust,errors,agent-surfaces}.md`; Bifrost public edge/terminal contract; security Tenant isolation/Audit integrity | PASS static: typed request/response and existing error/authentication flow retained; optional arrival extension is internal. No new SQL/pool/route/compatibility route/generated type. Timing correctness belongs to invariant review. |
| Testing `capacity.rs`, `experiment.rs` | Root Rust/errors/docs/tests; `agent_docs/{rust,errors,agent-surfaces}.md`; specification internal evidence boundary | FAIL for shared workload docs below. Typed records and `thiserror`; no durable behavior moved to spec/SDK. Internal experiment errors are not new HTTP/MCP/SDK errors. Tests inline. |
| `release_server.rs`, `release_server/os.rs`, `profile.rs` | Root portability/resource restrictions/secrets/tests; Rust guide; Bifrost Resource invariants; security Secret handling | PASS static: plain children, sysinfo, optional external samply; no added OS cfg, /proc, cgroup, systemd scope, credential switching or production runtime creation. SetupTenant keys remain SecretString. Lifecycle proof belongs to system review. |
| Capacity `main.rs`, `levels.rs`, `flags.rs`; `client_queue/{mod,step}.rs` | Root agent surfaces, Rust/docs/tests; `agent_docs/agent-surfaces.md`; spec-driven workflow | PASS static ownership/tiering/placement: shared experiment/collector and production client owners. Paused Tokio runtime construction found in binary tests, not library code. |
| Capacity `query/{mod,run,isolated,competing,telemetry,workload}.rs` | Root Rust/Bifrost client path/fixture cleanup/secrets/tests/docs; Bifrost authority; security tenant binding | FAIL for isolated-workload docs below. Public reads/writes use client owner; operator compaction setup uses TenantTableBinding and existing catalog transaction, not raw SQL pool. |
| Capacity `verification/{mod,run}.rs`, `gateway/{mod,run,capacity,workload}.rs` | Root Rust/production paths/tests/docs; `agent_docs/rust.md`; security tenant/audit/secret boundaries | PASS static boundary: existing authentication/client/local fixture owners retained; fresh trace identity supplements real requests. No new SDK/PyO3/tenant-bypass API. |
| `mise.toml`, `TESTING.md`, `benchmarks/README.md`, three `benchmarks/*/baselines.toml` | Root commands/discoverability/no benchmark CI; agent surfaces; specification evidence format | PASS standards: discoverable mise tasks with release-build dependency, no gate/CI addition, old tasks removed. README covers commands/units/record schema/verdicts/history. Baselines are authored configuration, not generated contracts; eligibility accuracy is behavior review scope. |

## Mandatory rules

| Rule | Evidence | Result |
|---|---|---|
| Portable harness, no resource envelopes | Removed scope/Forge binary; os.rs sysinfo, profile.rs external samply | PASS static, no three-platform execution claimed |
| Durable contracts/behavior stay in owner tier | Production changes add timing at existing owners; no new SDK/spec contract | PASS |
| Tenant-bound SQL; no raw pool escape | No new PgPool field/signature in diff; competing.rs:740–789 uses TenantTableBinding | PASS for changed signatures |
| No auth/audit/durability weakening for speed | Gate recording, OTLP future wrapping, query guard and emission diffs retain checks | PASS for changed blocks |
| Secrets retained in redacted wrappers | SetupTenant key is SecretString; Invocation redacts sensitive config names | PASS static on inspected paths; complete record privacy is behavior scope |
| No allow/ignore, weakened skip or library runtime | Added-line inspection; runtime construction only in binary tests | PASS |
| Inline tests with local fixtures | Source tests, local OTLP collector, inline engine tests | PASS placement |
| Changed items document Errors/Panics and cancellation/partial effects | REP-001 | FAIL |
| Rust lints | Orchestrator `verification.md`: PASS | PASS reported |
| Format | Orchestrator reports fmt:check failure confined to preexisting uncommitted repair test | No candidate formatting finding retained |
| Focused executable proof | Orchestrator reports nine shared experiment tests PASS; exact-export repair test PASS | Recorded, does not prove candidate end-to-end recording |

## REP-001 — Required lifecycle/error and test-panic rustdoc is incomplete

Classification: **VIOLATION**, explicit mandatory documentation gate.

Authority: root AGENTS Code requires rustdoc for changed Rust items, `# Errors` on fallible items and `# Panics` when an item can panic; missing rustdoc blocks merge. `agent_docs/rust.md` Rustdoc explicitly includes private helpers/tests and requires async/durable operations to document cancellation, partial progress, idempotency and retry.

Exact candidate locations:

- `crates/wyrd/wyrd-testing/src/experiment.rs:1342–1357`: new `Workload::prepare`, `measure`, `finish` return Result, but have only one-line summaries without Errors sections. Trait overview describes order/equivalent state, without error/cancellation/partial-progress responsibilities.
- `crates/wyrd/wyrd-testing/src/bin/capacity/query/isolated.rs:232–235`, `313–324`: the new `Isolated` implementations omit Errors sections. Prepare can fail after collector/server start, data registration/loading and before publication or sender setup completes. Finish propagates server stop/log preservation failure.
- `crates/shared/wyrd-telemetry/src/lib.rs:560–563` (candidate coordinates before uncommitted repair), new `timing_spans_carry_exact_nanoseconds`: assertion/expect-bearing test lacks Panics section.
- `crates/vala/vala-bifrost-redux/src/oracle/exec.rs:2087–2090`, new `scan_stats_sum_local_output_rows`: assertion-bearing test lacks Panics section.

The full trait and Isolated implementation were read. Prepare propagates collector, server, client, registration/loading, publication-settlement and OTLP errors; it can also explicitly return wrong-load errors. Finish propagates LocalServer::stop. Tests contain asserts/expects. The gap exists in committed candidate and is not repaired by the two uncommitted data-type/test changes.

Observable consequence/materiality: explicit mandatory merge gate fails. Maintainers implementing the shared extension point also cannot find its error/cancellation obligations despite cleanup being a key workflow responsibility. This does not assert an additional cleanup implementation defect.

Smallest testable correction: supply accurate Errors and cancellation/partial-progress documentation on the three shared Workload methods and the three cited Isolated implementations; supply Panics sections on the two cited new tests. Follow existing LocalServer::start cancellation docs and existing benchmark test Panics sections. Batch these eight bounded item corrections with functional remediation; do not introduce code, a checker, abstraction, or a style-only round. Document actual behavior rather than promising a new behavior.

Closure proof: inspect these rustdoc blocks against their method bodies and applicable gate, then run the write-set’s existing format/lint lanes. No benchmark execution is needed solely for this correction.

## Limits and calibration

No other material repository-rule finding proposed. Function-scoped imports and fully qualified type spellings in changed code are not separately retained without a material consequence or explicit mandatory merge gate. The small CPU-accounting arithmetic loop predates the candidate; it does not establish a newly introduced synthetic-host-load validity test.

This standards report does not certify samples, runtime budgets, baseline derivation, platform execution or full task acceptance. Those require the other selected reports and orchestrator evidence. Source inspection establishes the mandatory documentation gap independently of those outcomes.
