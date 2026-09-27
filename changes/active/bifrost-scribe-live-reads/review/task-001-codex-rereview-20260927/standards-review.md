# TASK-001 cumulative repository standards review

Subject: repository `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`; base `d1ec13200d332745af2fed8069a21d5b5c39cb47`; immutable candidate `f1f1d5ebd264e8f9ac861ec79c6340da44a7a1a8`. Reviewed the cumulative diff (119 files), surrounding owners and tests, not just the R1 commits. The approved skill edit `e15c610af` is separately authorized and is not query code. Candidate HEAD and worktree were clean at inspection.

## Authority coverage

| Changed surface | Applicable authority |
|---|---|
| `wyrd-spec` API, schemas, tonic protobuf/conversions, client facade | `AGENTS.md` §§2–4, 8–9, 16; `agent-rules.md` import, generated artifact and documentation rules; `wyrd-design.md`; `wyrd-doctrine.mdx`; `bifrost-design.md`; references `architecture/patterns`, `languages/rust-core`, `languages/errors`, `languages/testing-workflows` |
| Oracle planning, execution, discovery, dispatcher, follower, stream | `AGENTS.md` §§2–6, 9–11, 16; `agent-rules.md` struct, import, async and rustdoc rules; `bifrost-design.md`; `wyrd-security-posture.md`; references `domain/vala-architecture`, `domain/olap-serving`, `domain/datafusion`, `domain/analytical-operations-reliability`, `languages/rust-core`, `languages/errors` |
| Scribe live producer, staged Parquet, server peer/listing/auth | Same Rust, server, security and Bifrost authorities; references `domain/iceberg`, `domain/arrow-analytical-interop`, `domain/analytical-operations-reliability` |
| CLI, MCP, HTTP/OpenAPI, scheduled Drift | `AGENTS.md` §§2, 9–11, 16; `wyrd-design.md`, `wyrd-doctrine.mdx`, `bifrost-design.md`; references `languages/agent-harness`, `languages/errors`, `domain/drift-monitoring`, `languages/testing-workflows` |
| Python SDK and PyO3/stubs; TypeScript SDK and declarations | `AGENTS.md` §§2–3, 7–8, 11; `agent-rules.md` generated artifact rule; references `languages/pyo3-boundaries`, `languages/python-api-and-stubs`, `languages/typescript-guide`, `languages/errors`, `languages/testing-workflows` |
| Rust/Python/TypeScript journeys and test hooks, `mise.toml` | `AGENTS.md` §§11–12, 16; `agent-rules.md` test placement/gates; references `languages/testing-workflows`, `languages/implementation-execution`, `languages/spec-driven-development` |
| Bifrost docs, task/review evidence, approved skill mirror | `AGENTS.md` §§11–12, 14–16; `agent-rules.md` no task references in product code; `bifrost-design.md`; references `languages/spec-driven-development`, `languages/implementation-execution` |

## Rule results

