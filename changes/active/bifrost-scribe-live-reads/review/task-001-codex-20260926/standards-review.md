# Repository standards review: TASK-001

**Subject:** `d1ec13200d332745af2fed8069a21d5b5c39cb47..f9115fbbf6b6f116cf5ec5fe5582a9543107955a` in `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`. HEAD matched the candidate during review. No `.codegraph/` index exists. This review assesses repository standards, not task acceptance.

## Authority coverage

| Changed surface | Applicable authority inspected | Result |
|---|---|---|
| `wyrd-spec` request, terminals, JSON schemas, tonic protocol/conversion | `AGENTS.md` §§2–4, 8–9, 16; `agent-rules.md` generated-artifact and import rules; `wyrd-design.md` public contract; `bifrost-design.md` query/public surface; `references/languages/errors.md`, `agent-harness.md` | PASS except STAND-002, STAND-003 |
| Vala Oracle plan, dispatcher, live leaf, Scribe source and staging | `AGENTS.md` §§3–6, 9–12, 16; `agent-rules.md` Rust structure, async, imports, documentation, tests; `bifrost-design.md` query/resource/failure invariants; `references/architecture/patterns.md`, `languages/rust-core.md`, `domain/vala-architecture.md`, `olap-serving.md`, `datafusion.md`, `analytical-operations-reliability.md`; `wyrd-security-posture.md` peer/tenant boundary | PASS except STAND-002, STAND-003 |
| Server HTTP/gRPC/MCP, Oracle peer authority, scheduled query, CLI | `AGENTS.md` §§2–4, 9, 11, 16; `agent-rules.md` audit, test-only and import rules; `wyrd-design.md`, `bifrost-design.md`, `wyrd-security-posture.md`; `references/languages/agent-harness.md`, `errors.md`, `testing-workflows.md` | PASS except STAND-003 |
| Rust client and SDK, Python/PyO3/stubs, TypeScript/N-API/declarations | `AGENTS.md` §§2–4, 7–9, 11; `agent-rules.md` generated artifacts; `bifrost-design.md` public surface; `references/languages/pyo3-boundaries.md`, `python-api-and-stubs.md`, `typescript-guide.md`, `errors.md`, `testing-workflows.md` | PASS |
| Rust, Python, TypeScript, MCP, and server journey tests and test harness | `AGENTS.md` §§11, 16; `agent-rules.md` test ownership and exact commands; `references/languages/testing-workflows.md`, `spec-driven-development.md`; `bifrost-design.md` observable terminals | PASS except STAND-001, STAND-002, STAND-003 |
| Bifrost architecture and docs; `mise.toml`; approved independent skill edit `e15c610af` | `AGENTS.md` §§1, 11–16; `architecture/references/README.md`; `bifrost-design.md`; `references/languages/implementation-execution.md`, `testing-workflows.md`, `spec-driven-development.md` | PASS |

## Rule results

| Rule | Source evidence | Result |
|---|---|---|
| Core/server/client ownership and one public query contract | `crates/wyrd-spec/src/vala/api.rs`, `crates/vala/vala-bifrost-redux/src/oracle/live.rs`, `crates/shared/wyrd-client/src/bifrost/query.rs`, server and SDK projections; no added client-tier heavy dependency or Python in `wyrd-spec` | PASS |
| Typed public contract and generated projections | Updated schema/proto/stubs/declarations; reported `codegen:check`, `ts:napi:check`, `py:typecheck`, served OpenAPI integration lane passed | PASS |
| Test-only hooks remain behind a test feature | `wyrd-server/src/oracle/peer_service.rs:44–75` and `vala-bifrost-redux/src/scribe/tail_rpc.rs:374–457` use `#[cfg(feature = "test-support")]` | PASS |
| Required user journeys, owner runtime, and repository test lanes | Oracle, Scribe, Drift, Rust/Python/TypeScript, MCP and server journeys present; task evidence reports `verify:bifrost`, full gate, format/lint and targeted lanes passed | PASS as a repository test-topology rule; test adequacy for acceptance is outside this review |
| Rustdoc on every new or materially changed Rust item; `# Errors` on fallible items | Several new fallible journey functions document intent but omit the mandatory `# Errors` section (STAND-001) | FAIL |
| All `use` statements at module top, except the documented narrow exception | New function-local imports in production and tests (STAND-002) | FAIL |
| Bare imported names in Rust signatures and fields | New fully qualified types in signatures (STAND-003) | FAIL |
| No check circumvention or forbidden production `#[allow]` | Diff contains no added production suppression; test-only hooks are feature gated; reported gate is green | PASS |

## Material findings

### STAND-001 — Missing mandatory `# Errors` on added fallible Rust tests and helper

**Rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` Rust documentation rule require a `# Errors` section for every fallible Rust function, including tests and test helpers. **Location:** `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs:974,1087,1296,1447,1655` and `crates/wyrd/wyrd-testing/tests/bifrost/oracle/support.rs:324`. These new `Result<_, JourneyError>` functions have rustdoc but no `# Errors` heading. **Consequence:** the repository's mandatory documentation contract is incomplete for the new journey logic, despite green compilation. **Testable correction:** add concise `# Errors` sections describing the setup, query, and assertion failure conditions to each changed function; source inspection is the focused proof, followed by `mise run fmt` and the Oracle journey lane.

### STAND-002 — Added function-local `use` statements

**Rule:** `architecture/agent-rules.md` says all imports live at the top of a module; the only function exception is `use TraitName as _` inside one generic function. **Location:** production `crates/vala/vala-bifrost-redux/src/oracle/live.rs:404` imports both `Transformed` and `TreeNode as _` inside `LiveUnionBoundary::optimize`; `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs:1088` imports process-cluster types inside a test; `crates/wyrd-spec/src/vala/api.rs:1024–1025` imports enum variants inside a test; `crates/vala/vala-bifrost-redux/src/scribe/tail_rpc.rs:1296` imports `HotAuthority` inside a test. The diff adds these sites. **Consequence:** the module dependency manifest is incomplete and the changed code violates the mandatory import layout. **Testable correction:** move imports to each owning module or test module's top import block; run `mise run fmt` and `mise run lints`.

### STAND-003 — Added qualified types in Rust signatures

**Rule:** `architecture/agent-rules.md` requires bringing types into scope with `use` and using bare names in fields, parameters, and return types. **Location:** `crates/vala/vala-bifrost-redux/src/oracle/bindings.rs:248` returns `datafusion::error::Result<&LiveDispatch>`; `crates/vala/vala-bifrost-redux/src/oracle/live.rs:575` takes `chrono::DateTime<chrono::Utc>` and returns `std::time::Duration`; `crates/wyrd/wyrd-testing/tests/bifrost/forge/public_support.rs:314` returns `wyrd_spec::vala::api::BifrostQueryRequest`; the new Oracle test helper at `crates/vala/vala-bifrost-redux/src/oracle/exec.rs:6690` uses two qualified types. **Consequence:** changed signatures obscure the dependency owner and violate the required Rust source shape. **Testable correction:** import these types at the owning module top and use their bare names in the changed signatures; run `mise run fmt` and `mise run lints`.

## Verification limits and result

I inspected the cumulative diff, changed source and callers where needed for these rule claims, the routed authorities, and the verification evidence recorded in the task. I did not rerun the reported full gate; this is an independent static standards audit. The independent skill edit is an approved exception to task scope and its mirror/check evidence is recorded as passing. No standards conclusion here decides whether the task behavior meets the specification.

**Overall: FAIL.** The three findings are explicit mandatory repository-rule violations in added code.