| Applicable rule | Result and source evidence |
|---|---|
| Contract owner and first-class client surfaces (`AGENTS.md` §§2–3, 8–9) | PASS: request/terminal types in `wyrd-spec/src/vala/api.rs`, wire conversion in `wyrd-tonic`, shared `wyrd-client::Bifrost`, and Python/TypeScript projections; no SDK-owned durable query implementation in the diff. |
| Typed errors at service and wire boundaries (`AGENTS.md` §§4, 9; `languages/errors`) | PASS as a repository-shape rule: R1 carries `TailReadError` and `FollowerResolutionError` classes through private boundaries; public query errors remain typed. Behavioral classification is assigned to task/domain reviewers. |
| Tenant/security boundaries (`AGENTS.md` §§2, 9; `wyrd-security-posture.md`) | PASS on inspected shape: signed peer ticket and bound tenant/table/query stay in Scribe listing and fragment paths; no new public bypass or raw tenant selector. Security semantics receive separate domain review. |
| Struct ownership and async blocking (`AGENTS.md` §§5–6; `agent-rules.md`) | PASS on inspected changed owners: `Oracle`, `LiveDispatch`, `FetchLiveTailService`, `StagedRunWindows` own workflows; staged file open/decode now uses `spawn_blocking` one window per pull. Concurrency semantics receive separate domain review. |
| Module-top imports (`agent-rules.md`: all `use` at top of owning module; test modules have their own top block) | **FAIL**: new `PartitionStream::execute` imports `StreamExt` inside the function at `oracle/follower.rs:1171`; changed test `projected_leaf_union_preserves_predicate_and_tenant_columns` adds `TreeNode` at `oracle/exec.rs:6730`. Prior cited imports were moved, but these remain. |
| Bare imported types in fields and signatures (`agent-rules.md`) | **FAIL**: new `oracle/tail_discovery.rs:11–13,26–29,42` uses qualified spec and error types in fields/trait/function signatures instead of module imports and bare names. The same file uses only `use super::*`, obscuring its actual dependencies. Other newly changed signatures such as `oracle/live.rs::source_key` also use qualified module paths. |
| Meaningful accurate rustdoc on new/modified Rust items (`AGENTS.md` §16; `agent-rules.md`) | **FAIL**: `oracle/tail_discovery.rs:21–29` documents `BifrostError::QueryVisibilityUnavailable` but `discover` returns `TailReadError`; the new failure classes make that distinction operationally important. R1 added the formerly missing journey `# Errors` sections. |
| Python/TypeScript boundary and generated outputs (`AGENTS.md` §§7–8; `agent-rules.md`) | PASS: PyO3 remains in SDK owner, Python exports/stubs and TypeScript declarations change with the Rust request shape; recorded `codegen:check` passed. |
| Test placement/journey and exact verification (`AGENTS.md` §11; `agent-rules.md`) | PASS for repository workflow: real server journeys exist for Rust, Python, TypeScript, MCP/HTTP; R1 evidence records exact focused nextest commands. `verify:bifrost` reports 9/9 and full gate 48/48 sequential lanes. These are recorded results, not independently rerun here. |
| Generated schemas/protobuf, docs, tooling checks (`AGENTS.md` §§8, 11–12; `agent-rules.md`) | PASS by diff and recorded gate: source and generated artifacts change together; `codegen:check`, `check:skills-sync`, format, lint, and boundary checks are reported green; `git diff --check` is clean. The skill edit is approved and synced to the Claude mirror. |

## Material findings

### REPO-R2-1 — New function-local imports remain

Rule: `architecture/agent-rules.md`, “All `use` statements live at the top of the module.” Exact locations: `crates/vala/vala-bifrost-redux/src/oracle/follower.rs:1170–1171` (new production function) and `src/oracle/exec.rs:6728–6734` (changed test function; its test module already has a top import block). The dependency is hidden in a function despite the R1 style remediation. Move each needed import to its owning module’s top `use` block; inspect the other existing local imports only where their containing symbols were materially changed. `mise run fmt`/`mise run lints` do not enforce this rule, so close by source inspection and the normal checks.

### REPO-R2-2 — New discovery signatures still use qualified types

Rule: `architecture/agent-rules.md`, “Bring types in with `use` and use bare names in signatures,” explicitly including struct fields, trait bounds, parameters, and return types. Exact location: `crates/vala/vala-bifrost-redux/src/oracle/tail_discovery.rs:6–43`, especially fields `time_partition`/`stream`, `discover` input and error, and `wire_binding` output. `oracle/live.rs::source_key` and new `scribe/tail_rpc.rs` test-support signatures show the same pattern. This defeats the prescribed module dependency manifest and leaves the R1 structural finding incompletely closed. Import the named types at the module top and use bare type names in these newly introduced/modified signatures; no behavior change or helper is needed. Verify by source inspection plus normal format/lint.

### REPO-R2-3 — Discovery rustdoc names the wrong error contract

Rule: `AGENTS.md` §16 requires rustdoc to describe the actual operation and error conditions; `agent-rules.md` calls missing or placeholder rustdoc a blocker. `crates/vala/vala-bifrost-redux/src/oracle/tail_discovery.rs:21–29` claims `discover` returns `BifrostError::QueryVisibilityUnavailable`, while the return type is `TailReadError` and the implementation now preserves authorization, binding, deadline, state and unavailable classes. A maintainer following the doc could collapse failure classes again, reversing R1. Describe the actual `TailReadError` outcomes in the existing `# Errors` section; source inspection suffices.

Overall: **FAIL**. The failures are source-shape/documentation rules that green compiler and gate lanes do not enforce. No task acceptance verdict is expressed here.
